use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use desafinado_features::evals::baseline::{RunBaseline, SummaryBaseline};
use desafinado_features::evals::command::{EvalCommandInput, build_eval_args};
use desafinado_features::evals::compare::{StoredRun, compare_summaries, summary_of_stored};
use desafinado_features::evals::limits::{
    FIRST_SIDE_NOTE, LIMIT_TOO_SMALL, NOTHING_LEFT_NOTE, SECOND_SIDE_NOTE, first_run_limit,
    second_run_limit,
};
use desafinado_features::evals::outcome::{
    OutcomeInput, RunOutcome, cost_limit_message, map_run_outcome,
};
use desafinado_features::evals::risks::{Risk, RiskInput, find_risks};
use desafinado_features::evals::settings::check_eval_settings;
use desafinado_features::evals::types::{
    EvalCancelResult, EvalOpenReportRequest, EvalOpenReportResult, EvalPrepareRequest,
    EvalPrepareResult, EvalRunRequest, EvalRunResult, EvalSkillRequest, EvalStatus, EvalsKit,
    PreparedReady, ReportPart, StoppedRun,
};
use desafinado_features::history::store::HistoryStore;
use desafinado_features::history::types::{VersionId, VersionList};

use super::results::{NextRun, ResultsStore, skill_key_of};
use super::runner::{EvalExit, EvalJob, EvalRunner};
use super::stage::Stage;
use crate::library::SKILL_FILE_NAME;

const SHUTDOWN_WAIT: Duration = Duration::from_secs(6);
const INVALID: &str = "Invalid request";
const NOT_A_SKILL: &str = "Not a Skill in the Library";
const VERSION_GONE: &str = "That Version is not in this Skill's History";
const COULD_NOT_RUN: &str = "Could not run claude";
const COULD_NOT_STAGE: &str = "Could not stage the Skill";

type StageFn = dyn Fn(&Path, Option<&str>) -> io::Result<Stage> + Send + Sync;

pub struct EvalDeps {
    pub library_skill_paths: Box<dyn Fn() -> Vec<String> + Send + Sync>,
    pub find_claude: Box<dyn Fn() -> Option<PathBuf> + Send + Sync>,
    pub runner: EvalRunner,
    pub stage: Box<StageFn>,
    pub count_evals: Box<dyn Fn(&Path) -> usize + Send + Sync>,
    pub results: ResultsStore,
    pub history: Arc<dyn HistoryStore>,
    pub open_path: Box<dyn Fn(&Path) -> bool + Send + Sync>,
}

#[derive(Clone)]
struct Prepared {
    risks: Vec<Risk>,
    baseline_risks: Option<Vec<Risk>>,
    baseline: RunBaseline,
    claude_version: String,
}

#[derive(Default)]
struct State {
    prepared: HashMap<String, Prepared>,
    prepare_sequence: HashMap<String, u64>,
    stopped: HashMap<String, StoppedRun>,
    running: bool,
    running_key: Option<String>,
    running_real: Option<String>,
    committing: bool,
    abort: Arc<AtomicBool>,
}

enum Resolved {
    None,
    Version {
        version_id: VersionId,
        at: i64,
        text: String,
    },
}

struct SkillHistory {
    file: String,
    versions: VersionList,
}

struct Stages {
    today: Stage,
    older: Option<(Stage, VersionId, i64)>,
}

impl Stages {
    fn cleanup(&self) {
        let _ = self.today.cleanup();
        if let Some((older, ..)) = &self.older {
            let _ = older.cleanup();
        }
    }

    fn risks(&self) -> (Vec<Risk>, Option<Vec<Risk>>) {
        (
            stage_risks(&self.today),
            self.older.as_ref().map(|(older, ..)| stage_risks(older)),
        )
    }
}

struct RunContext<'a> {
    key: &'a str,
    executable: &'a Path,
    request: &'a EvalRunRequest,
    claude_version: &'a str,
    abort: &'a AtomicBool,
}

