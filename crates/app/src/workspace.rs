use std::path::PathBuf;

use gpui_kit::{component::TitleBar, prelude::FluentBuilder as _, *};

use crate::{
    ai_backend,
    editor_pane::{EditorPane, PaneEvent},
    theme::{self, Roles},
    ui::{self, ButtonKind},
};

actions!(
    desafinado,
    [
        NewDocument,
        OpenDocument,
        SaveDocument,
        SaveDocumentAs,
        OpenPreferences,
        StartReview,
        StopReview,
        ConfirmReview,
        AcceptAll,
        RejectAll,
        CloseMenu,
    ]
);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-n", NewDocument, None),
        KeyBinding::new("ctrl-o", OpenDocument, None),
        KeyBinding::new("ctrl-s", SaveDocument, None),
        KeyBinding::new("ctrl-shift-s", SaveDocumentAs, None),
        KeyBinding::new("ctrl-,", OpenPreferences, None),
        KeyBinding::new("escape", CloseMenu, Some("Menu")),
        KeyBinding::new("enter", ConfirmReview, Some("ReviewPopover > Input")),
        KeyBinding::new("escape", CloseMenu, Some("ReviewPopover > Input")),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuKind {
    File,
    Edit,
}

#[derive(Clone, Debug, PartialEq)]
enum LeaveIntent {
    New,
    Open,
    OpenPath(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LeaveChoice {
    Save,
    Discard,
    Cancel,
}

struct MenuRow {
    label: &'static str,
    shortcut: Option<&'static str>,
    action: Box<dyn Action>,
    disabled: bool,
    separated: bool,
}

pub struct Workspace {
    pane: Entity<EditorPane>,
    path: Option<PathBuf>,
    saved_text: String,
    open_menu: Option<MenuKind>,
    notice: Option<SharedString>,
    leave: Option<LeaveIntent>,
    review_focus: Option<Entity<gpui_kit::component::input::InputState>>,
    after_save: Option<LeaveIntent>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(launch: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let pane = cx.new(|cx| EditorPane::new(ai_backend::backend(), window, cx));
        let subscription = cx.subscribe(&pane, |_, _, _: &PaneEvent, cx| cx.notify());
        let mut workspace = Self {
            pane,
            path: None,
            saved_text: String::new(),
            open_menu: None,
            notice: None,
            leave: None,
            review_focus: None,
            after_save: None,
            focus: cx.focus_handle(),
            _subscriptions: vec![subscription],
        };
        if let Some(path) = launch {
            workspace.load(path, window, cx);
        }
        workspace
    }

    fn text(&self, cx: &App) -> String {
        self.pane.read(cx).text(cx)
    }

    fn dirty(&self, cx: &App) -> bool {
        self.text(cx) != self.saved_text
    }

    fn load(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                self.pane
                    .update(cx, |pane, cx| pane.set_text(&text, window, cx));
                self.saved_text = text;
                self.path = Some(path);
                self.notice = None;
            }
            Err(error) => {
                self.notice = Some(format!("Could not open {}: {error}", path.display()).into());
            }
        }
        cx.notify();
    }

    fn write(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.text(cx);
        let temp = path.with_extension("desafinado-tmp");
        let result = std::fs::write(&temp, &text).and_then(|_| std::fs::rename(&temp, &path));
        match result {
            Ok(()) => {
                self.saved_text = text;
                self.path = Some(path);
                self.notice = None;
                if let Some(intent) = self.after_save.take() {
                    self.continue_with(intent, window, cx);
                }
            }
            Err(error) => {
                self.after_save = None;
                self.notice = Some(format!("Could not save: {error}").into());
            }
        }
        cx.notify();
    }

    fn guard(&mut self, intent: LeaveIntent, window: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = None;
        if self.dirty(cx) {
            self.leave = Some(intent);
            cx.notify();
        } else {
            self.continue_with(intent, window, cx);
        }
    }

    fn continue_with(&mut self, intent: LeaveIntent, window: &mut Window, cx: &mut Context<Self>) {
        match intent {
            LeaveIntent::New => self.clear_document(window, cx),
            LeaveIntent::Open => self.prompt_open(window, cx),
            LeaveIntent::OpenPath(path) => self.load(path, window, cx),
        }
    }

    fn answer_leave(&mut self, choice: LeaveChoice, window: &mut Window, cx: &mut Context<Self>) {
        let Some(intent) = self.leave.take() else {
            return;
        };
        match choice {
            LeaveChoice::Cancel => {}
            LeaveChoice::Discard => self.continue_with(intent, window, cx),
            LeaveChoice::Save => {
                self.after_save = Some(intent);
                self.save_document(&SaveDocument, window, cx);
            }
        }
        cx.notify();
    }

