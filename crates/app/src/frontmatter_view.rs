use desafinado_core::log_err::LogErr;
use std::sync::Arc;

use desafinado_ai::prompts::DescriptionPromptInput;
use desafinado_core::suggestions::describe_error;
use desafinado_features::skills::{
    description_ai::{DescriptionOutcome, description_outcome, description_source, skill_body},
    frontmatter_fields::SkillField,
    frontmatter_panel::{FrontmatterReport, PanelProblem, other_problems, problems_of_field},
    skill_frontmatter::{CheckedField, DESCRIPTION_MAX_LENGTH},
};
use desafinado_runtime::ai_backend::{AiBackend, DescriptionCall};
use gpui_kit::{
    component::input::{Input, InputEvent, InputState, Textarea, TextareaState},
    prelude::FluentBuilder as _,
    *,
};

use crate::{
    theme::{self, Roles},
    ui::{self, ButtonKind, ButtonSize, Type as _},
};

pub enum FrontmatterEvent {
    Edit(SkillField, String),
}

impl EventEmitter<FrontmatterEvent> for FrontmatterView {}

#[derive(Clone, Debug, PartialEq)]
enum DescriptionAi {
    Idle,
    Working {
        source: String,
    },
    Ready {
        source: String,
        description: String,
        rationale: String,
    },
    Failed {
        source: String,
        message: String,
    },
}

impl DescriptionAi {
    fn source(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::Working { source } | Self::Ready { source, .. } | Self::Failed { source, .. } => {
                Some(source)
            }
        }
    }
}

pub struct FrontmatterView {
    pub report: FrontmatterReport,
    text: String,
    name: Entity<InputState>,
    description: Entity<TextareaState>,
    ai: Arc<dyn AiBackend>,
    assist: DescriptionAi,
    request: Option<Task<()>>,
    pub focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

fn messages(problems: Vec<PanelProblem>) -> Vec<String> {
    problems
        .into_iter()
        .map(|problem| problem.message)
        .collect()
}

impl FrontmatterView {
    pub fn new(ai: Arc<dyn AiBackend>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx));
        let description = cx.new(|cx| TextareaState::new(window, cx).auto_grow(2, 8));
        let name_events = cx.subscribe(&name, |this, state, event: &InputEvent, cx| {
            this.input_event(
                SkillField::Name,
                state.read(cx).value().to_string(),
                event,
                cx,
            );
        });
        let description_events =
            cx.subscribe(&description, |this, state, event: &InputEvent, cx| {
                this.input_event(
                    SkillField::Description,
                    state.read(cx).value().to_string(),
                    event,
                    cx,
                );
            });
        Self {
            report: FrontmatterReport::None,
            text: String::new(),
            name,
            description,
            ai,
            assist: DescriptionAi::Idle,
            request: None,
            focus: cx.focus_handle(),
            _subscriptions: vec![name_events, description_events],
        }
    }

    fn input_event(
        &mut self,
        field: SkillField,
        value: String,
        event: &InputEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => cx.emit(FrontmatterEvent::Edit(field, value)),
            _ => cx.notify(),
        }
    }

    pub fn refresh(
        &mut self,
        text: &str,
        report: FrontmatterReport,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.text = text.to_string();
        if let FrontmatterReport::Skill {
            name, description, ..
        } = &report
        {
            let name_focused = self.name.read(cx).focus_handle(cx).is_focused(window);
            if !name_focused && self.name.read(cx).value().as_ref() != name.as_str() {
                let name = name.clone();
                self.name
                    .update(cx, |state, cx| state.set_value(name, window, cx));
            }
            let description_focused = self
                .description
                .read(cx)
                .focus_handle(cx)
                .is_focused(window);
            if !description_focused
                && self.description.read(cx).value().as_ref() != description.as_str()
            {
                let description = description.clone();
                self.description
                    .update(cx, |state, cx| state.set_value(description, window, cx));
            }
        }
        if self.report != report {
            self.report = report;
            cx.notify();
        }
    }

    pub fn has_panel(&self) -> bool {
        self.report != FrontmatterReport::None
    }

    pub fn focus_panel(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.report {
            FrontmatterReport::Skill { .. } => {
                self.name.update(cx, |state, cx| state.focus(window, cx));
            }
            _ => window.focus(&self.focus, cx),
        }
    }

    fn ask(&mut self, cx: &mut Context<Self>) {
        let FrontmatterReport::Skill {
            name, description, ..
        } = &self.report
        else {
            return;
        };
        let source = description_source(name, description);
        let input = DescriptionPromptInput {
            name: name.clone(),
            description: description.clone(),
            body: skill_body(&self.text),
        };
        let current = description.clone();
        self.assist = DescriptionAi::Working {
            source: source.clone(),
        };
        let ai = Arc::clone(&self.ai);
        let work = cx.background_spawn(async move { ai.description(DescriptionCall::new(input)) });
        self.request = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| {
                this.request = None;
                this.assist = match result {
                    Err(error) => DescriptionAi::Failed {
                        source,
                        message: describe_error(&error),
                    },
                    Ok(value) => match description_outcome(&value.replacement, &current) {
                        DescriptionOutcome::Failed { message } => {
                            DescriptionAi::Failed { source, message }
                        }
                        DescriptionOutcome::Ready { description } => DescriptionAi::Ready {
                            source,
                            description,
                            rationale: value.rationale,
                        },
                    },
                };
                cx.notify();
            })
            .log_warn();
        }));
        cx.notify();
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.request = None;
        self.assist = DescriptionAi::Idle;
        cx.notify();
    }

    fn visible_assist(&self, name: &str, description: &str) -> DescriptionAi {
        match self.assist.source() {
            Some(source) if source != description_source(name, description) => DescriptionAi::Idle,
            _ => self.assist.clone(),
        }
    }

    fn render_proposal(
        &self,
        description: &str,
        rationale: &str,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let accepted = description.to_string();
        div()
            .id("description-proposal")
            .bg(roles.ai_fill)
            .px(px(12.))
            .py(px(8.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .type_body()
                    .text_color(roles.ink)
                    .child(description.to_string()),
            )
            .child(div().text_color(roles.ink).child(rationale.to_string()))
            .child(div().text_color(roles.muted).child(format!(
                "{} / {DESCRIPTION_MAX_LENGTH} characters",
                description.encode_utf16().count()
            )))
            .child(
                ui::row()
                    .mt(px(4.))
                    .gap(px(8.))
                    .child(
                        ui::button("description-accept", "Accept", ButtonKind::Strong, roles)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.emit(FrontmatterEvent::Edit(
                                    SkillField::Description,
                                    accepted.clone(),
                                ));
                                this.dismiss(cx);
                            })),
                    )
                    .child(
                        ui::button("description-retry", "Try again", ButtonKind::Default, roles)
                            .on_click(cx.listener(|this, _, _, cx| this.ask(cx))),
                    )
                    .child(
                        ui::button("description-reject", "Reject", ButtonKind::Default, roles)
                            .on_click(cx.listener(|this, _, _, cx| this.dismiss(cx))),
                    ),
            )
    }

    fn render_improve(
        &self,
        working: bool,
        empty: bool,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Div {
        let disabled = empty || working;
        ui::row().child(
            ui::sized_button(
                "improve-description",
                if working {
                    "Improving…"
                } else {
                    "Improve description"
                },
                ButtonKind::Default,
                ButtonSize::Regular,
                ui::Control::disabled(disabled),
                roles,
            )
            .when(!disabled, |this| {
                this.on_click(cx.listener(|this, _, _, cx| this.ask(cx)))
            }),
        )
    }

    fn render_assist(
        &self,
        name: &str,
        description: &str,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let state = self.visible_assist(name, description);
        let empty = description.trim().is_empty();
        div()
            .type_small()
            .map(|this| match &state {
                DescriptionAi::Ready {
                    description,
                    rationale,
                    ..
                } => this.child(self.render_proposal(description, rationale, roles, cx)),
                other => this.child(self.render_improve(
                    matches!(other, DescriptionAi::Working { .. }),
                    empty,
                    roles,
                    cx,
                )),
            })
            .when_some(
                match &state {
                    DescriptionAi::Failed { message, .. } => Some(message.clone()),
                    _ => None,
                },
                |this, message| this.child(ui::error_text(message, roles).mt(px(4.))),
            )
    }

    fn render_fields(
        &self,
        name_focused: bool,
        description_focused: bool,
        name: &str,
        description: &str,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Div {
        let name_problems = messages(problems_of_field(&self.report, CheckedField::Name));
        let description_problems =
            messages(problems_of_field(&self.report, CheckedField::Description));
        let others = messages(other_problems(&self.report));
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .child(ui::label("name", roles))
                    .child(ui::field(
                        Input::new(&self.name).appearance(false),
                        name_focused,
                        !name_problems.is_empty(),
                        roles,
                    ))
                    .when(!name_problems.is_empty(), |this| {
                        this.child(ui::problem_list(name_problems, roles))
                    }),
            )
            .child(
                div()
                    .child(ui::label("description", roles))
                    .child(ui::field(
                        Textarea::new(&self.description).appearance(false),
                        description_focused,
                        !description_problems.is_empty(),
                        roles,
                    ))
                    .when(!description_problems.is_empty(), |this| {
                        this.child(ui::problem_list(description_problems, roles))
                    }),
            )
            .child(self.render_assist(name, description, roles, cx))
            .when(!others.is_empty(), |this| {
                this.child(ui::problem_list(others, roles))
            })
    }
}

