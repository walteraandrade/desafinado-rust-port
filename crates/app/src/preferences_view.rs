use desafinado_ai::quick_action_settings::{
    QuickActionDraft, QuickActionDraftErrors, QuickActionSettings, QuickActionSlot,
    add_user_quick_action, check_quick_action_draft, draft_is_valid, is_user_quick_action,
    move_quick_action, remove_user_quick_action, toggle_quick_action, update_user_quick_action,
};
use desafinado_features::theme::{
    catalog::{ThemeOption, themes_of_mode},
    tokens::ThemeMode,
};
use gpui_kit::{
    component::input::{Input, InputEvent, InputState, Textarea, TextareaState},
    prelude::FluentBuilder as _,
    *,
};

use crate::{
    theme::{self, Roles},
    ui::{self, ButtonKind, ButtonSize, Type as _},
};

actions!(
    preferences,
    [
        ClosePreferences,
        CancelQuickActionDraft,
        SubmitQuickActionDraft
    ]
);

pub const PREFERENCES_WIDTH: f32 = 416.;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_COMMIT: &str = env!("DESAFINADO_COMMIT");

pub fn version_line() -> String {
    format!("Desafinado {APP_VERSION} · {APP_COMMIT}")
}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", ClosePreferences, Some("Preferences")),
        KeyBinding::new("escape", CancelQuickActionDraft, Some("QuickActionForm")),
        KeyBinding::new(
            "enter",
            SubmitQuickActionDraft,
            Some("QuickActionForm > QuickActionName"),
        ),
    ]);
}

pub enum PreferencesEvent {
    Close,
    OpenModel,
    OpenKey,
    OpenSpend,
    QuickActions(QuickActionSettings),
}

impl EventEmitter<PreferencesEvent> for PreferencesPanel {}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Editing {
    None,
    New,
    Edit(String),
    Delete(String),
}

struct Draft {
    label: Entity<InputState>,
    instruction: Entity<TextareaState>,
    skill_only: bool,
    errors: QuickActionDraftErrors,
    _subscriptions: Vec<Subscription>,
}

pub fn new_user_id(now_nanos: u128, counter: u32) -> String {
    format!("user-{now_nanos:x}-{counter:x}")
}

fn draft_of(slot: &QuickActionSlot) -> QuickActionDraft {
    QuickActionDraft {
        label: slot.action.label.clone(),
        instruction: slot.action.instruction.clone(),
        skill_only: slot.action.skill_only,
    }
}

pub struct PreferencesPanel {
    settings: QuickActionSettings,
    editing: Editing,
    draft: Option<Draft>,
    created: u32,
    scroll: ScrollHandle,
    focus: FocusHandle,
}

