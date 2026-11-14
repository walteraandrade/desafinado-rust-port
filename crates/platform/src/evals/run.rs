use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use super::command::{EvalCommandInput, build_eval_args};
use super::limits::{first_run_limit, second_run_limit};
use super::results::{ResultsStore, RunPart};
use super::runner::{EvalEvent, EvalExit, EvalJob, EvalRunner};

#[derive(Debug, Clone)]
pub struct ArmPlan {
    pub part: Option<RunPart>,
    pub stage_dir: PathBuf,
    pub ablation_none: bool,
}

#[derive(Debug, Clone)]
pub struct EvalRunPlan {
    pub executable: PathBuf,
    pub skill_key: String,
    pub runs: u32,
    pub model: String,
    pub max_cost_usd: f64,
    pub today: PathBuf,
    pub older: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArmResult {
    pub part: Option<RunPart>,
    pub limit_usd: f64,
    pub exit: EvalExit,
    pub report_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvalRunOutcome {
    pub arms: Vec<ArmResult>,
    pub cancelled: bool,
    pub nothing_left_for_second: bool,
    pub spent_first_usd: Option<f64>,
}

#[derive(Debug, thiserror::Error)]
pub enum EvalRunError {
    #[error("{}", super::limits::LIMIT_TOO_SMALL)]
    LimitTooSmall,
    #[error("Could not run claude: {0}")]
    Io(#[from] std::io::Error),
}

pub fn arm_args(
    plan: &EvalRunPlan,
    next: &super::results::NextRun,
    limit: f64,
    ablation: bool,
) -> Vec<String> {
    build_eval_args(&EvalCommandInput {
        runs: plan.runs,
        model: plan.model.clone(),
        max_cost_usd: limit,
        json_file: next.json_file.to_string_lossy().into_owned(),
        output_dir: next.output_dir.to_string_lossy().into_owned(),
        ablation_none: ablation,
    })
}

#[derive(Clone, Copy)]
struct ArmSpec<'a> {
    part: Option<RunPart>,
    stage_dir: &'a Path,
    limit: f64,
    ablation: bool,
}

fn run_arm(
    plan: &EvalRunPlan,
    runner: &EvalRunner,
    results: &ResultsStore,
    spec: &ArmSpec,
    on_event: &mut dyn FnMut(Option<RunPart>, EvalEvent),
) -> Result<ArmResult, EvalRunError> {
    let ArmSpec {
        part,
        stage_dir,
        limit,
        ablation,
    } = *spec;
    let next = match part {
        None => results.begin(&plan.skill_key)?,
        Some(part) => results.begin_part(&plan.skill_key, part)?,
    };
    let job = EvalJob {
        executable: plan.executable.clone(),
        args: arm_args(plan, &next, limit, ablation),
        cwd: stage_dir.to_path_buf(),
    };
    let exit = runner.run(&job, &mut |event| on_event(part, event))?;
    Ok(ArmResult {
        part,
        limit_usd: limit,
        exit,
        report_text: results.read_next(&plan.skill_key, part),
    })
}

pub fn run_eval(
    plan: &EvalRunPlan,
    runner: &EvalRunner,
    results: &ResultsStore,
    cost_of: &dyn Fn(&str) -> Option<f64>,
    cancelled: &AtomicBool,
    on_event: &mut dyn FnMut(Option<RunPart>, EvalEvent),
) -> Result<EvalRunOutcome, EvalRunError> {
    let mut outcome = EvalRunOutcome {
        arms: Vec::new(),
        cancelled: false,
        nothing_left_for_second: false,
        spent_first_usd: None,
    };
    let Some(older) = &plan.older else {
        if cancelled.load(Ordering::SeqCst) {
            outcome.cancelled = true;
            return Ok(outcome);
        }
        let arm = run_arm(
            plan,
            runner,
            results,
            &ArmSpec {
                part: None,
                stage_dir: &plan.today,
                limit: plan.max_cost_usd,
                ablation: false,
            },
            on_event,
        )?;
        outcome.cancelled = cancelled.load(Ordering::SeqCst);
        outcome.arms.push(arm);
        return Ok(outcome);
    };
    let first_limit = first_run_limit(plan.max_cost_usd).ok_or(EvalRunError::LimitTooSmall)?;
    results.begin(&plan.skill_key)?;
    if cancelled.load(Ordering::SeqCst) {
        outcome.cancelled = true;
        return Ok(outcome);
    }
    let first = run_arm(
        plan,
        runner,
        results,
        &ArmSpec {
            part: Some(RunPart::Today),
            stage_dir: &plan.today,
            limit: first_limit,
            ablation: true,
        },
        on_event,
    )?;
    let first_ok = first.exit.exit_code == Some(0) && first.report_text.is_some();
    let spent = first.report_text.as_deref().and_then(cost_of);
    outcome.spent_first_usd = spent;
    outcome.arms.push(first);
    outcome.cancelled = cancelled.load(Ordering::SeqCst);
    if outcome.cancelled || !first_ok {
        return Ok(outcome);
    }
    let Some(second_limit) = second_run_limit(plan.max_cost_usd, spent.unwrap_or(0.0)) else {
        outcome.nothing_left_for_second = true;
        return Ok(outcome);
    };
    let second = run_arm(
        plan,
        runner,
        results,
        &ArmSpec {
            part: Some(RunPart::Version),
            stage_dir: older,
            limit: second_limit,
            ablation: true,
        },
        on_event,
    )?;
    outcome.arms.push(second);
    outcome.cancelled = cancelled.load(Ordering::SeqCst);
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evals::runner::RunnerEnv;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    struct Env {
        _dir: tempfile::TempDir,
        base: PathBuf,
        results: ResultsStore,
        runner: EvalRunner,
    }

    fn env() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();
        fs::create_dir_all(base.join("today")).unwrap();
        fs::create_dir_all(base.join("older")).unwrap();
        Env {
            results: ResultsStore::new(&base.join("data")),
            runner: EvalRunner::new(RunnerEnv {
                home: base.to_string_lossy().into_owned(),
                path: std::env::var("PATH").ok(),
                extra: Vec::new(),
            }),
            base,
            _dir: dir,
        }
    }

    fn fake_claude(e: &Env, body: &str) -> PathBuf {
        let path = e.base.join("claude");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    const WRITE_RESULT: &str = r#"json=""; while [ $# -gt 0 ]; do case "$1" in --json) json="$2";; --max-cost-usd) cost="$2";; esac; shift; done; echo "limit $cost"; echo "{\"cost\":0.2,\"limit\":$cost}" > "$json""#;

    fn plan(e: &Env, executable: PathBuf, older: bool, max_cost: f64) -> EvalRunPlan {
        EvalRunPlan {
            executable,
            skill_key: "key".into(),
            runs: 2,
            model: "haiku".into(),
            max_cost_usd: max_cost,
            today: e.base.join("today"),
            older: older.then(|| e.base.join("older")),
        }
    }

    fn cost(text: &str) -> Option<f64> {
        serde_json::from_str::<serde_json::Value>(text)
            .ok()?
            .get("cost")?
            .as_f64()
    }

    #[test]
    fn runs_one_arm_for_a_run_without_baseline_with_the_full_limit() {
        let e = env();
        let claude = fake_claude(&e, WRITE_RESULT);
        let mut lines = Vec::new();
        let outcome = run_eval(
            &plan(&e, claude, false, 1.0),
            &e.runner,
            &e.results,
            &cost,
            &AtomicBool::new(false),
            &mut |part, event| {
                if let EvalEvent::Output { line, .. } = event {
                    lines.push((part, line));
                }
            },
        )
        .unwrap();
        assert_eq!(outcome.arms.len(), 1);
        assert_eq!(outcome.arms[0].limit_usd, 1.0);
        assert_eq!(outcome.arms[0].exit.exit_code, Some(0));
        assert!(
            outcome.arms[0]
                .report_text
                .as_deref()
                .unwrap()
                .contains("\"limit\":1")
        );
        assert_eq!(lines, [(None, "limit 1".to_owned())]);
    }

    #[test]
    fn runs_the_two_arms_splitting_the_cost_limit() {
        let e = env();
        let claude = fake_claude(&e, WRITE_RESULT);
        let outcome = run_eval(
            &plan(&e, claude, true, 1.0),
            &e.runner,
            &e.results,
            &cost,
            &AtomicBool::new(false),
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(outcome.arms.len(), 2);
        assert_eq!(outcome.arms[0].part, Some(RunPart::Today));
        assert_eq!(outcome.arms[0].limit_usd, 0.5);
        assert_eq!(outcome.spent_first_usd, Some(0.2));
        assert_eq!(outcome.arms[1].part, Some(RunPart::Version));
        assert_eq!(outcome.arms[1].limit_usd, 0.8);
        assert!(!outcome.nothing_left_for_second);
    }

    #[test]
    fn stops_after_the_first_arm_when_nothing_is_left_for_the_second() {
        let e = env();
        let claude = fake_claude(&e, WRITE_RESULT);
        let outcome = run_eval(
            &plan(&e, claude, true, 0.2),
            &e.runner,
            &e.results,
            &cost,
            &AtomicBool::new(false),
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(outcome.arms.len(), 1);
        assert!(outcome.nothing_left_for_second);
    }

    #[test]
    fn does_not_start_the_second_arm_when_the_first_fails() {
        let e = env();
        let claude = fake_claude(&e, "exit 1");
        let outcome = run_eval(
            &plan(&e, claude, true, 1.0),
            &e.runner,
            &e.results,
            &cost,
            &AtomicBool::new(false),
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(outcome.arms.len(), 1);
        assert_eq!(outcome.arms[0].exit.exit_code, Some(1));
        assert_eq!(outcome.arms[0].report_text, None);
    }

    #[test]
    fn refuses_a_limit_too_small_to_share_and_runs_nothing_when_already_cancelled() {
        let e = env();
        let claude = fake_claude(&e, WRITE_RESULT);
        let tiny = run_eval(
            &plan(&e, claude.clone(), true, 0.0001),
            &e.runner,
            &e.results,
            &cost,
            &AtomicBool::new(false),
            &mut |_, _| {},
        );
        assert!(matches!(tiny, Err(EvalRunError::LimitTooSmall)));
        let outcome = run_eval(
            &plan(&e, claude, false, 1.0),
            &e.runner,
            &e.results,
            &cost,
            &AtomicBool::new(true),
            &mut |_, _| {},
        )
        .unwrap();
        assert!(outcome.cancelled && outcome.arms.is_empty());
    }

    #[test]
    fn passes_the_ablation_only_for_a_version_run_and_runs_in_the_stage_folder() {
        let e = env();
        let claude = fake_claude(
            &e,
            &format!(
                "all=\"$*\"; {WRITE_RESULT}; pwd; echo \"$all\" | tr ' ' '\\n' | grep -c ablation; true"
            ),
        );
        let mut seen = Vec::new();
        run_eval(
            &plan(&e, claude, true, 1.0),
            &e.runner,
            &e.results,
            &cost,
            &AtomicBool::new(false),
            &mut |part, event| {
                if let EvalEvent::Output { line, .. } = event {
                    seen.push((part, line));
                }
            },
        )
        .unwrap();
        let today = e.base.join("today").to_string_lossy().into_owned();
        let older = e.base.join("older").to_string_lossy().into_owned();
        assert!(seen.contains(&(Some(RunPart::Today), today)));
        assert!(seen.contains(&(Some(RunPart::Version), older)));
        assert!(seen.contains(&(Some(RunPart::Today), "1".to_owned())));
        assert!(seen.contains(&(Some(RunPart::Version), "1".to_owned())));
    }
}
