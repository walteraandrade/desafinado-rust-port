use std::path::PathBuf;

use gpui_kit::{
    component::{
        TitleBar,
        input::{Editor, EditorState, InputEvent},
    },
    prelude::FluentBuilder as _,
    *,
};

use crate::theme::{self, Roles};

actions!(
    desafinado,
    [
        NewDocument,
        OpenDocument,
        SaveDocument,
        SaveDocumentAs,
        OpenPreferences,
        StartReview,
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
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuKind {
    File,
    Edit,
}

struct MenuRow {
    label: &'static str,
    shortcut: Option<&'static str>,
    action: Box<dyn Action>,
    disabled: bool,
    separated: bool,
}

pub struct Workspace {
    editor: Entity<EditorState>,
    path: Option<PathBuf>,
    saved_text: String,
    open_menu: Option<MenuKind>,
    notice: Option<SharedString>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(launch: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("markdown")
                .line_number(false)
                .soft_wrap(true)
        });
        let subscription = cx.subscribe(&editor, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let mut workspace = Self {
            editor,
            path: None,
            saved_text: String::new(),
            open_menu: None,
            notice: None,
            focus: cx.focus_handle(),
            _subscriptions: vec![subscription],
        };
        if let Some(path) = launch {
            workspace.load(path, window, cx);
        }
        workspace
    }

    fn text(&self, cx: &App) -> String {
        self.editor.read(cx).value().to_string()
    }

    fn dirty(&self, cx: &App) -> bool {
        self.text(cx) != self.saved_text
    }

    fn load(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                self.editor
                    .update(cx, |state, cx| state.set_value(text.clone(), window, cx));
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

    fn write(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let text = self.text(cx);
        let temp = path.with_extension("desafinado-tmp");
        let result = std::fs::write(&temp, &text).and_then(|_| std::fs::rename(&temp, &path));
        match result {
            Ok(()) => {
                self.saved_text = text;
                self.path = Some(path);
                self.notice = None;
            }
            Err(error) => {
                self.notice = Some(format!("Could not save: {error}").into());
            }
        }
        cx.notify();
    }

    fn new_document(&mut self, _: &NewDocument, window: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = None;
        self.editor
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.saved_text = String::new();
        self.path = None;
        cx.notify();
    }

    fn open_document(&mut self, _: &OpenDocument, window: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = None;
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
            Some(path) => self.write(path, cx),
            None => self.save_document_as(&SaveDocumentAs, window, cx),
        }
    }

    fn save_document_as(&mut self, _: &SaveDocumentAs, _: &mut Window, cx: &mut Context<Self>) {
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
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(path))) = target.await {
                let _ = this.update(cx, |this, cx| this.write(path, cx));
            }
        })
        .detach();
    }

    fn close_menu(&mut self, _: &CloseMenu, _: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = None;
        cx.notify();
    }

    fn menu_rows(&self, kind: MenuKind) -> Vec<MenuRow> {
        match kind {
            MenuKind::File => vec![
                row("New", Some("Ctrl+N"), Box::new(NewDocument), false),
                row("Open…", Some("Ctrl+O"), Box::new(OpenDocument), false),
                row("Save", Some("Ctrl+S"), Box::new(SaveDocument), false),
                row("Save as…", Some("Ctrl+Shift+S"), Box::new(SaveDocumentAs), false),
                MenuRow {
                    separated: true,
                    ..row("Preferences…", Some("Ctrl+,"), Box::new(OpenPreferences), false)
                },
            ],
            MenuKind::Edit => vec![
                row("Review…", None, Box::new(StartReview), false),
                MenuRow {
                    separated: true,
                    ..row("Accept all suggestions", None, Box::new(AcceptAll), true)
                },
                row("Reject all suggestions", None, Box::new(RejectAll), true),
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
            .border_color(if open { roles.hairline } else { transparent_black() })
            .when(open, |this| this.bg(roles.fill))
            .hover(|this| this.bg(roles.fill))
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.open_menu = if this.open_menu == Some(kind) { None } else { Some(kind) };
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

    fn render_menu(&self, kind: MenuKind, roles: Roles, cx: &mut Context<Self>) -> impl IntoElement {
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
            .children(self.menu_rows(kind).into_iter().enumerate().map(|(index, item)| {
                let action = item.action.boxed_clone();
                div()
                    .when(item.separated, |this| {
                        this.border_t_1().border_color(roles.hairline).mt(px(4.)).pt(px(4.))
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
                            .text_color(if item.disabled { roles.faint } else { roles.ink })
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
            }))
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

fn row(label: &'static str, shortcut: Option<&'static str>, action: Box<dyn Action>, disabled: bool) -> MenuRow {
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
                    .child(format!("{}{}", self.file_name(), if dirty { " •" } else { "" })),
            )
            .children(self.notice.clone().map(|notice| {
                div().text_size(px(13.)).text_color(roles.danger).child(notice)
            }));
        let editor = div()
            .flex_1()
            .min_h_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .w(px(768.))
                    .max_w_full()
                    .h_full()
                    .px(px(24.))
                    .child(Editor::new(&self.editor).h_full().bordered(false)),
            );
        div()
            .id("workspace")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::new_document))
            .on_action(cx.listener(Self::open_document))
            .on_action(cx.listener(Self::save_document))
            .on_action(cx.listener(Self::save_document_as))
            .on_action(cx.listener(Self::close_menu))
            .size_full()
            .flex()
            .flex_col()
            .bg(roles.canvas)
            .text_color(roles.ink)
            .font_family("Inter")
            .child(title_bar)
            .child(header)
            .child(editor)
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