    fn new_document(&mut self, _: &NewDocument, window: &mut Window, cx: &mut Context<Self>) {
        self.guard(LeaveIntent::New, window, cx);
    }

    fn clear_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pane
            .update(cx, |pane, cx| pane.set_text("", window, cx));
        self.saved_text = String::new();
        self.path = None;
        cx.notify();
    }

    fn open_document(&mut self, _: &OpenDocument, window: &mut Window, cx: &mut Context<Self>) {
        self.guard(LeaveIntent::Open, window, cx);
    }

    fn prompt_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = paths.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = this.update_in(cx, |this, window, cx| this.load(path, window, cx));
            }
        })
        .detach();
    }

    fn save_document(&mut self, _: &SaveDocument, window: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = None;
        match self.path.clone() {
            Some(path) => self.write(path, window, cx),
            None => self.save_document_as(&SaveDocumentAs, window, cx),
        }
    }

    fn save_document_as(
        &mut self,
        _: &SaveDocumentAs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_menu = None;
        let directory = self
            .path
            .as_ref()
            .and_then(|path| path.parent().map(PathBuf::from))
            .or_else(dirs_home)
            .unwrap_or_else(|| PathBuf::from("."));
        let name = self
            .path
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled.md".into());
        let target = cx.prompt_for_new_path(&directory, Some(&name));
        cx.spawn_in(window, async move |this, cx| match target.await {
            Ok(Ok(Some(path))) => {
                let _ = this.update_in(cx, |this, window, cx| this.write(path, window, cx));
            }
            _ => {
                let _ = this.update(cx, |this, _| this.after_save = None);
            }
        })
        .detach();
    }

    fn close_menu(&mut self, _: &CloseMenu, window: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = None;
        if self.review_focus.take().is_some() {
            self.pane.update(cx, |pane, cx| pane.focus(window, cx));
        }
        cx.notify();
    }

    fn menu_rows(&self, kind: MenuKind, cx: &App) -> Vec<MenuRow> {
        let open = self.pane.read(cx).open_count();
        match kind {
            MenuKind::File => vec![
                row("New", Some("Ctrl+N"), Box::new(NewDocument), false),
                row("Open…", Some("Ctrl+O"), Box::new(OpenDocument), false),
                row("Save", Some("Ctrl+S"), Box::new(SaveDocument), false),
                row(
                    "Save as…",
                    Some("Ctrl+Shift+S"),
                    Box::new(SaveDocumentAs),
                    false,
                ),
                MenuRow {
                    separated: true,
                    ..row(
                        "Preferences…",
                        Some("Ctrl+,"),
                        Box::new(OpenPreferences),
                        false,
                    )
                },
            ],
            MenuKind::Edit => vec![
                if self.pane.read(cx).review.is_some() {
                    row("Stop Review", None, Box::new(StopReview), false)
                } else {
                    row("Review…", None, Box::new(StartReview), false)
                },
                MenuRow {
                    separated: true,
                    ..row(
                        "Accept all suggestions",
                        None,
                        Box::new(AcceptAll),
                        open == 0,
                    )
                },
                row(
                    "Reject all suggestions",
                    None,
                    Box::new(RejectAll),
                    open == 0,
                ),
            ],
        }
    }

    fn render_menu_button(
        &self,
        kind: MenuKind,
        label: &'static str,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let open = self.open_menu == Some(kind);
        div()
            .id(label)
            .h(px(26.))
            .px(px(10.))
            .flex()
            .items_center()
            .text_size(px(13.))
            .text_color(roles.ink)
            .border_1()
            .border_color(if open {
                roles.hairline
            } else {
                transparent_black()
            })
            .when(open, |this| this.bg(roles.fill))
            .hover(|this| this.bg(roles.fill))
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.open_menu = if this.open_menu == Some(kind) {
                        None
                    } else {
                        Some(kind)
                    };
                    cx.notify();
                }),
            )
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered && this.open_menu.is_some() && this.open_menu != Some(kind) {
                    this.open_menu = Some(kind);
                    cx.notify();
                }
            }))
    }

    fn render_menu(
        &self,
        kind: MenuKind,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let left = match kind {
            MenuKind::File => px(8.),
            MenuKind::Edit => px(52.),
        };
        div()
            .id("menu")
            .key_context("Menu")
            .absolute()
            .top(px(38.))
            .left(left)
            .min_w(px(240.))
            .bg(roles.raised)
            .border_1()
            .border_color(roles.hairline)
            .py(px(4.))
            .children(
                self.menu_rows(kind, cx)
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| {
                        let action = item.action.boxed_clone();
                        div()
                            .when(item.separated, |this| {
                                this.border_t_1()
                                    .border_color(roles.hairline)
                                    .mt(px(4.))
                                    .pt(px(4.))
                            })
                            .child(
                                div()
                                    .id(("menu-row", index))
                                    .h(px(28.))
                                    .px(px(12.))
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(24.))
                                    .text_size(px(13.))
                                    .text_color(if item.disabled {
                                        roles.faint
                                    } else {
                                        roles.ink
                                    })
                                    .when(!item.disabled, |this| {
                                        this.hover(|this| this.bg(roles.fill).outline_accent(roles))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.open_menu = None;
                                                window.dispatch_action(action.boxed_clone(), cx);
                                                cx.notify();
                                            }))
                                    })
                                    .child(item.label)
                                    .children(item.shortcut.map(|shortcut| {
                                        div().text_color(roles.faint).child(shortcut)
                                    })),
                            )
                    }),
            )
    }

    fn open_review(&mut self, _: &StartReview, window: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = None;
        let input = cx.new(|cx| {
            gpui_kit::component::input::InputState::new(window, cx)
                .placeholder("Focus (optional), such as tone or clarity")
        });
        input.update(cx, |state, cx| state.focus(window, cx));
        self.review_focus = Some(input);
        cx.notify();
    }

    fn confirm_review(&mut self, _: &ConfirmReview, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.review_focus.take() else {
            return;
        };
        let focus = input.read(cx).value().to_string();
        self.pane.update(cx, |pane, cx| {
            pane.start_review(focus, cx);
            pane.focus(window, cx);
        });
        cx.notify();
    }

    fn stop_review(&mut self, _: &StopReview, _: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = None;
        self.pane.update(cx, |pane, cx| pane.stop_review(cx));
        cx.notify();
    }

    fn render_review_popover(
        &self,
        input: &Entity<gpui_kit::component::input::InputState>,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("review-popover")
            .key_context("ReviewPopover")
            .occlude()
            .absolute()
            .top(px(38.))
            .left(px(52.))
            .w(px(360.))
            .p(px(12.))
            .bg(roles.raised)
            .border_1()
            .border_color(roles.hairline)
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(ui::heading("Review the Document", roles).text_size(px(14.)))
            .child(div().text_size(px(13.)).child(
                "The AI reads the whole Document and suggests up to 6 changes, one per Block.",
            ))
            .child(gpui_kit::component::input::Input::new(input))
            .child(
                ui::row()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        ui::button("review-cancel", "Cancel", ButtonKind::Default, roles).on_click(
                            cx.listener(|this, _, window, cx| {
                                this.review_focus = None;
                                this.pane.update(cx, |pane, cx| pane.focus(window, cx));
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        ui::button("review-start", "Review", ButtonKind::Primary, roles).on_click(
                            cx.listener(|this, _, window, cx| {
                                this.confirm_review(&ConfirmReview, window, cx)
                            }),
                        ),
                    ),
            )
    }

    fn render_leave(&self, roles: Roles, cx: &mut Context<Self>) -> impl IntoElement {
        let answer = |choice: LeaveChoice| {
            cx.listener(
                move |this: &mut Self,
                      _: &ClickEvent,
                      window: &mut Window,
                      cx: &mut Context<Self>| {
                    this.answer_leave(choice, window, cx)
                },
            )
        };
        ui::modal_backdrop(
            "leave-backdrop",
            roles,
            ui::dialog_frame("leave", px(416.), roles)
                .key_context("LeaveDialog")
                .child(ui::heading(
                    format!("Save changes to {}?", self.file_name()),
                    roles,
                ))
                .child(ui::detail(
                    "Discard removes your unsaved changes. Cancel keeps you here.",
                    roles,
                ))
                .child(
                    ui::actions_row()
                        .child(
                            ui::button("leave-cancel", "Cancel", ButtonKind::Default, roles)
                                .on_click(answer(LeaveChoice::Cancel)),
                        )
                        .child(
                            ui::button("leave-discard", "Discard", ButtonKind::Default, roles)
                                .on_click(answer(LeaveChoice::Discard)),
                        )
                        .child(
                            ui::button("leave-save", "Save", ButtonKind::Strong, roles)
                                .on_click(answer(LeaveChoice::Save)),
                        ),
                ),
        )
    }

    fn file_name(&self) -> SharedString {
        self.path
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".into())
            .into()
    }
}

trait OutlineAccent {
    fn outline_accent(self, roles: Roles) -> Self;
}

impl OutlineAccent for StyleRefinement {
    fn outline_accent(self, roles: Roles) -> Self {
        self.border_1().border_color(roles.accent)
    }
}

fn row(
    label: &'static str,
    shortcut: Option<&'static str>,
    action: Box<dyn Action>,
    disabled: bool,
) -> MenuRow {
    MenuRow {
        label,
        shortcut,
        action,
        disabled,
        separated: false,
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let roles = theme::roles(cx);
        let dirty = self.dirty(cx);
        let review = self.pane.read(cx).review.as_ref().map(|run| run.added);
        let open = self.pane.read(cx).open_count();
        let title_bar = TitleBar::new().child(
            h_flex_row()
                .gap(px(2.))
                .child(self.render_menu_button(MenuKind::File, "File", roles, cx))
                .child(self.render_menu_button(MenuKind::Edit, "Edit", roles, cx)),
        );
        let header = h_flex_row()
            .h(px(48.))
            .px(px(16.))
            .gap(px(12.))
            .border_b_1()
            .border_color(roles.hairline)
            .child(
                div()
                    .font_family("League Spartan")
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(24.))
                    .text_color(roles.ink)
                    .child("Desafinado"),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(roles.ink)
                    .child(format!(
                        "{}{}",
                        self.file_name(),
                        if dirty { " •" } else { "" }
                    )),
            )
            .children(self.notice.clone().map(|notice| {
                div()
                    .text_size(px(13.))
                    .text_color(roles.danger)
                    .child(notice)
            }))
            .child(div().flex_1())
            .when_some(review, |this, added| {
                this.child(
                    ui::row()
                        .gap(px(8.))
                        .text_size(px(13.))
                        .child(ui::ai_flag(roles))
                        .child(div().text_color(roles.ai).child(format!(
                            "Reviewing · {added} {}",
                            if added == 1 {
                                "suggestion"
                            } else {
                                "suggestions"
                            }
                        )))
                        .child(
                            ui::sized_button(
                                "header-stop",
                                "Stop",
                                ButtonKind::Default,
                                ui::ButtonSize::Small,
                                false,
                                roles,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| this.stop_review(&StopReview, window, cx),
                            )),
                        ),
                )
            })
            .when(open > 0, |this| {
                this.child(
                    ui::row()
                        .gap(px(8.))
                        .child(
                            ui::sized_button(
                                "header-accept-all",
                                "Accept all",
                                ButtonKind::Primary,
                                ui::ButtonSize::Small,
                                false,
                                roles,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.pane.update(cx, |pane, cx| pane.accept_all(window, cx))
                                },
                            )),
                        )
                        .child(
                            ui::sized_button(
                                "header-reject-all",
                                "Reject all",
                                ButtonKind::Default,
                                ui::ButtonSize::Small,
                                false,
                                roles,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.pane.update(cx, |pane, cx| pane.reject_all(cx))
                            })),
                        ),
                )
            });
        let editor = div().flex_1().min_h_0().child(self.pane.clone());
        div()
            .id("workspace")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::new_document))
            .on_action(cx.listener(Self::open_document))
            .on_action(cx.listener(Self::save_document))
            .on_action(cx.listener(Self::save_document_as))
            .on_action(cx.listener(Self::close_menu))
            .on_action(cx.listener(Self::open_review))
            .on_action(cx.listener(Self::confirm_review))
            .on_action(cx.listener(Self::stop_review))
            .on_action(cx.listener(|this, _: &AcceptAll, window, cx| {
                this.open_menu = None;
                this.pane.update(cx, |pane, cx| pane.accept_all(window, cx));
            }))
            .on_action(cx.listener(|this, _: &RejectAll, _, cx| {
                this.open_menu = None;
                this.pane.update(cx, |pane, cx| pane.reject_all(cx));
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(roles.canvas)
            .text_color(roles.ink)
            .font_family("Inter")
            .child(title_bar)
            .child(header)
            .child(editor)
            .when_some(self.leave.as_ref().map(|_| ()), |this, _| {
                this.child(self.render_leave(roles, cx))
            })
            .when_some(self.review_focus.clone(), |this, input| {
                this.child(self.render_review_popover(&input, roles, cx))
            })
            .when_some(self.open_menu, |this, kind| {
                this.child(
                    div()
                        .id("menu-scrim")
                        .absolute()
                        .inset_0()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_menu = None;
                            cx.notify();
                        })),
                )
                .child(self.render_menu(kind, roles, cx))
            })
    }
}

fn h_flex_row() -> Div {
    div().flex().flex_row().items_center()
}