struct Arm<'a> {
    stage: &'a Stage,
    files: NextRun,
    part: Option<ReportPart>,
    limit: f64,
    ablation_none: bool,
    limit_note: Option<&'static str>,
}

fn stage_risks(stage: &Stage) -> Vec<Risk> {
    find_risks(&RiskInput {
        skill_text: stage.skill_text.clone(),
        left_out: stage.left_out.clone(),
    })
}

fn failed(reason: &str) -> EvalRunResult {
    EvalRunResult::Failed {
        reason: reason.to_owned(),
    }
}

fn cancelled(cost_usd: Option<f64>) -> EvalRunResult {
    EvalRunResult::NotComplete {
        message: "Cancelled".to_owned(),
        cost_usd,
    }
}

pub fn signal_name(signal: i32) -> String {
    match signal {
        libc::SIGHUP => "SIGHUP".into(),
        libc::SIGINT => "SIGINT".into(),
        libc::SIGKILL => "SIGKILL".into(),
        libc::SIGTERM => "SIGTERM".into(),
        libc::SIGABRT => "SIGABRT".into(),
        libc::SIGSEGV => "SIGSEGV".into(),
        libc::SIGPIPE => "SIGPIPE".into(),
        other => format!("signal {other}"),
    }
}

fn outcome_of(
    exit: EvalExit,
    report_text: Option<String>,
    context: &RunContext,
    limit_note: Option<&str>,
) -> RunOutcome {
    map_run_outcome(&OutcomeInput {
        report_text,
        exit_code: exit.exit_code,
        signal: exit.signal.map(signal_name),
        cancelled: context.abort.load(Ordering::SeqCst),
        max_cost_usd: context.request.settings.max_cost_usd,
        cli_version: Some(context.claude_version.to_owned()),
        limit_note: limit_note.map(str::to_owned),
    })
}

fn summary_or_stop(outcome: RunOutcome) -> Result<EvalSummaryOf, Box<EvalRunResult>> {
    match outcome {
        RunOutcome::Complete { summary } => Ok(summary),
        RunOutcome::NotComplete { message, cost_usd } => {
            Err(Box::new(EvalRunResult::NotComplete { message, cost_usd }))
        }
    }
}

type EvalSummaryOf = desafinado_features::evals::report::EvalSummary;

pub struct EvalService {
    deps: EvalDeps,
    state: Mutex<State>,
    finished: Condvar,
}

