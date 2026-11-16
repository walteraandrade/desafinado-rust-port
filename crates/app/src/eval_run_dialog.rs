use desafinado_features::{
    evals::{
        baseline::{RunBaseline, version_option_label},
        baseline_view::{RunKind, run_copy},
        limits::{LIMIT_TOO_SMALL, first_run_limit},
        risks::risk_lines,
        settings::{EvalSettings, check_eval_settings, default_eval_settings},
        types::PreparedReady,
    },
    history::time::format_clock,
};
use gpui_kit::{
    component::input::{Input, InputEvent, InputState},
    prelude::FluentBuilder as _,
    *,
};

use crate::{
    theme::{self, Roles},
    ui::{self, ButtonKind, ButtonSize},
    workspace::{now_ms, utc_offset_minutes},
};

actions!(eval_run, [CancelEvalRun, SubmitEvalRun]);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CancelEvalRun, Some("EvalRunDialog")),
        KeyBinding::new("enter", SubmitEvalRun, Some("EvalRunDialog > Input")),
    ]);
}

const REACH: &str = "The agent runs as you, on your Claude login, in a temporary copy of the Skill. That copy leaves out hooks, the plugin manifest, MCP and LSP config, settings, agents and commands. The CLI gives each run its own home and working folder, but it says its sandbox is not a guarantee, so what the agent's tools can read on this machine is not limited by Desafinado. Hard links to files outside the folder are copied as content.";

pub enum EvalRunEvent {
    Pick(RunBaseline),
    Run(EvalSettings),
    Close,
}

impl EventEmitter<EvalRunEvent> for EvalRunDialog {}

pub struct EvalRunDialog {
    skill_name: String,
    prepared: PreparedReady,
    baseline: RunBaseline,
    refreshing: bool,
    unsaved_notice: Option<String>,
    runs: Entity<InputState>,
    model: Entity<InputState>,
    cost: Entity<InputState>,
    picking: bool,
    utc_offset: i64,
    _subscriptions: Vec<Subscription>,
}

fn js_number(text: &str) -> f64 {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        0.0
    } else {
        trimmed.parse().unwrap_or(f64::NAN)
    }
}

fn number_text(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}

