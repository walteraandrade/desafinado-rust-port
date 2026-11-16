use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::Duration,
};

use desafinado_features::{
    evals::{
        baseline::{RunBaseline, SummaryBaseline},
        baseline_view::summary_view,
        dialog_state::{EvalDialogState, PickEffect, settle_ask, settle_pick, start_pick},
        ended::failure_reason,
        first_eval::first_eval_example,
        report::EvalSummary,
        settings::EvalSettings,
        signals::{
            CaseSignals, FiredSide, SuspiciousKind, case_signals, fired_overall, helped_overall,
        },
        tally::{RunRecord, Tally},
        types::{
            EvalCancelResult, EvalOpenReportRequest, EvalOpenReportResult, EvalPrepareRequest,
            EvalRunRequest, EvalRunResult, EvalSkillRequest, EvalStatus, EvalsKit, ReportPart,
            StoppedRun,
        },
        unsaved::unsaved_notice,
        view::{
            ArmLabels, cost_estimate, count_label, errored_text, meter_share, stopped_text,
            suspicious_text, verdict,
        },
    },
    history::{store::HistoryStore, time::format_clock},
    skills::scan_skills::{Skill, SkillFile},
};
use desafinado_platform::evals::{
    CLAUDE_CLI_VARIABLE, EvalDeps, EvalRunner, EvalService, ResultsStore, RunnerEnv, count_evals,
    find_claude_with_override, is_executable_file, stage_skill,
};
use futures::channel::oneshot;
use gpui_kit::{prelude::FluentBuilder as _, *};

use crate::{
    eval_run_dialog::{EvalRunDialog, EvalRunEvent},
    library_panel,
    theme::{self, Roles},
    ui::{self, ButtonKind, ButtonSize},
    workspace::{column_row, now_ms},
};

const POLL: Duration = Duration::from_millis(1500);
const MONO: &str = "DejaVu Sans Mono";

pub fn service(user_data: PathBuf, history: Arc<dyn HistoryStore>) -> Arc<EvalService> {
    let home = std::env::var("HOME").unwrap_or_default();
    let path = std::env::var("PATH").ok();
    let library_data = user_data.clone();
    let find_home = PathBuf::from(&home);
    let find_path = path.clone();
    Arc::new(EvalService::new(EvalDeps {
        library_skill_paths: Box::new(move || {
            library_panel::list(&library_data)
                .roots
                .iter()
                .flat_map(|root| root.skills.iter().map(|skill| skill.real_path.clone()))
                .collect()
        }),
        find_claude: Box::new(move || {
            find_claude_with_override(
                std::env::var(CLAUDE_CLI_VARIABLE).ok().as_deref(),
                find_path.as_deref(),
                &find_home,
                &is_executable_file,
            )
        }),
        runner: EvalRunner::new(RunnerEnv {
            home,
            path,
            extra: Vec::new(),
        }),
        stage: Box::new(|skill, text| stage_skill(skill, &std::env::temp_dir(), text)),
        count_evals: Box::new(count_evals),
        results: ResultsStore::new(&user_data),
        history,
        open_path: Box::new(|file: &Path| Command::new("xdg-open").arg(file).spawn().is_ok()),
    }))
}

fn off_thread<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> oneshot::Receiver<T> {
    let (sender, receiver) = oneshot::channel();
    std::thread::spawn(move || {
        let _ = sender.send(work());
    });
    receiver
}

#[derive(Clone, Debug, PartialEq)]
pub struct EvalSkill {
    pub path: String,
    pub name: String,
    pub folder_name: String,
    pub files: Vec<SkillFile>,
}

impl From<&Skill> for EvalSkill {
    fn from(skill: &Skill) -> Self {
        Self {
            path: skill.real_path.clone(),
            name: skill.name.clone(),
            folder_name: skill.folder_name.clone(),
            files: skill.files.clone(),
        }
    }
}

#[derive(Clone, Debug)]
enum PanelState {
    Loading,
    Ready {
        case_count: usize,
        last: Option<EvalSummary>,
        cancelling: bool,
        stopped: Option<StoppedRun>,
    },
    NoCli,
    Failed(String),
}

