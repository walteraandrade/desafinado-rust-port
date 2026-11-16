use desafinado_features::skills::{
    library::{CreateSkillRequest, LibraryRoot},
    new_skill::{new_skill_draft, new_skill_request, writable_roots},
};
use gpui_kit::{
    component::input::{Input, InputEvent, InputState},
    prelude::FluentBuilder as _,
    *,
};

use crate::{
    theme::{self, Roles},
    ui::{self, ButtonKind, ButtonSize},
};

actions!(new_skill, [CancelNewSkill, SubmitNewSkill]);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CancelNewSkill, Some("NewSkillDialog")),
        KeyBinding::new("enter", SubmitNewSkill, Some("NewSkillDialog > Input")),
    ]);
}

pub enum NewSkillEvent {
    Create(CreateSkillRequest),
    Close,
}

impl EventEmitter<NewSkillEvent> for NewSkillDialog {}

pub struct NewSkillDialog {
    roots: Vec<LibraryRoot>,
    root: Option<String>,
    name: Entity<InputState>,
    busy: bool,
    failure: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl NewSkillDialog {
    pub fn new(roots: &[LibraryRoot], window: &mut Window, cx: &mut Context<Self>) -> Self {
        let roots: Vec<LibraryRoot> = writable_roots(roots).into_iter().cloned().collect();
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("pdf-processing"));
        name.update(cx, |state, cx| state.focus(window, cx));
        let changes = cx.subscribe(&name, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.failure = None;
                cx.notify();
            }
        });
        Self {
            root: roots.first().map(|root| root.root.clone()),
            roots,
            name,
            busy: false,
            failure: None,
            _subscriptions: vec![changes],
        }
    }

    fn chosen(&self) -> Option<&LibraryRoot> {
        self.roots
            .iter()
            .find(|root| Some(&root.root) == self.root.as_ref())
    }

    pub fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        self.busy = busy;
        cx.notify();
    }

    pub fn fail(&mut self, reason: String, cx: &mut Context<Self>) {
        self.busy = false;
        self.failure = Some(reason);
        cx.notify();
    }

    fn submit(&mut self, _: &SubmitNewSkill, _: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value().to_string();
        let Some(root) = self.chosen() else {
            return;
        };
        if self.busy || !new_skill_draft(&name, Some(root)).ready {
            return;
        }
        let request = new_skill_request(&name, root);
        self.busy = true;
        self.failure = None;
        cx.emit(NewSkillEvent::Create(request));
        cx.notify();
    }

    fn cancel(&mut self, _: &CancelNewSkill, _: &mut Window, cx: &mut Context<Self>) {
        if !self.busy {
            cx.emit(NewSkillEvent::Close);
        }
    }

    fn render_roots(&self, roles: Roles, cx: &mut Context<Self>) -> AnyElement {
        if self.roots.is_empty() {
            return div()
                .text_size(px(14.))
                .text_color(roles.danger)
                .child("No folder in the Library can be read. Add one first.")
                .into_any_element();
        }
        div()
            .flex()
            .flex_col()
            .border_1()
            .border_color(roles.fill)
            .bg(roles.fill)
            .children(self.roots.iter().enumerate().map(|(index, root)| {
                let chosen = Some(&root.root) == self.root.as_ref();
                let value = root.root.clone();
                div()
                    .id(("new-skill-root", index))
                    .h(px(28.))
                    .px(px(10.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(14.))
                    .text_color(roles.ink)
                    .border_1()
                    .border_color(if chosen { roles.accent } else { roles.fill })
                    .when(chosen, |this| this.bg(roles.raised))
                    .when(!self.busy, |this| {
                        this.cursor_pointer()
                            .hover(|this| this.bg(roles.fill_hover))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.root = Some(value.clone());
                                this.failure = None;
                                cx.notify();
                            }))
                    })
                    .child(
                        div()
                            .size(px(10.))
                            .flex_none()
                            .border_1()
                            .border_color(roles.accent)
                            .when(chosen, |this| this.bg(roles.accent)),
                    )
                    .child(div().truncate().child(root.label.clone()))
            }))
            .into_any_element()
    }
}

impl Render for NewSkillDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let roles = theme::roles(cx);
        let name = self.name.read(cx).value().to_string();
        let draft = new_skill_draft(&name, self.chosen());
        let problems = match &self.failure {
            Some(failure) => vec![failure.clone()],
            None => draft.problems.clone(),
        };
        let focused = self.name.read(cx).focus_handle(cx).is_focused(window);
        let ready = draft.ready && !self.busy;
        ui::modal_backdrop(
            "new-skill-backdrop",
            roles,
            ui::dialog_frame("new-skill", px(480.), roles)
                .key_context("NewSkillDialog")
                .on_action(cx.listener(Self::submit))
                .on_action(cx.listener(Self::cancel))
                .child(ui::title("New skill", roles))
                .child(
                    ui::detail(
                        "Creates a folder and a SKILL.md from a template, then opens it.",
                        roles,
                    )
                    .text_size(px(14.)),
                )
                .child(
                    div()
                        .mt(px(16.))
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .child(
                            div()
                                .child(ui::label("Folder", roles))
                                .child(self.render_roots(roles, cx)),
                        )
                        .child(
                            div()
                                .child(ui::label("Name", roles))
                                .child(ui::field(
                                    Input::new(&self.name).appearance(false).disabled(self.busy),
                                    focused,
                                    !problems.is_empty(),
                                    roles,
                                ))
                                .when(!problems.is_empty(), |this| {
                                    this.child(ui::problem_list(problems, roles))
                                })
                                .child(
                                    div()
                                        .mt(px(4.))
                                        .truncate()
                                        .text_size(px(12.))
                                        .text_color(roles.faint)
                                        .child(draft.location.clone().unwrap_or_else(|| {
                                            "Lowercase letters, digits and hyphens".into()
                                        })),
                                ),
                        ),
                )
                .child(
                    ui::actions_row()
                        .child(
                            ui::sized_button(
                                "new-skill-cancel",
                                "Cancel",
                                ButtonKind::Default,
                                ButtonSize::Regular, ui::Control::disabled(self.busy),
                                roles,)
                            .when(!self.busy, |this| {
                                this.on_click(
                                    cx.listener(|_, _, _, cx| cx.emit(NewSkillEvent::Close)),
                                )
                            }),
                        )
                        .child(
                            ui::sized_button(
                                "new-skill-create",
                                "Create",
                                ButtonKind::Strong,
                                ButtonSize::Regular, ui::Control::disabled(!ready),
                                roles,)
                            .when(ready, |this| {
                                this.on_click(cx.listener(|this, _, window, cx| {
                                    this.submit(&SubmitNewSkill, window, cx)
                                }))
                            }),
                        ),
                ),
        )
    }
}