impl EvalService {
    pub fn new(deps: EvalDeps) -> Self {
        Self {
            deps,
            state: Mutex::default(),
            finished: Condvar::new(),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn library_real_path(&self, path: &str) -> Option<String> {
        if path.is_empty() || path.contains('\0') {
            return None;
        }
        let real = fs::canonicalize(path).ok()?.to_string_lossy().into_owned();
        (self.deps.library_skill_paths)()
            .contains(&real)
            .then_some(real)
    }

    fn claude(&self) -> Result<(PathBuf, String), Option<&'static str>> {
        let executable = (self.deps.find_claude)().ok_or(None)?;
        let version = self
            .deps
            .runner
            .version(&executable)
            .ok_or(Some(COULD_NOT_RUN))?;
        Ok((executable, version))
    }

    fn last_summary(&self, key: &str) -> Option<EvalSummaryOf> {
        let last = self.deps.results.read_last(key)?;
        summary_of_stored(&StoredRun {
            baseline_text: last.baseline_text,
            first_text: last.first_text,
            second_text: last.second_text,
        })
    }

    fn history_of(&self, real: &str) -> Option<SkillHistory> {
        let file = fs::canonicalize(Path::new(real).join(SKILL_FILE_NAME))
            .ok()?
            .to_string_lossy()
            .into_owned();
        let versions = self.deps.history.list(&file).ok()?;
        Some(SkillHistory { file, versions })
    }

    fn resolve_baseline(
        &self,
        history: Option<&SkillHistory>,
        baseline: RunBaseline,
    ) -> Option<Resolved> {
        let RunBaseline::Version { version_id } = baseline else {
            return Some(Resolved::None);
        };
        let history = history?;
        let found = history.versions.iter().find(|v| v.id == version_id)?;
        let text = self
            .deps
            .history
            .text(&history.file, version_id)
            .ok()
            .flatten()?;
        Some(Resolved::Version {
            version_id,
            at: found.at,
            text,
        })
    }

    fn same_as_saved(&self, history: Option<&SkillHistory>, saved: &str) -> Vec<VersionId> {
        history.map_or_else(Vec::new, |history| {
            history
                .versions
                .iter()
                .filter(|version| {
                    self.deps
                        .history
                        .text(&history.file, version.id)
                        .ok()
                        .flatten()
                        .as_deref()
                        == Some(saved)
                })
                .map(|version| version.id)
                .collect()
        })
    }

    fn open_stages(&self, real: &str, baseline: &Resolved) -> Option<Stages> {
        let today = (self.deps.stage)(Path::new(real), None).ok()?;
        let older = match baseline {
            Resolved::None => None,
            Resolved::Version {
                version_id,
                at,
                text,
            } => match (self.deps.stage)(Path::new(real), Some(text)) {
                Ok(older) => Some((older, *version_id, *at)),
                Err(_) => {
                    let _ = today.cleanup();
                    return None;
                }
            },
        };
        Some(Stages { today, older })
    }

    fn run_arm(&self, context: &RunContext, arm: Arm) -> io::Result<RunOutcome> {
        let args = build_eval_args(&EvalCommandInput {
            runs: context.request.settings.runs,
            model: context.request.settings.model.clone(),
            max_cost_usd: arm.limit,
            json_file: arm.files.json_file.to_string_lossy().into_owned(),
            output_dir: arm.files.output_dir.to_string_lossy().into_owned(),
            ablation_none: arm.ablation_none,
        });
        if context.abort.load(Ordering::SeqCst) {
            return Ok(RunOutcome::NotComplete {
                message: "Cancelled".into(),
                cost_usd: None,
            });
        }
        let exit = self.deps.runner.run(
            &EvalJob {
                executable: context.executable.to_path_buf(),
                args,
                cwd: arm.stage.dir.clone(),
            },
            &mut |_| {},
        )?;
        let report = self.deps.results.read_next(context.key, arm.part);
        Ok(outcome_of(exit, report, context, arm.limit_note))
    }

    fn commit(&self, key: &str) -> io::Result<()> {
        self.state().committing = true;
        self.deps.results.promote(key)
    }

    fn run_single(&self, context: &RunContext, stages: &Stages) -> io::Result<EvalRunResult> {
        let files = self.deps.results.begin(context.key)?;
        let outcome = self.run_arm(
            context,
            Arm {
                stage: &stages.today,
                files,
                part: None,
                limit: context.request.settings.max_cost_usd,
                ablation_none: false,
                limit_note: None,
            },
        )?;
        let summary = match summary_or_stop(outcome) {
            Ok(summary) => summary,
            Err(stopped) => return Ok(*stopped),
        };
        self.commit(context.key)?;
        Ok(EvalRunResult::Completed { summary })
    }

    fn run_pair(
        &self,
        context: &RunContext,
        today: &Stage,
        older: &Stage,
        version_id: VersionId,
        at: i64,
    ) -> io::Result<EvalRunResult> {
        let key = context.key;
        let max = context.request.settings.max_cost_usd;
        let Some(first_limit) = first_run_limit(max) else {
            return Ok(failed(LIMIT_TOO_SMALL));
        };
        self.deps.results.begin(key)?;
        let first = self.run_arm(
            context,
            Arm {
                stage: today,
                files: self.deps.results.begin_part(key, ReportPart::Today)?,
                part: Some(ReportPart::Today),
                limit: first_limit,
                ablation_none: true,
                limit_note: Some(FIRST_SIDE_NOTE),
            },
        )?;
        let first = match summary_or_stop(first) {
            Ok(summary) => summary,
            Err(stopped) => return Ok(*stopped),
        };
        let spent = first.cost_usd;
        if context.abort.load(Ordering::SeqCst) {
            return Ok(cancelled(Some(spent)));
        }
        let Some(second_limit) = second_run_limit(max, spent) else {
            return Ok(EvalRunResult::NotComplete {
                message: cost_limit_message(max, Some(NOTHING_LEFT_NOTE)),
                cost_usd: Some(spent),
            });
        };
        let second = self.run_arm(
            context,
            Arm {
                stage: older,
                files: self.deps.results.begin_part(key, ReportPart::Version)?,
                part: Some(ReportPart::Version),
                limit: second_limit,
                ablation_none: true,
                limit_note: Some(SECOND_SIDE_NOTE),
            },
        )?;
        let second = match second {
            RunOutcome::Complete { summary } => summary,
            RunOutcome::NotComplete { message, cost_usd } => {
                return Ok(EvalRunResult::NotComplete {
                    message,
                    cost_usd: Some(spent + cost_usd.unwrap_or(0.0)),
                });
            }
        };
        let stored = SummaryBaseline::Version { version_id, at };
        let summary = compare_summaries(&first, &second, stored);
        let stored_text = serde_json::json!({"kind": "version", "versionId": version_id, "at": at});
        self.deps
            .results
            .write_baseline(key, &stored_text.to_string())?;
        if context.abort.load(Ordering::SeqCst) {
            return Ok(cancelled(Some(summary.cost_usd)));
        }
        self.commit(key)?;
        Ok(EvalRunResult::Completed { summary })
    }

    fn execute(
        &self,
        real: &str,
        key: &str,
        executable: &Path,
        request: &EvalRunRequest,
        abort: &AtomicBool,
    ) -> EvalRunResult {
        let aborted = || abort.load(Ordering::SeqCst);
        let remembered = self.state().prepared.remove(key);
        let history = self.history_of(real);
        let Some(baseline) = self.resolve_baseline(history.as_ref(), request.baseline) else {
            return if aborted() {
                cancelled(None)
            } else {
                failed(VERSION_GONE)
            };
        };
        if matches!(baseline, Resolved::Version { .. })
            && first_run_limit(request.settings.max_cost_usd).is_none()
        {
            return failed(LIMIT_TOO_SMALL);
        }
        let Some(stages) = self.open_stages(real, &baseline) else {
            return if aborted() {
                cancelled(None)
            } else {
                failed(COULD_NOT_STAGE)
            };
        };
        let result = (|| {
            if aborted() {
                return cancelled(None);
            }
            let (risks, baseline_risks) = stages.risks();
            let Some(remembered) = remembered.filter(|remembered| {
                remembered.baseline == request.baseline
                    && remembered.risks == risks
                    && remembered.baseline_risks == baseline_risks
            }) else {
                return EvalRunResult::RisksChanged;
            };
            let context = RunContext {
                key,
                executable,
                request,
                claude_version: &remembered.claude_version,
                abort,
            };
            let ran = match &stages.older {
                None => self.run_single(&context, &stages),
                Some((older, version_id, at)) => {
                    self.run_pair(&context, &stages.today, older, *version_id, *at)
                }
            };
            ran.unwrap_or_else(|_| {
                if aborted() {
                    cancelled(None)
                } else {
                    failed(COULD_NOT_RUN)
                }
            })
        })();
        stages.cleanup();
        let _ = self.deps.results.discard(key);
        result
    }

    fn stop_run(&self) -> bool {
        let state = self.state();
        if !state.running || state.committing {
            return false;
        }
        state.abort.store(true, Ordering::SeqCst);
        drop(state);
        self.deps.runner.kill();
        true
    }

    pub fn is_running(&self) -> bool {
        self.state().running
    }

    pub fn shutdown(&self) {
        if !self.is_running() {
            return;
        }
        self.stop_run();
        let state = self.state();
        let _ = self
            .finished
            .wait_timeout_while(state, SHUTDOWN_WAIT, |state| state.running);
    }

    fn begin_run(&self) -> Option<Arc<AtomicBool>> {
        let mut state = self.state();
        if state.running {
            return None;
        }
        state.running = true;
        state.committing = false;
        state.running_key = None;
        state.running_real = None;
        state.abort = Arc::new(AtomicBool::new(false));
        Some(state.abort.clone())
    }

    fn end_run(&self) {
        let mut state = self.state();
        state.running = false;
        state.running_key = None;
        state.running_real = None;
        drop(state);
        self.finished.notify_all();
    }

    fn run_started(&self, request: &EvalRunRequest, abort: &AtomicBool) -> EvalRunResult {
        let real = self.library_real_path(&request.skill_path);
        if abort.load(Ordering::SeqCst) {
            return cancelled(None);
        }
        let Some(real) = real else {
            return failed(NOT_A_SKILL);
        };
        let key = skill_key_of(&real);
        {
            let mut state = self.state();
            state.running_key = Some(key.clone());
            state.running_real = Some(real.clone());
            state.stopped.remove(&key);
        }
        let executable = (self.deps.find_claude)();
        if abort.load(Ordering::SeqCst) {
            return cancelled(None);
        }
        let Some(executable) = executable else {
            return EvalRunResult::NoCli;
        };
        let result = self.execute(&real, &key, &executable, request, abort);
        if let EvalRunResult::NotComplete { message, cost_usd } = &result {
            self.state().stopped.insert(
                key,
                StoppedRun {
                    message: message.clone(),
                    cost_usd: *cost_usd,
                },
            );
        }
        result
    }
}

fn valid_run(request: &EvalRunRequest) -> bool {
    !request.skill_path.is_empty()
        && check_eval_settings(&request.settings).is_empty()
        && match request.baseline {
            RunBaseline::None => true,
            RunBaseline::Version { version_id } => version_id >= 1,
        }
}

impl EvalsKit for EvalService {
    fn status(&self, request: &EvalSkillRequest) -> EvalStatus {
        let Some(real) = self.library_real_path(&request.skill_path) else {
            return EvalStatus::Failed {
                reason: NOT_A_SKILL.into(),
            };
        };
        let claude_version = match self.claude() {
            Ok((_, version)) => version,
            Err(None) => return EvalStatus::NoCli,
            Err(Some(reason)) => {
                return EvalStatus::Failed {
                    reason: reason.into(),
                };
            }
        };
        let key = skill_key_of(&real);
        let case_count = (self.deps.count_evals)(Path::new(&real));
        let last = self.last_summary(&key);
        let state = self.state();
        let here = state.running && state.running_key.as_deref() == Some(key.as_str());
        EvalStatus::Ready {
            claude_version,
            case_count,
            last,
            running: here,
            cancelling: here && state.abort.load(Ordering::SeqCst),
            running_skill: if state.running {
                state.running_real.clone()
            } else {
                None
            },
            stopped: state.stopped.get(&key).cloned(),
        }
    }