pub enum EvalsEvent {
    Ended {
        path: String,
        name: String,
        result: EvalRunResult,
    },
    Ask,
    DialogClosed,
}

impl EventEmitter<EvalsEvent> for EvalsView {}

pub struct EvalsView {
    service: Arc<EvalService>,
    skill: Option<EvalSkill>,
    state: PanelState,
    dialog: EvalDialogState,
    dialog_view: Option<Entity<EvalRunDialog>>,
    message: Option<String>,
    going: Option<String>,
    latest: u64,
    unsaved: Vec<String>,
    checking: bool,
    busy_name: Option<String>,
    scroll: ScrollHandle,
    poll: Option<Task<()>>,
    utc_offset: i64,
    _dialog_events: Option<Subscription>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    Pass,
    Fail,
    None,
}

fn pass_mark(run: &RunRecord) -> Mark {
    match (run.errored, run.passed) {
        (true, _) => Mark::None,
        (false, true) => Mark::Pass,
        (false, false) => Mark::Fail,
    }
}

fn fired_mark(run: &RunRecord) -> Mark {
    match (run.errored, run.fired) {
        (false, Some(true)) => Mark::Pass,
        (false, Some(false)) => Mark::Fail,
        _ => Mark::None,
    }
}

fn short_flag(kind: SuspiciousKind) -> &'static str {
    match kind {
        SuspiciousKind::PassesInBoth => "passes either way",
        SuspiciousKind::FailsInBoth => "fails either way",
        SuspiciousKind::RunsDisagree => "runs disagree",
    }
}

pub fn local_date_time(iso: &str, offset_minutes: i64) -> String {
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|date| {
            (date.naive_utc() + chrono::Duration::minutes(offset_minutes))
                .format("%-m/%-d/%Y, %-I:%M:%S %p")
                .to_string()
        })
        .unwrap_or_else(|_| "Invalid Date".to_string())
}

fn faint_text(text: impl Into<SharedString>, roles: Roles) -> Div {
    div()
        .text_size(px(12.))
        .text_color(roles.faint)
        .child(text.into())
}

fn text_action(id: &'static str, label: &'static str, roles: Roles) -> Stateful<Div> {
    div()
        .id(id)
        .text_color(roles.ink)
        .underline()
        .cursor_pointer()
        .hover(|this| this.text_color(roles.accent))
        .child(label)
}

fn meter(label: &str, tally: &Tally, roles: Roles) -> Div {
    let share = meter_share(tally) as f32;
    div()
        .mt(px(12.))
        .child(
            ui::row()
                .justify_between()
                .items_end()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(roles.muted)
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .font_family("League Spartan")
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_size(px(20.))
                        .text_color(roles.heading)
                        .child(count_label(tally)),
                ),
        )
        .child(
            div()
                .mt(px(4.))
                .h(px(6.))
                .bg(roles.fill)
                .child(div().h_full().w(relative(share)).bg(roles.success)),
        )
}

fn pips(marks: Vec<Mark>, tally: Option<Tally>, roles: Roles) -> Div {
    let Some(tally) = tally else {
        return div().text_color(roles.faint).child("–");
    };
    ui::row()
        .flex_wrap()
        .gap(px(8.))
        .child(
            ui::row()
                .gap(px(2.))
                .children(marks.into_iter().map(|mark| {
                    let pip = div().size(px(10.)).border_1();
                    match mark {
                        Mark::Pass => pip.border_color(roles.success).bg(roles.success),
                        Mark::Fail => pip.border_color(roles.danger),
                        Mark::None => pip.border_color(roles.faint).border_dashed(),
                    }
                })),
        )
        .child(div().text_color(roles.ink).child(count_label(&tally)))
}