impl EvalRunDialog {
    pub fn new(
        skill_name: String,
        prepared: PreparedReady,
        baseline: RunBaseline,
        refreshing: bool,
        unsaved_notice: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let defaults = default_eval_settings();
        let input = |value: String, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).default_value(value))
        };
        let runs = input(number_text(defaults.runs), window, cx);
        let model = input(defaults.model, window, cx);
        let cost = input(number_text(defaults.max_cost_usd), window, cx);
        runs.update(cx, |state, cx| state.focus(window, cx));
        let subscriptions = [&runs, &model, &cost]
            .into_iter()
            .map(|state| {
                cx.subscribe(state, |_, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                })
            })
            .collect();
        Self {
            skill_name,
            prepared,
            baseline,
            refreshing,
            unsaved_notice,
            runs,
            model,
            cost,
            picking: false,
            utc_offset: utc_offset_minutes(),
            _subscriptions: subscriptions,
        }
    }

    pub fn update_state(
        &mut self,
        prepared: PreparedReady,
        baseline: RunBaseline,
        refreshing: bool,
        cx: &mut Context<Self>,
    ) {
        self.prepared = prepared;
        self.baseline = baseline;
        self.refreshing = refreshing;
        cx.notify();
    }

    pub fn set_unsaved_notice(&mut self, notice: Option<String>, cx: &mut Context<Self>) {
        self.unsaved_notice = notice;
        cx.notify();
    }

    fn settings(&self, cx: &App) -> EvalSettings {
        EvalSettings {
            runs: js_number(&self.runs.read(cx).value()),
            model: self.model.read(cx).value().to_string(),
            max_cost_usd: js_number(&self.cost.read(cx).value()),
        }
    }

    fn problems(&self, settings: &EvalSettings) -> Vec<String> {
        let mut problems = check_eval_settings(settings);
        if matches!(self.baseline, RunBaseline::Version { .. })
            && settings.max_cost_usd.is_finite()
            && settings.max_cost_usd > 0.0
            && first_run_limit(settings.max_cost_usd).is_none()
        {
            problems.push(LIMIT_TOO_SMALL.to_string());
        }
        problems
    }

    fn ready(&self, cx: &App) -> bool {
        self.problems(&self.settings(cx)).is_empty()
            && self.prepared.case_count != 0
            && !self.refreshing
    }

    fn submit(&mut self, _: &SubmitEvalRun, _: &mut Window, cx: &mut Context<Self>) {
        if self.ready(cx) {
            cx.emit(EvalRunEvent::Run(self.settings(cx)));
        }
    }

    fn cancel(&mut self, _: &CancelEvalRun, _: &mut Window, cx: &mut Context<Self>) {
        if self.picking {
            self.picking = false;
            cx.notify();
        } else {
            cx.emit(EvalRunEvent::Close);
        }
    }

    fn baseline_label(&self, baseline: RunBaseline) -> String {
        let now = now_ms();
        let offset = self.utc_offset;
        match baseline {
            RunBaseline::None => "No Skill".to_string(),
            RunBaseline::Version { version_id } => self
                .prepared
                .versions
                .iter()
                .find(|version| version.id == version_id)
                .map(|version| {
                    version_option_label(
                        &self.prepared.versions,
                        version,
                        |at| format_clock(at, now, offset),
                        self.prepared.same_as_saved.contains(&version.id),
                    )
                })
                .unwrap_or_default(),
        }
    }

    fn render_select(&self, roles: Roles, cx: &mut Context<Self>) -> AnyElement {
        let options: Vec<RunBaseline> = std::iter::once(RunBaseline::None)
            .chain(
                self.prepared
                    .versions
                    .iter()
                    .rev()
                    .map(|version| RunBaseline::Version {
                        version_id: version.id,
                    }),
            )
            .collect();
        let disabled = self.refreshing;
        let field = div()
            .id("eval-baseline")
            .w_full()
            .h(px(30.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .bg(roles.fill)
            .border_1()
            .border_color(if self.picking {
                roles.accent
            } else {
                roles.hairline
            })
            .text_size(px(14.))
            .text_color(if disabled { roles.faint } else { roles.ink })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(self.baseline_label(self.baseline)),
            )
            .child(
                component::Icon::new(component::IconName::ChevronDown)
                    .size(px(14.))
                    .text_color(roles.ink),
            )
            .when(!disabled, |this| {
                this.cursor_pointer()
                    .hover(|this| this.bg(roles.fill_hover))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.picking = !this.picking;
                        cx.notify();
                    }))
            });
        let list = self.picking.then(|| {
            div()
                .id("eval-baseline-options")
                .w_full()
                .max_h(px(200.))
                .overflow_y_scroll()
                .border_1()
                .border_t_0()
                .border_color(roles.hairline)
                .bg(roles.fill)
                .children(options.into_iter().enumerate().map(|(index, option)| {
                    let chosen = option == self.baseline;
                    div()
                        .id(("eval-baseline-option", index))
                        .px(px(10.))
                        .py(px(4.))
                        .text_size(px(13.))
                        .text_color(roles.ink)
                        .border_1()
                        .border_color(if chosen { roles.accent } else { roles.fill })
                        .when(chosen, |this| this.bg(roles.raised))
                        .cursor_pointer()
                        .hover(|this| this.bg(roles.fill_hover))
                        .child(self.baseline_label(option))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.picking = false;
                            if option != this.baseline {
                                cx.emit(EvalRunEvent::Pick(option));
                            }
                            cx.notify();
                        }))
                }))
        });
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(field)
            .children(list)
            .into_any_element()
    }
}

fn small_heading(text: &'static str, roles: Roles) -> Div {
    div()
        .mt(px(16.))
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(roles.ink)
        .child(text)
}