    fn prepare(&self, request: &EvalPrepareRequest) -> EvalPrepareResult {
        if matches!(request.baseline, RunBaseline::Version { version_id } if version_id < 1) {
            return EvalPrepareResult::Failed {
                reason: INVALID.into(),
            };
        }
        let Some(real) = self.library_real_path(&request.skill_path) else {
            return EvalPrepareResult::Failed {
                reason: NOT_A_SKILL.into(),
            };
        };
        let key = skill_key_of(&real);
        let sequence = {
            let mut state = self.state();
            let next = state.prepare_sequence.get(&key).copied().unwrap_or(0) + 1;
            state.prepare_sequence.insert(key.clone(), next);
            next
        };
        let claude_version = match self.claude() {
            Ok((_, version)) => version,
            Err(None) => return EvalPrepareResult::NoCli,
            Err(Some(reason)) => {
                return EvalPrepareResult::Failed {
                    reason: reason.into(),
                };
            }
        };
        let history = self.history_of(&real);
        let Some(resolved) = self.resolve_baseline(history.as_ref(), request.baseline) else {
            return EvalPrepareResult::Failed {
                reason: VERSION_GONE.into(),
            };
        };
        let Some(stages) = self.open_stages(&real, &resolved) else {
            return EvalPrepareResult::Failed {
                reason: COULD_NOT_STAGE.into(),
            };
        };
        let (risks, baseline_risks) = stages.risks();
        {
            let mut state = self.state();
            if state.prepare_sequence.get(&key) == Some(&sequence) {
                state.prepared.insert(
                    key,
                    Prepared {
                        risks: risks.clone(),
                        baseline_risks: baseline_risks.clone(),
                        baseline: request.baseline,
                        claude_version: claude_version.clone(),
                    },
                );
            }
        }
        let result = EvalPrepareResult::Ready(PreparedReady {
            claude_version,
            case_count: stages.today.case_count,
            risks,
            baseline_risks,
            versions: history
                .as_ref()
                .map(|history| history.versions.clone())
                .unwrap_or_default(),
            same_as_saved: self.same_as_saved(history.as_ref(), &stages.today.skill_text),
        });
        stages.cleanup();
        result
    }