fn flags(row: usize, signal: &CaseSignals, labels: &ArmLabels, roles: Roles) -> Option<Div> {
    if signal.suspicious.is_empty() && signal.errored == 0 {
        return None;
    }
    let tag = |text: String, danger: bool| {
        div()
            .px(px(6.))
            .text_size(px(12.))
            .bg(if danger {
                roles.danger_fill
            } else {
                roles.fill
            })
            .text_color(if danger { roles.danger } else { roles.ink })
            .child(text)
    };
    Some(
        ui::row()
            .mt(px(4.))
            .flex_wrap()
            .gap(px(4.))
            .children(signal.suspicious.iter().enumerate().map(|(index, kind)| {
                let tip: SharedString = suspicious_text(*kind, labels).into();
                tag(
                    short_flag(*kind).to_string(),
                    *kind == SuspiciousKind::FailsInBoth,
                )
                .id(("eval-flag", row * 8 + index))
                .cursor(CursorStyle::Arrow)
                .tooltip(move |window, cx| {
                    component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                })
            }))
            .when(signal.errored != 0, |this| {
                this.child(tag(errored_text(signal.errored), true))
            }),
    )
}

fn code_block(text: &str, roles: Roles) -> Div {
    div()
        .mt(px(4.))
        .border_1()
        .border_color(roles.raised)
        .bg(roles.raised)
        .px(px(12.))
        .py(px(8.))
        .font_family(MONO)
        .text_size(px(12.))
        .line_height(relative(1.6))
        .children(
            text.strip_suffix('\n')
                .unwrap_or(text)
                .split('\n')
                .map(|line| {
                    div().whitespace_nowrap().child(if line.is_empty() {
                        "\u{a0}".to_string()
                    } else {
                        line.to_string()
                    })
                }),
        )
}

impl EvalsView {
    pub fn new(service: Arc<EvalService>, utc_offset: i64) -> Self {
        Self {
            service,
            skill: None,
            state: PanelState::Loading,
            dialog: EvalDialogState::Closed,
            dialog_view: None,
            message: None,
            going: None,
            latest: 0,
            unsaved: Vec::new(),
            checking: false,
            busy_name: None,
            scroll: ScrollHandle::new(),
            poll: None,
            utc_offset,
            _dialog_events: None,
        }
    }

    pub fn service(&self) -> Arc<EvalService> {
        self.service.clone()
    }

    pub fn skill(&self) -> Option<&EvalSkill> {
        self.skill.as_ref()
    }

    pub fn going(&self) -> Option<&str> {
        self.going.as_deref()
    }

    pub fn dialog(&self) -> Option<Entity<EvalRunDialog>> {
        self.dialog_view.clone()
    }

    pub fn set_busy_name(&mut self, name: Option<String>) {
        self.busy_name = name;
    }

    pub fn set_skill(
        &mut self,
        skill: Option<EvalSkill>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let same =
            self.skill.as_ref().map(|skill| &skill.path) == skill.as_ref().map(|skill| &skill.path);
        self.skill = skill;
        if same {
            return;
        }
        self.state = PanelState::Loading;
        self.message = None;
        self.unsaved.clear();
        self.set_dialog(EvalDialogState::Closed, window, cx);
        self.refresh(cx);
    }

    pub fn set_unsaved(&mut self, names: Vec<String>, cx: &mut Context<Self>) {
        if names == self.unsaved {
            return;
        }
        self.unsaved = names;
        let notice = unsaved_notice(&self.unsaved);
        if let Some(dialog) = &self.dialog_view {
            dialog.update(cx, |dialog, cx| dialog.set_unsaved_notice(notice, cx));
        }
        cx.notify();
    }

    pub fn set_checking(&mut self, checking: bool, cx: &mut Context<Self>) {
        self.checking = checking;
        cx.notify();
    }

    fn skill_path(&self) -> Option<String> {
        self.skill.as_ref().map(|skill| skill.path.clone())
    }

    fn running(&self) -> bool {
        self.going.is_some() && self.going == self.skill_path()
    }