impl Focusable for FrontmatterView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for FrontmatterView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let roles = theme::roles(cx);
        let panel = div()
            .id("frontmatter-panel")
            .track_focus(&self.focus)
            .w_full()
            .mb(px(16.))
            .px(px(16.))
            .py(px(12.))
            .bg(roles.raised)
            .text_color(roles.ink)
            .type_body()
            .flex()
            .flex_col();
        let title = ui::title("Frontmatter", roles);
        match self.report.clone() {
            FrontmatterReport::None => div().into_any_element(),
            FrontmatterReport::Unreadable { skill, problems } => panel
                .child(title)
                .child(ui::problem_list(messages(problems), roles))
                .when(skill, |this| {
                    this.child(
                        ui::meta(
                            "The fields come back when the frontmatter is fixed in the editor.",
                            roles,
                        )
                        .mt(px(4.)),
                    )
                })
                .into_any_element(),
            FrontmatterReport::Skill {
                name,
                description,
                present,
                ..
            } => {
                let name_focused = self.name.read(cx).focus_handle(cx).is_focused(window);
                let description_focused = self
                    .description
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window);
                panel
                    .child(
                        ui::row()
                            .mb(px(8.))
                            .justify_between()
                            .gap(px(12.))
                            .child(title)
                            .child(ui::meta(
                                if present {
                                    "Edits change only name and description in the YAML"
                                } else {
                                    "Typing adds the frontmatter to the top of the file"
                                },
                                roles,
                            )),
                    )
                    .child(self.render_fields(
                        name_focused,
                        description_focused,
                        &name,
                        &description,
                        roles,
                        cx,
                    ))
                    .into_any_element()
            }
        }
    }
}