impl PreferencesPanel {
    pub fn new(settings: QuickActionSettings, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            settings,
            editing: Editing::None,
            draft: None,
            created: 0,
            scroll: ScrollHandle::new(),
            focus,
        }
    }

    fn change(
        &mut self,
        edit: impl FnOnce(&QuickActionSettings) -> QuickActionSettings,
        cx: &mut Context<Self>,
    ) {
        let next = edit(&self.settings);
        if next == self.settings {
            return;
        }
        self.settings = next.clone();
        cx.emit(PreferencesEvent::QuickActions(next));
        cx.notify();
    }

    fn start_draft(
        &mut self,
        editing: Editing,
        initial: QuickActionDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = cx.new(|cx| InputState::new(window, cx).default_value(initial.label.clone()));
        let instruction = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(4, 10)
                .placeholder("What the AI should do with the text")
                .default_value(initial.instruction.clone())
        });
        label.update(cx, |input, cx| input.focus(window, cx));
        let clear =
            |this: &mut Self, _: Entity<InputState>, event: &InputEvent, cx: &mut Context<Self>| {
                if matches!(event, InputEvent::Change)
                    && let Some(draft) = this.draft.as_mut()
                {
                    draft.errors = QuickActionDraftErrors::default();
                    cx.notify();
                }
            };
        let clear_instruction = |this: &mut Self,
                                 _: Entity<TextareaState>,
                                 event: &InputEvent,
                                 cx: &mut Context<Self>| {
            if matches!(event, InputEvent::Change)
                && let Some(draft) = this.draft.as_mut()
            {
                draft.errors = QuickActionDraftErrors::default();
                cx.notify();
            }
        };
        let subscriptions = vec![
            cx.subscribe(&label, clear),
            cx.subscribe(&instruction, clear_instruction),
        ];
        self.draft = Some(Draft {
            label,
            instruction,
            skill_only: initial.skill_only,
            errors: QuickActionDraftErrors::default(),
            _subscriptions: subscriptions,
        });
        self.editing = editing;
        cx.notify();
    }

    fn end_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.draft = None;
        self.editing = Editing::None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn cancel_draft(
        &mut self,
        _: &CancelQuickActionDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.editing, Editing::Delete(_)) {
            self.editing = Editing::None;
            cx.notify();
            return;
        }
        self.end_draft(window, cx);
    }

    fn submit_draft(
        &mut self,
        _: &SubmitQuickActionDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(draft) = self.draft.as_mut() else {
            return;
        };
        let value = QuickActionDraft {
            label: draft.label.read(cx).value().to_string(),
            instruction: draft.instruction.read(cx).value().to_string(),
            skill_only: draft.skill_only,
        };
        let editing = match &self.editing {
            Editing::Edit(id) => Some(id.clone()),
            _ => None,
        };
        let errors = check_quick_action_draft(&self.settings, &value, editing.as_deref());
        if !draft_is_valid(&errors) {
            let to_label = errors.label.is_some();
            draft.errors = errors;
            if to_label {
                draft.label.update(cx, |input, cx| input.focus(window, cx));
            } else {
                draft
                    .instruction
                    .update(cx, |input, cx| input.focus(window, cx));
            }
            cx.notify();
            return;
        }
        match editing {
            Some(id) => self.change(|s| update_user_quick_action(s, &id, &value), cx),
            None => {
                self.created += 1;
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_nanos())
                    .unwrap_or(0);
                let id = new_user_id(nanos, self.created);
                self.change(|s| add_user_quick_action(s, &value, &id), cx);
            }
        }
        self.end_draft(window, cx);
    }

    fn close(&mut self, _: &ClosePreferences, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(PreferencesEvent::Close);
    }

    fn render_theme_row(
        &self,
        option: &ThemeOption,
        selected: &str,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let checked = option.id == selected;
        let id = option.id.clone();
        ui::option_row(
            SharedString::from(format!("theme-{}", option.id)),
            false,
            checked,
            roles,
        )
        .gap(px(12.))
        .px(px(8.))
        .py(px(8.))
        .on_click(cx.listener(move |_, _, _, cx| theme::choose(&id, cx)))
        .child(ui::check(checked, roles))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .child(div().truncate().child(option.title.clone()))
                .child(
                    div()
                        .truncate()
                        .type_small()
                        .text_color(roles.muted)
                        .child(option.origin.clone()),
                ),
        )
        .child(
            div().flex().flex_row().flex_none().children(
                theme::swatches(option)
                    .into_iter()
                    .map(|color| div().w(px(12.)).h(px(20.)).bg(color)),
            ),
        )
        .into_any_element()
    }

    fn render_theme_group(
        &self,
        label: &'static str,
        options: Vec<ThemeOption>,
        selected: &str,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Div {
        div().mt(px(16.)).child(ui::label(label, roles)).children(
            options
                .iter()
                .map(|option| self.render_theme_row(option, selected, roles, cx)),
        )
    }

    fn render_setting_row(
        &self,
        id: &'static str,
        title: &'static str,
        detail: &'static str,
        action: (&'static str, fn() -> PreferencesEvent),
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Div {
        let (label, event) = action;
        ui::row()
            .gap(px(12.))
            .py(px(8.))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .child(title)
                    .child(ui::meta(detail, roles)),
            )
            .child(
                ui::small_button(id, label, ButtonKind::Default, roles)
                    .on_click(cx.listener(move |_, _, _, cx| cx.emit(event()))),
            )
    }

    fn render_draft(&self, roles: Roles, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(draft) = self.draft.as_ref() else {
            return div().into_any_element();
        };
        let label_focused = draft.label.read(cx).focus_handle(cx).is_focused(window);
        let instruction_focused = draft
            .instruction
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let skill_only = draft.skill_only;
        let submit = if matches!(self.editing, Editing::New) {
            "Add"
        } else {
            "Save"
        };
        div()
            .id("quick-action-form")
            .key_context("QuickActionForm")
            .my(px(8.))
            .p(px(12.))
            .bg(roles.canvas)
            .flex()
            .flex_col()
            .child(ui::label("Name", roles))
            .child(div().key_context("QuickActionName").child(ui::field(
                Input::new(&draft.label).appearance(false),
                label_focused,
                draft.errors.label.is_some(),
                roles,
            )))
            .when_some(draft.errors.label.clone(), |this, error| {
                this.child(ui::problem_list(vec![error], roles))
            })
            .child(ui::label("Instruction", roles).mt(px(12.)))
            .child(ui::field(
                Textarea::new(&draft.instruction).appearance(false),
                instruction_focused,
                draft.errors.instruction.is_some(),
                roles,
            ))
            .when_some(draft.errors.instruction.clone(), |this, error| {
                this.child(ui::problem_list(vec![error], roles))
            })
            .child(
                ui::option_row("quick-action-skill-only", false, false, roles)
                    .mt(px(8.))
                    .px(px(8.))
                    .py(px(8.))
                    .gap(px(12.))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(draft) = this.draft.as_mut() {
                            draft.skill_only = !draft.skill_only;
                            cx.notify();
                        }
                    }))
                    .child(ui::check(skill_only, roles))
                    .child("Only in skills"),
            )
            .child(
                ui::row()
                    .mt(px(12.))
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        ui::button("quick-action-cancel", "Cancel", ButtonKind::Default, roles)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.cancel_draft(&CancelQuickActionDraft, window, cx)
                            })),
                    )
                    .child(
                        ui::button("quick-action-submit", submit, ButtonKind::Strong, roles)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.submit_draft(&SubmitQuickActionDraft, window, cx)
                            })),
                    ),
            )
            .into_any_element()
    }

    fn render_quick_action(
        &self,
        index: usize,
        slot: &QuickActionSlot,
        roles: Roles,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = slot.action.id.clone();
        if self.editing == Editing::Edit(id.clone()) {
            return self.render_draft(roles, window, cx);
        }
        if self.editing == Editing::Delete(id.clone()) {
            return self.render_delete(index, slot, roles, cx);
        }
        let on = slot.on;
        let toggle_id = id.clone();
        ui::row()
            .id(("quick-action", index))
            .min_h(px(36.))
            .gap(px(8.))
            .child(
                ui::option_row(("quick-action-toggle", index), false, false, roles)
                    .min_w_0()
                    .flex_1()
                    .px(px(8.))
                    .py(px(8.))
                    .gap(px(12.))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.change(|s| toggle_quick_action(s, &toggle_id, !on), cx)
                    }))
                    .child(ui::check(on, roles))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .child(slot.action.label.clone()),
                    )
                    .when(slot.action.skill_only, |this| {
                        this.child(ui::meta("Skill", roles).flex_none())
                    }),
            )
            .child(self.render_quick_action_buttons(index, slot, roles, cx))
            .into_any_element()
    }

    fn render_delete(
        &self,
        index: usize,
        slot: &QuickActionSlot,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = slot.action.id.clone();
        let keep = id.clone();
        ui::row()
            .id(("quick-action", index))
            .min_h(px(36.))
            .gap(px(8.))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .pl(px(8.))
                    .child(format!("Delete “{}”?", slot.action.label)),
            )
            .child(
                ui::row()
                    .gap(px(4.))
                    .child(
                        ui::small_button(("keep", index), "Keep", ButtonKind::Default, roles)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.editing == Editing::Delete(keep.clone()) {
                                    this.editing = Editing::None;
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        ui::small_button(
                            ("confirm-delete", index),
                            "Delete",
                            ButtonKind::Danger,
                            roles,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.editing = Editing::None;
                            this.change(|s| remove_user_quick_action(s, &id), cx);
                        })),
                    ),
            )
            .into_any_element()
    }

    fn render_quick_action_buttons(
        &self,
        index: usize,
        slot: &QuickActionSlot,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = slot.action.id.clone();
        let last = index + 1 == self.settings.len();
        let busy = matches!(self.editing, Editing::New | Editing::Edit(_));
        let mine = is_user_quick_action(&slot.action);
        let up_id = id.clone();
        let down_id = id.clone();
        let edit_slot = slot.clone();
        let small = |name: &'static str, label: &'static str, kind: ButtonKind, disabled: bool| {
            ui::sized_button(
                SharedString::from(format!("{name}-{index}")),
                label,
                kind,
                ButtonSize::Small,
                ui::Control::disabled(disabled),
                roles,
            )
        };
        ui::row()
            .flex_none()
            .gap(px(4.))
            .child(
                small("up", "↑", ButtonKind::Default, index == 0).when(index > 0, |this| {
                    this.on_click(cx.listener(move |this, _, _, cx| {
                        this.change(|s| move_quick_action(s, &up_id, -1), cx)
                    }))
                }),
            )
            .child(
                small("down", "↓", ButtonKind::Default, last).when(!last, |this| {
                    this.on_click(cx.listener(move |this, _, _, cx| {
                        this.change(|s| move_quick_action(s, &down_id, 1), cx)
                    }))
                }),
            )
            .when(mine, |this| {
                this.child(
                    small("edit", "Edit", ButtonKind::Default, busy).when(!busy, |this| {
                        this.on_click(cx.listener(move |this, _, window, cx| {
                            this.start_draft(
                                Editing::Edit(edit_slot.action.id.clone()),
                                draft_of(&edit_slot),
                                window,
                                cx,
                            )
                        }))
                    }),
                )
                .child(
                    small("delete", "Delete", ButtonKind::Danger, false).on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.editing = Editing::Delete(id.clone());
                            this.draft = None;
                            cx.notify();
                        },
                    )),
                )
            })
    }

    fn render_theme_section(&self, roles: Roles, cx: &mut Context<Self>) -> Div {
        let state = theme::state(cx).clone();
        let selected = state.selected();
        let of_mode =
            |mode| -> Vec<ThemeOption> { themes_of_mode(mode).into_iter().cloned().collect() };
        div()
            .child(ui::title("Theme", roles).pt(px(16.)))
            .when_some(state.omarchy_option(), |this, option| {
                this.child(self.render_theme_group(
                    "This machine",
                    vec![option],
                    &selected,
                    roles,
                    cx,
                ))
            })
            .child(self.render_theme_group("Dark", of_mode(ThemeMode::Dark), &selected, roles, cx))
            .child(self.render_theme_group(
                "Light",
                of_mode(ThemeMode::Light),
                &selected,
                roles,
                cx,
            ))
            .child(ui::meta(
                "Colors come from A Dictionary of Color Combinations by Sanzo Wada (Seigensha), with data by Matt DesLauriers. Each palette joins two of the book's combinations, so the pairing is ours, not Wada's. Names are Duke Ellington compositions.",
                roles,
            ).mt(px(20.)))
    }

    fn render_ai_section(&self, roles: Roles, cx: &mut Context<Self>) -> Div {
        div()
            .child(ui::title("AI", roles).mt(px(24.)))
            .child(self.render_setting_row(
                "preferences-model",
                "Model",
                "The OpenRouter model that writes Suggestions",
                ("Change", || PreferencesEvent::OpenModel),
                roles,
                cx,
            ))
            .child(self.render_setting_row(
                "preferences-key",
                "OpenRouter key",
                "Lets Desafinado call OpenRouter",
                ("Change", || PreferencesEvent::OpenKey),
                roles,
                cx,
            ))
            .child(self.render_setting_row(
                "preferences-spend",
                "AI spend",
                "Calls and cost from this machine's records",
                ("Show", || PreferencesEvent::OpenSpend),
                roles,
                cx,
            ))
    }

    fn render_quick_actions_section(
        &self,
        roles: Roles,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let settings = self.settings.clone();
        let editing_any = matches!(self.editing, Editing::Edit(_));
        let quick_actions = settings
            .iter()
            .enumerate()
            .map(|(index, slot)| self.render_quick_action(index, slot, roles, window, cx))
            .collect::<Vec<_>>();
        let new_form =
            matches!(self.editing, Editing::New).then(|| self.render_draft(roles, window, cx));
        div()
            .child(ui::title("Quick actions", roles).pt(px(24.)))
            .child(
                ui::meta(
                    "The ones you turn on show in the Ask AI bubble, in this order.",
                    roles,
                )
                .mt(px(4.)),
            )
            .child(div().mt(px(8.)).children(quick_actions))
            .map(|this| match new_form {
                Some(form) => this.child(form),
                None => this.child(
                    div().mt(px(12.)).flex().child(
                        ui::sized_button(
                            "quick-action-new",
                            "New Quick action",
                            ButtonKind::Default,
                            ButtonSize::Regular,
                            ui::Control::disabled(editing_any),
                            roles,
                        )
                        .when(!editing_any, |this| {
                            this.on_click(cx.listener(|this, _, window, cx| {
                                this.start_draft(
                                    Editing::New,
                                    QuickActionDraft {
                                        label: String::new(),
                                        instruction: String::new(),
                                        skill_only: false,
                                    },
                                    window,
                                    cx,
                                )
                            }))
                        }),
                    ),
                ),
            })
    }
}