    fn busy_with(&self) -> Option<&str> {
        self.going
            .as_deref()
            .filter(|going| Some(*going) != self.skill.as_ref().map(|skill| skill.path.as_str()))
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let Some(skill_path) = self.skill_path() else {
            return;
        };
        self.latest += 1;
        let load = self.latest;
        let service = self.service.clone();
        let result = off_thread(move || service.status(&EvalSkillRequest { skill_path }));
        cx.spawn(async move |this, cx| {
            let Ok(result) = result.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                if load != this.latest {
                    return;
                }
                this.state = match result {
                    EvalStatus::Ready {
                        case_count,
                        last,
                        cancelling,
                        stopped,
                        running_skill,
                        ..
                    } => {
                        this.set_going(running_skill, cx);
                        PanelState::Ready {
                            case_count,
                            last,
                            cancelling,
                            stopped,
                        }
                    }
                    EvalStatus::NoCli => PanelState::NoCli,
                    EvalStatus::Failed { reason } => PanelState::Failed(reason),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn set_going(&mut self, going: Option<String>, cx: &mut Context<Self>) {
        self.going = going;
        if self.going.is_none() {
            self.poll = None;
        } else if self.poll.is_none() {
            self.poll = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(POLL).await;
                    if this.update(cx, |this, cx| this.refresh(cx)).is_err() {
                        break;
                    }
                }
            }));
        }
        cx.notify();
    }

    fn patch_ready(
        &mut self,
        patch: impl FnOnce(&mut Option<EvalSummary>, &mut bool, &mut Option<StoppedRun>),
    ) {
        if let PanelState::Ready {
            last,
            cancelling,
            stopped,
            ..
        } = &mut self.state
        {
            patch(last, cancelling, stopped);
        }
    }

    fn set_dialog(&mut self, dialog: EvalDialogState, window: &mut Window, cx: &mut Context<Self>) {
        match &dialog {
            EvalDialogState::Confirm {
                prepared,
                baseline,
                refreshing,
            } => match &self.dialog_view {
                Some(view) => view.update(cx, |view, cx| {
                    view.update_state(prepared.clone(), *baseline, *refreshing, cx)
                }),
                None => {
                    let name = self
                        .skill
                        .as_ref()
                        .map(|skill| skill.name.clone())
                        .unwrap_or_default();
                    let notice = unsaved_notice(&self.unsaved);
                    let (prepared, baseline, refreshing) =
                        (prepared.clone(), *baseline, *refreshing);
                    let view = cx.new(|cx| {
                        EvalRunDialog::new(name, prepared, baseline, refreshing, notice, window, cx)
                    });
                    self._dialog_events = Some(cx.subscribe_in(
                        &view,
                        window,
                        |this, _, event: &EvalRunEvent, window, cx| match event {
                            EvalRunEvent::Pick(baseline) => this.pick(*baseline, window, cx),
                            EvalRunEvent::Run(settings) => this.run(settings.clone(), window, cx),
                            EvalRunEvent::Close => this.close(window, cx),
                        },
                    ));
                    self.dialog_view = Some(view);
                }
            },
            _ => {
                if self.dialog_view.take().is_some() {
                    cx.emit(EvalsEvent::DialogClosed);
                }
                self._dialog_events = None;
            }
        }
        self.dialog = dialog;
        cx.notify();
    }

    fn apply_effect(&mut self, effect: PickEffect) {
        match effect {
            PickEffect::NoEvals => {
                if let PanelState::Ready { case_count, .. } = &mut self.state {
                    *case_count = 0;
                }
            }
            PickEffect::NoCli => self.state = PanelState::NoCli,
            PickEffect::Message { reason } => self.message = Some(reason),
            PickEffect::None => {}
        }
    }

    pub fn ask(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(skill_path) = self.skill_path() else {
            return;
        };
        self.message = None;
        self.set_dialog(EvalDialogState::Preparing, window, cx);
        let service = self.service.clone();
        let result = off_thread(move || {
            service.prepare(&EvalPrepareRequest {
                skill_path,
                baseline: RunBaseline::None,
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(result) = result.await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                let settled = settle_ask(&result);
                this.set_dialog(settled.dialog, window, cx);
                this.apply_effect(settled.effect);
                cx.notify();
            });
        })
        .detach();
    }