    fn run(&self, request: &EvalRunRequest) -> EvalRunResult {
        if !valid_run(request) {
            return failed(INVALID);
        }
        let Some(abort) = self.begin_run() else {
            return EvalRunResult::Busy;
        };
        let result = self.run_started(request, &abort);
        self.end_run();
        result
    }

    fn open_report(&self, request: &EvalOpenReportRequest) -> EvalOpenReportResult {
        let Some(real) = self.library_real_path(&request.skill_path) else {
            return EvalOpenReportResult::Failed {
                reason: NOT_A_SKILL.into(),
            };
        };
        let part = request.part.unwrap_or(ReportPart::Today);
        let Some(report) = self.deps.results.last_report(&skill_key_of(&real), part) else {
            return EvalOpenReportResult::Failed {
                reason: "There is no report yet".into(),
            };
        };
        if (self.deps.open_path)(&report) {
            EvalOpenReportResult::Opened
        } else {
            EvalOpenReportResult::Failed {
                reason: "Could not open the report".into(),
            }
        }
    }

    fn cancel(&self) -> EvalCancelResult {
        if self.stop_run() {
            EvalCancelResult::Cancelling
        } else {
            EvalCancelResult::Idle
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use desafinado_features::evals::settings::EvalSettings;
    use desafinado_features::history::memory_store::MemoryHistoryStore;
    use desafinado_features::history::store::RecordInput;
    use desafinado_features::history::types::VersionCause;

    use super::*;
    use crate::evals::runner::RunnerEnv;
    use crate::evals::stage::{count_evals, stage_skill};

    const REPORT: &str = r#"{"schemaVersion":1,"claudeVersion":"2.1.5","startedAt":"2026-11-07T20:03:24.309Z","costUsd":0.2,"partial":false,"suite":{"plugins":[{"name":"pdf"}]},"cases":[{"name":"greet","graders":[],"arms":{"with":[{"passed":true,"error":null,"graders":[]}],"without":[{"passed":false,"error":null,"graders":[]}]}}]}"#;

    const CLAUDE: &str = r#"#!/bin/sh
here=$(dirname "$0")
if [ "$1" = "--version" ]; then echo "2.1.5 (Claude Code)"; exit 0; fi
echo "$*" >> "$here/calls"
if [ -f "$here/hang" ]; then sleep 30; fi
while [ $# -gt 0 ]; do case "$1" in --json) json="$2";; --output-dir) out="$2";; esac; shift; done
cp "$here/report.json" "$json"
mkdir -p "$out" && echo "<html>" > "$out/report.html"
"#;

    struct Env {
        _dir: tempfile::TempDir,
        base: PathBuf,
        skill: PathBuf,
        history: Arc<MemoryHistoryStore>,
        service: Arc<EvalService>,
    }

    fn env_with(claude: bool) -> Env {
        let dir = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();
        let skill = base.join("skills/pdf");
        fs::create_dir_all(skill.join("evals/greet")).unwrap();
        fs::write(skill.join("SKILL.md"), "---\nname: pdf\n---\n# today\n").unwrap();
        fs::write(skill.join("evals/greet/prompt.md"), "Hi").unwrap();
        fs::create_dir_all(base.join("bin")).unwrap();
        fs::create_dir_all(base.join("tmp")).unwrap();
        let executable = base.join("bin/claude");
        fs::write(&executable, CLAUDE).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(base.join("bin/report.json"), REPORT).unwrap();
        let history = Arc::new(MemoryHistoryStore::new());
        let skill_path = skill.to_string_lossy().into_owned();
        let temp = base.join("tmp");
        let service = Arc::new(EvalService::new(EvalDeps {
            library_skill_paths: Box::new(move || vec![skill_path.clone()]),
            find_claude: Box::new(move || claude.then(|| executable.clone())),
            runner: EvalRunner::new(RunnerEnv {
                home: base.to_string_lossy().into_owned(),
                path: std::env::var("PATH").ok(),
                extra: Vec::new(),
            }),
            stage: Box::new(move |path, text| stage_skill(path, &temp, text)),
            count_evals: Box::new(count_evals),
            results: ResultsStore::new(&base.join("data")),
            history: history.clone(),
            open_path: Box::new(|path| path.exists()),
        }));
        Env {
            _dir: dir,
            base,
            skill,
            history,
            service,
        }
    }

    fn path(e: &Env) -> String {
        e.skill.to_string_lossy().into_owned()
    }

    fn status(e: &Env) -> EvalStatus {
        e.service.status(&EvalSkillRequest {
            skill_path: path(e),
        })
    }

    fn prepare(e: &Env, baseline: RunBaseline) -> EvalPrepareResult {
        e.service.prepare(&EvalPrepareRequest {
            skill_path: path(e),
            baseline,
        })
    }

    fn run(e: &Env, baseline: RunBaseline) -> EvalRunResult {
        e.service.run(&EvalRunRequest {
            skill_path: path(e),
            baseline,
            settings: EvalSettings {
                runs: 2.0,
                model: "haiku".into(),
                max_cost_usd: 1.0,
            },
        })
    }

    fn calls(e: &Env) -> Vec<String> {
        fs::read_to_string(e.base.join("bin/calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn says_there_is_no_cli_and_refuses_a_folder_outside_the_library() {
        let e = env_with(false);
        assert_eq!(status(&e), EvalStatus::NoCli);
        let outside = e.service.status(&EvalSkillRequest {
            skill_path: e.base.to_string_lossy().into_owned(),
        });
        assert_eq!(
            outside,
            EvalStatus::Failed {
                reason: NOT_A_SKILL.into()
            }
        );
    }

    #[test]
    fn reports_the_version_and_cases_before_any_run() {
        let e = env_with(true);
        let EvalStatus::Ready {
            claude_version,
            case_count,
            last,
            running,
            ..
        } = status(&e)
        else {
            panic!("not ready");
        };
        assert_eq!(
            (claude_version.as_str(), case_count, last, running),
            ("2.1.5", 1, None, false)
        );
    }

    #[test]
    fn runs_only_after_prepare_and_keeps_the_last_complete_run() {
        let e = env_with(true);
        assert_eq!(run(&e, RunBaseline::None), EvalRunResult::RisksChanged);
        assert!(matches!(
            prepare(&e, RunBaseline::None),
            EvalPrepareResult::Ready(PreparedReady { case_count: 1, .. })
        ));
        let EvalRunResult::Completed { summary } = run(&e, RunBaseline::None) else {
            panic!("not completed");
        };
        assert_eq!(summary.cases.len(), 1);
        assert_eq!(calls(&e).len(), 1);
        assert!(calls(&e)[0].contains("--runs 2"));
        assert!(!calls(&e)[0].contains("--ablation"));
        let EvalStatus::Ready { last, .. } = status(&e) else {
            panic!("not ready");
        };
        assert_eq!(last.unwrap().cases[0].name, "greet");
        assert_eq!(
            e.service.open_report(&EvalOpenReportRequest {
                skill_path: path(&e),
                part: None
            }),
            EvalOpenReportResult::Opened
        );
    }

    #[test]
    fn runs_two_stages_against_a_version_and_splits_the_limit() {
        let e = env_with(true);
        let file = fs::canonicalize(e.skill.join("SKILL.md"))
            .unwrap()
            .to_string_lossy()
            .into_owned();
        e.history
            .record(
                &file,
                RecordInput {
                    at: 5,
                    cause: VersionCause::Edited,
                    text: "---\nname: pdf\n---\n# older\n".into(),
                },
            )
            .unwrap();
        let id = e.history.list(&file).unwrap()[0].id;
        let baseline = RunBaseline::Version { version_id: id };
        assert!(matches!(
            prepare(&e, baseline),
            EvalPrepareResult::Ready(PreparedReady {
                baseline_risks: Some(_),
                ..
            })
        ));
        let EvalRunResult::Completed { summary } = run(&e, baseline) else {
            panic!("not completed");
        };
        assert_eq!(
            summary.baseline,
            SummaryBaseline::Version {
                version_id: id,
                at: 5
            }
        );
        let calls = calls(&e);
        assert_eq!(calls.len(), 2);
        assert!(calls[0].contains("--max-cost-usd 0.5") && calls[0].contains("--ablation none"));
        assert!(calls[1].contains("--max-cost-usd 0.8"));
        assert!(matches!(
            status(&e),
            EvalStatus::Ready { last: Some(_), .. }
        ));
    }

    #[test]
    fn cancels_a_running_run_and_remembers_why_it_stopped() {
        let e = env_with(true);
        fs::write(e.base.join("bin/hang"), "").unwrap();
        prepare(&e, RunBaseline::None);
        let service = e.service.clone();
        let skill_path = path(&e);
        let handle = std::thread::spawn(move || {
            service.run(&EvalRunRequest {
                skill_path,
                baseline: RunBaseline::None,
                settings: EvalSettings {
                    runs: 1.0,
                    model: "haiku".into(),
                    max_cost_usd: 1.0,
                },
            })
        });
        while calls(&e).is_empty() {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(matches!(
            status(&e),
            EvalStatus::Ready {
                running: true,
                cancelling: false,
                ..
            }
        ));
        assert_eq!(run(&e, RunBaseline::None), EvalRunResult::Busy);
        assert_eq!(e.service.cancel(), EvalCancelResult::Cancelling);
        assert_eq!(
            handle.join().unwrap(),
            EvalRunResult::NotComplete {
                message: "Cancelled".into(),
                cost_usd: None
            }
        );
        let EvalStatus::Ready {
            stopped,
            running,
            last,
            ..
        } = status(&e)
        else {
            panic!("not ready");
        };
        assert_eq!((running, last), (false, None));
        assert_eq!(stopped.unwrap().message, "Cancelled");
        assert_eq!(e.service.cancel(), EvalCancelResult::Idle);
    }

    #[test]
    fn says_a_version_is_gone_and_names_signals() {
        let e = env_with(true);
        assert_eq!(
            prepare(&e, RunBaseline::Version { version_id: 9 }),
            EvalPrepareResult::Failed {
                reason: VERSION_GONE.into()
            }
        );
        assert_eq!(signal_name(libc::SIGTERM), "SIGTERM");
    }
}