fn risk_list(title: Option<&'static str>, lines: Vec<String>, roles: Roles) -> Div {
    div()
        .when_some(title, |this, title| {
            this.child(
                div()
                    .mt(px(8.))
                    .text_size(px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(roles.ink)
                    .child(title),
            )
        })
        .child(if lines.is_empty() {
            div()
                .mt(px(4.))
                .text_size(px(14.))
                .child("Nothing flagged.")
        } else {
            div()
                .mt(px(4.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .text_size(px(14.))
                .children(lines.into_iter().map(|line| {
                    ui::row()
                        .items_start()
                        .gap(px(8.))
                        .pl(px(6.))
                        .child(div().flex_none().child("•"))
                        .child(div().flex_1().min_w_0().child(line))
                }))
        })
}

impl Render for EvalRunDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let roles = theme::roles(cx);
        let settings = self.settings(cx);
        let problems = self.problems(&settings);
        let ready = self.ready(cx);
        let kind = match self.baseline {
            RunBaseline::None => RunKind::None,
            RunBaseline::Version { .. } => RunKind::Version,
        };
        let copy = run_copy(kind);
        let no_cases = self.prepared.case_count == 0;
        let intro = if no_cases {
            "This Skill has no Evals yet. Close this dialog to see how to write the first one."
                .to_string()
        } else {
            (copy.intro)(self.prepared.case_count, &self.prepared.claude_version)
        };
        let field = |id: &'static str,
                     label: &'static str,
                     state: &Entity<InputState>,
                     window: &Window,
                     cx: &App| {
            let focused = state.read(cx).focus_handle(cx).is_focused(window);
            div()
                .id(id)
                .flex_1()
                .min_w_0()
                .child(ui::label(label, roles))
                .child(ui::field(
                    Input::new(state).appearance(false),
                    focused,
                    false,
                    roles,
                ))
        };
        let risks = if self.refreshing {
            div()
                .mt(px(4.))
                .text_size(px(14.))
                .text_color(roles.faint)
                .child("Checking this Version…")
        } else {
            div()
                .child(risk_list(
                    copy.today_title,
                    risk_lines(&self.prepared.risks),
                    roles,
                ))
                .when_some(self.prepared.baseline_risks.clone(), |this, older| {
                    this.child(risk_list(copy.older_title, risk_lines(&older), roles))
                })
        };
        ui::modal_backdrop(
            "eval-run-backdrop",
            roles,
            ui::dialog_frame("eval-run", px(544.), roles)
                .key_context("EvalRunDialog")
                .on_action(cx.listener(Self::submit))
                .on_action(cx.listener(Self::cancel))
                .max_h(window.viewport_size().height - px(48.))
                .overflow_y_scroll()
                .child(ui::title(
                    format!("Run Evals for {}", self.skill_name),
                    roles,
                ))
                .child(ui::detail(intro, roles))
                .child(small_heading("Compare against", roles).mb(px(4.)))
                .child(self.render_select(roles, cx))
                .when_some(copy.stage_note, |this, note| {
                    this.child(
                        div()
                            .mt(px(4.))
                            .text_size(px(12.))
                            .text_color(roles.faint)
                            .child(note),
                    )
                })
                .child(small_heading("What a run can reach", roles))
                .child(div().mt(px(4.)).text_size(px(14.)).child(REACH))
                .child(small_heading("Found on this Skill", roles))
                .child(risks)
                .when_some(self.unsaved_notice.clone(), |this, notice| {
                    this.child(
                        div()
                            .mt(px(16.))
                            .text_size(px(14.))
                            .text_color(roles.ink)
                            .child(format!("{notice} Save first to measure your edits.")),
                    )
                })
                .child(
                    ui::row()
                        .mt(px(16.))
                        .items_start()
                        .gap(px(12.))
                        .child(field("eval-runs", "Runs per Arm", &self.runs, window, cx))
                        .child(field("eval-model", "Model", &self.model, window, cx))
                        .child(field(
                            "eval-cost",
                            "Cost limit (US$)",
                            &self.cost,
                            window,
                            cx,
                        )),
                )
                .when(!problems.is_empty(), |this| {
                    this.child(ui::problem_list(problems, roles).mt(px(8.)))
                })
                .child(
                    div()
                        .mt(px(8.))
                        .text_size(px(12.))
                        .text_color(roles.faint)
                        .child(format!(
                            "{} A run reads what is saved on disk.",
                            copy.limit_note
                        )),
                )
                .child(
                    ui::actions_row()
                        .child(
                            ui::sized_button(
                                "eval-run-cancel",
                                "Cancel",
                                ButtonKind::Default,
                                ButtonSize::Regular, ui::Control::default(),
                                roles,)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(EvalRunEvent::Close))),
                        )
                        .child(
                            ui::sized_button(
                                "eval-run-submit",
                                "Run",
                                ButtonKind::Strong,
                                ButtonSize::Regular, ui::Control::disabled(!ready),
                                roles,)
                            .when(ready, |this| {
                                this.on_click(cx.listener(|this, _, window, cx| {
                                    this.submit(&SubmitEvalRun, window, cx)
                                }))
                            }),
                        ),
                ),
        )
    }
}