impl Focusable for PreferencesPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for PreferencesPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let roles = theme::roles(cx);
        div()
            .id("preferences")
            .key_context("Preferences")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::close))
            .on_action(cx.listener(Self::cancel_draft))
            .on_action(cx.listener(Self::submit_draft))
            .occlude()
            .size_full()
            .flex()
            .flex_col()
            .bg(roles.raised)
            .type_body()
            .text_color(roles.ink)
            .child(
                ui::row()
                    .justify_between()
                    .gap(px(8.))
                    .px(px(20.))
                    .py(px(16.))
                    .child(ui::title("Preferences", roles))
                    .child(
                        ui::button("preferences-done", "Done", ButtonKind::Default, roles)
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(PreferencesEvent::Close))),
                    ),
            )
            .child(ui::scrolled_rest(
                div()
                    .id("preferences-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .px(px(20.))
                    .pb(px(20.))
                    .child(self.render_theme_section(roles, cx))
                    .child(self.render_ai_section(roles, cx))
                    .child(self.render_quick_actions_section(roles, window, cx))
                    .child(ui::meta(version_line(), roles).mt(px(32.))),
                &self.scroll,
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::{new_user_id, version_line};
    use desafinado_ai::quick_action_settings::is_user_quick_action_id;

    #[test]
    fn makes_ids_that_count_as_the_writer_s_own() {
        let id = new_user_id(0x1234_5678_9abc_def0, 3);
        assert_eq!(id, "user-123456789abcdef0-3");
        assert!(is_user_quick_action_id(&id));
        assert!(is_user_quick_action_id(&new_user_id(u128::MAX, u32::MAX)));
    }

    #[test]
    fn names_the_version_and_commit() {
        assert!(version_line().starts_with("Desafinado "));
        assert!(version_line().contains(" · "));
    }
}