    fn pick(&mut self, baseline: RunBaseline, window: &mut Window, cx: &mut Context<Self>) {
        let Some(skill_path) = self.skill_path() else {
            return;
        };
        let started = start_pick(&self.dialog, baseline);
        self.set_dialog(started, window, cx);
        let service = self.service.clone();
        let result = off_thread(move || {
            service.prepare(&EvalPrepareRequest {
                skill_path,
                baseline,
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(result) = result.await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                let settled = settle_pick(&this.dialog, baseline, &result);
                this.set_dialog(settled.dialog, window, cx);
                this.apply_effect(settled.effect);
                cx.notify();
            });
        })
        .detach();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_dialog(EvalDialogState::Closed, window, cx);
    }

    fn run(&mut self, settings: EvalSettings, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(skill_path), EvalDialogState::Confirm { baseline, .. }) =
            (self.skill_path(), &self.dialog)
        else {
            return;
        };
        let baseline = *baseline;
        let name = self
            .skill
            .as_ref()
            .map(|skill| skill.name.clone())
            .unwrap_or_default();
        self.set_dialog(EvalDialogState::Closed, window, cx);
        self.latest += 1;
        self.set_going(Some(skill_path.clone()), cx);
        self.patch_ready(|_, cancelling, stopped| {
            *cancelling = false;
            *stopped = None;
        });
        self.message = None;
        let service = self.service.clone();
        let request = EvalRunRequest {
            skill_path: skill_path.clone(),
            baseline,
            settings,
        };
        let result = off_thread(move || service.run(&request));
        cx.spawn(async move |this, cx| {
            let result = result.await.unwrap_or(EvalRunResult::Failed {
                reason: "Could not run claude".into(),
            });
            let _ = this.update(cx, |this, cx| {
                this.set_going(None, cx);
                this.latest += 1;
                cx.emit(EvalsEvent::Ended {
                    path: skill_path.clone(),
                    name,
                    result: result.clone(),
                });
                if this.skill_path().as_deref() != Some(skill_path.as_str()) {
                    return;
                }
                match result {
                    EvalRunResult::Completed { summary } => {
                        this.patch_ready(|last, cancelling, stopped| {
                            *last = Some(summary);
                            *cancelling = false;
                            *stopped = None;
                        })
                    }
                    EvalRunResult::NotComplete { message, cost_usd } => {
                        this.patch_ready(|_, cancelling, stopped| {
                            *cancelling = false;
                            *stopped = Some(StoppedRun { message, cost_usd });
                        })
                    }
                    other => {
                        this.patch_ready(|_, cancelling, _| *cancelling = false);
                        this.message = Some(failure_reason(&other));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        self.latest += 1;
        self.patch_ready(|_, cancelling, _| *cancelling = true);
        cx.notify();
        let service = self.service.clone();
        let result = off_thread(move || service.cancel());
        cx.spawn(async move |this, cx| {
            let Ok(result) = result.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.latest += 1;
                if let EvalCancelResult::Failed { reason } = result {
                    this.patch_ready(|_, cancelling, _| *cancelling = false);
                    this.message = Some(reason);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_report(&mut self, part: ReportPart, cx: &mut Context<Self>) {
        let Some(skill_path) = self.skill_path() else {
            return;
        };
        let service = self.service.clone();
        let result = off_thread(move || {
            service.open_report(&EvalOpenReportRequest {
                skill_path,
                part: Some(part),
            })
        });
        cx.spawn(async move |this, cx| {
            let Ok(EvalOpenReportResult::Failed { reason }) = result.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.message = Some(reason);
                cx.notify();
            });
        })
        .detach();
    }

    fn render_guide(&self, skill: &EvalSkill, roles: Roles, cx: &mut Context<Self>) -> Div {
        let example = first_eval_example(&skill.name, &skill.folder_name);
        let paragraph = |text: &'static str| div().mt(px(16.)).child(text);
        div()
            .mt(px(24.))
            .child(ui::title("This Skill has no Evals yet", roles))
            .child(div().mt(px(8.)).child(
                "An Eval is a folder you write by hand under the Skill's evals/. An Eval run sends its prompt.md to the agent in two Arms, with the Skill and without it, and the files in graders/ score each answer.",
            ))
            .child(code_block(&example.tree, roles))
            .children(example.files.iter().map(|file| {
                div()
                    .mt(px(16.))
                    .child(
                        div()
                            .font_family(MONO)
                            .text_size(px(12.))
                            .text_color(roles.ink)
                            .child(file.path.clone()),
                    )
                    .child(code_block(&file.text, roles))
            }))
            .child(paragraph(
                "Reach for the free graders first: regex, tool_used, tool_order and file_exists cost nothing and give the same verdict every time. An llm grader calls a judge 3 times per run, so keep it for short answers.",
            ))
            .child(
                ui::row()
                    .mt(px(16.))
                    .flex_wrap()
                    .gap(px(8.))
                    .child("Save the files, then check again.")
                    .child(
                        ui::button("eval-guide-check", "Check again", ButtonKind::Default, roles)
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    ),
            )
    }

    fn render_summary(&self, last: &EvalSummary, roles: Roles) -> Div {
        let now = now_ms();
        let offset = self.utc_offset;
        let view = summary_view(&last.baseline, |at| format_clock(at, now, offset));
        let signals: Vec<CaseSignals> = last.cases.iter().map(case_signals).collect();
        let suspicious = signals.iter().any(|signal| !signal.suspicious.is_empty());
        let fired = fired_overall(&last.cases, FiredSide::First);
        let fired_second = if view.fired_sides == 1 {
            None
        } else {
            fired_overall(&last.cases, FiredSide::Second)
        };
        let helped = helped_overall(&last.cases);
        let card = |title: &'static str| {
            div()
                .flex_1()
                .min_w_0()
                .border_1()
                .border_color(roles.hairline)
                .p(px(16.))
                .child(div().text_size(px(12.)).text_color(roles.ink).child(title))
        };
        let fired_card = card("Did the Skill fire?").map(|this| match fired {
            None => this.child(
                div()
                    .mt(px(8.))
                    .text_color(roles.faint)
                    .child(view.fired_text(None, None)),
            ),
            Some(fired) => this
                .child(meter(view.columns.first, &fired, roles))
                .when_some(fired_second, |this, second| {
                    this.child(meter(view.columns.second, &second, roles))
                })
                .child(
                    faint_text(view.fired_text(Some(&fired), fired_second.as_ref()), roles)
                        .mt(px(12.)),
                ),
        });
        let helped_card = card("Did it help?").map(|this| match helped {
            None => this.child(
                div()
                    .mt(px(8.))
                    .text_color(roles.faint)
                    .child(view.helped_text(None)),
            ),
            Some(helped) => this
                .child(meter(view.columns.first, &helped.with, roles))
                .child(meter(view.columns.second, &helped.without, roles))
                .child(
                    div()
                        .mt(px(12.))
                        .text_size(px(12.))
                        .text_color(roles.ink)
                        .child(verdict(
                            &helped.with,
                            &helped.without,
                            &view.labels,
                            view.fired_sides == 2,
                        )),
                )
                .when(helped.left_out != 0, |this| {
                    this.child(
                        faint_text(format!("{}.", view.left_out(helped.left_out)), roles)
                            .mt(px(4.)),
                    )
                }),
        });
        let header_cell = |text: String| div().py(px(8.)).pr(px(12.)).child(text);
        let first_column =
            |content: Div| div().w(relative(0.34)).flex_none().min_w_0().child(content);
        let rest_column = |content: Div| div().flex_1().min_w_0().child(content);
        let header = ui::row()
            .items_start()
            .border_t_1()
            .border_color(roles.hairline)
            .text_size(px(12.))
            .text_color(roles.faint)
            .child(first_column(header_cell("Eval".into())))
            .child(rest_column(header_cell("Fired".into())))
            .child(rest_column(header_cell(view.columns.first.into())))
            .child(rest_column(header_cell(view.columns.second.into())));
        let rows = last
            .cases
            .iter()
            .zip(&signals)
            .enumerate()
            .map(|(index, (case, signal))| {
                let cell = |content: Div| div().py(px(12.)).pr(px(12.)).child(content);
                ui::row()
                    .items_start()
                    .border_t_1()
                    .border_color(roles.hairline)
                    .child(first_column(cell(
                        div()
                            .child(div().text_color(roles.ink).child(case.name.clone()))
                            .children(flags(index, signal, &view.labels, roles)),
                    )))
                    .child(rest_column(cell(pips(
                        case.runs.with.iter().map(fired_mark).collect(),
                        signal.fired,
                        roles,
                    ))))
                    .child(rest_column(cell(pips(
                        case.runs.with.iter().map(pass_mark).collect(),
                        (case.with.total != 0).then_some(case.with),
                        roles,
                    ))))
                    .child(rest_column(cell(pips(
                        case.runs.without.iter().flatten().map(pass_mark).collect(),
                        case.without,
                        roles,
                    ))))
            });
        div()
            .mt(px(24.))
            .when_some(view.compared_note.clone(), |this, note| {
                this.child(faint_text(note, roles).mb(px(12.)))
            })
            .child(
                ui::row()
                    .items_start()
                    .gap(px(12.))
                    .child(fired_card)
                    .child(helped_card),
            )
            .child(
                div()
                    .mt(px(32.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(roles.ink)
                            .child(format!("EACH EVAL · {}", last.cases.len())),
                    )
                    .child(div().mt(px(8.)).child(header).children(rows))
                    .when(suspicious, |this| {
                        this.child(
                            faint_text("Flagged Evals still count in the totals.", roles)
                                .mt(px(8.)),
                        )
                    }),
            )
    }
}

impl Render for EvalsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let roles = theme::roles(cx);
        let Some(skill) = self.skill.clone() else {
            return div().into_any_element();
        };
        let running = self.running();
        let busy_with = self.busy_with().is_some();
        let last = match &self.state {
            PanelState::Ready { last, .. } => last.clone(),
            _ => None,
        };
        let no_evals = matches!(self.state, PanelState::Ready { case_count: 0, .. });
        let no_run_yet = last.is_none() && !no_evals;
        let cancelling = running
            && matches!(
                self.state,
                PanelState::Ready {
                    cancelling: true,
                    ..
                }
            );
        let notice = unsaved_notice(&self.unsaved);
        let now = now_ms();
        let offset = self.utc_offset;
        let view = summary_view(
            &last
                .as_ref()
                .map_or(SummaryBaseline::None, |last| last.baseline),
            |at| format_clock(at, now, offset),
        );
        let header = div()
            .pt(px(16.))
            .child(
                div()
                    .font_family("League Spartan")
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(30.))
                    .text_color(roles.heading)
                    .child("How this Skill performs"),
            )
            .child(
                ui::row()
                    .mt(px(12.))
                    .flex_wrap()
                    .gap(px(8.))
                    .text_color(roles.ink)
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(skill.name.clone()),
                    )
                    .when_some(last.as_ref(), |this, last| {
                        this.child("·")
                            .child(format!(
                                "Last run {}",
                                local_date_time(&last.started_at, offset)
                            ))
                            .child("·")
                            .child(
                                div()
                                    .id("eval-cost")
                                    .child(format!("Cost {}", cost_estimate(last.cost_usd)))
                                    .tooltip(|window, cx| {
                                        component::tooltip::Tooltip::new("The CLI's figure")
                                            .build(window, cx)
                                    }),
                            )
                    }),
            );
        let line = |text: String, color: Hsla| div().mt(px(12.)).text_color(color).child(text);
        let state_line: Option<AnyElement> = match &self.state {
            PanelState::Loading => Some(line("Looking for the Claude CLI…".into(), roles.faint).into_any_element()),
            PanelState::NoCli => Some(
                ui::row()
                    .mt(px(12.))
                    .flex_wrap()
                    .gap(px(4.))
                    .child("The Claude CLI was not found on PATH or in ~/.local/bin. Install it and log in to run Evals.")
                    .child(
                        text_action("eval-check-again", "Check again", roles)
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    )
                    .into_any_element(),
            ),
            PanelState::Failed(reason) => Some(
                ui::row()
                    .mt(px(12.))
                    .gap(px(4.))
                    .child(div().text_color(roles.danger).child(format!("{reason}.")))
                    .child(
                        text_action("eval-try-again", "Try again", roles)
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    )
                    .into_any_element(),
            ),
            PanelState::Ready { .. } => None,
        };
        let ready = match &self.state {
            PanelState::Ready { stopped, .. } => {
                let run_disabled = running
                    || busy_with
                    || self.checking
                    || self.dialog == EvalDialogState::Preparing;
                let buttons = ui::row()
                    .mt(px(20.))
                    .flex_wrap()
                    .gap(px(8.))
                    .when(!no_evals, |this| {
                        this.child(
                            ui::sized_button(
                                "eval-run",
                                if running { "Running…" } else { "Run Evals" },
                                ButtonKind::Strong,
                                ButtonSize::Regular, ui::Control::disabled(run_disabled),
                                roles,)
                            .when(!run_disabled, |this| {
                                this.on_click(cx.listener(|_, _, _, cx| cx.emit(EvalsEvent::Ask)))
                            }),
                        )
                    })
                    .when(running, |this| {
                        this.child(
                            ui::sized_button(
                                "eval-cancel",
                                if cancelling {
                                    "Cancelling…"
                                } else {
                                    "Cancel"
                                },
                                ButtonKind::Default,
                                ButtonSize::Regular, ui::Control::disabled(cancelling),
                                roles,)
                            .when(!cancelling, |this| {
                                this.on_click(cx.listener(|this, _, _, cx| this.cancel(cx)))
                            }),
                        )
                    })
                    .when(last.is_some(), |this| {
                        this.children(view.reports.iter().map(|report| {
                            let part = report.part;
                            ui::button(
                                ("eval-report", part as usize),
                                report.label,
                                ButtonKind::Default,
                                roles,
                            )
                            .on_click(cx.listener(move |this, _, _, cx| this.open_report(part, cx)))
                        }))
                    });
                Some(
                    div()
                        .when_some(stopped.clone(), |this, stopped| {
                            this.child(line(stopped_text(&stopped.message, stopped.cost_usd), roles.ink))
                        })
                        .when(no_evals, |this| this.child(self.render_guide(&skill, roles, cx)))
                        .child(buttons)
                        .when_some(notice, |this, notice| this.child(line(notice, roles.ink)))
                        .when(no_run_yet, |this| {
                            this.child(
                                div()
                                    .mt(px(24.))
                                    .border_1()
                                    .border_color(roles.hairline)
                                    .px(px(16.))
                                    .py(px(24.))
                                    .flex()
                                    .justify_center()
                                    .text_color(roles.faint)
                                    .child("No complete run yet. Run Evals to see how this Skill does."),
                            )
                        })
                        .when_some(last.as_ref(), |this, last| this.child(self.render_summary(last, roles))),
                )
            }
            _ => None,
        };
        let busy_line = (busy_with && !running).then(|| {
            line(
                format!(
                    "Evals are running for {}. One run at a time.",
                    self.busy_name
                        .clone()
                        .unwrap_or_else(|| "another Skill".into())
                ),
                roles.ink,
            )
        });
        let running_line = running.then(|| {
            line(
                "Running Evals. This can take a few minutes.".into(),
                roles.faint,
            )
        });
        div()
            .id("evals-page")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .text_size(px(14.))
            .child(
                column_row(
                    div()
                        .child(header)
                        .children(state_line)
                        .when_some(self.message.clone(), |this, message| {
                            this.child(line(message, roles.danger))
                        })
                        .children(ready)
                        .children(busy_line)
                        .children(running_line),
                )
                .pb(px(160.)),
            )
            .into_any_element()
    }
}
