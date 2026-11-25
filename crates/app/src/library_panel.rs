use desafinado_core::log_err::LogErr;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};

use desafinado_features::skills::{
    library::{
        AddRootResult, CreateSkillRequest, CreateSkillResult, LibraryRoot, RemoveRootResult,
    },
    library_search::{MatchRange, SearchKind, SearchResult, search_library},
    library_tree::{TreeItem, TreeKind, TreeNode, TreeView, derive_tree_view, items_change},
    library_view::skill_count,
    memo::Memo,
    search_keys::{SearchKeyAction, SearchKeyEvent, SearchKeyState, search_key},
    tree_keys::{TreeAction, TreeKeyEvent, tree_key},
};
use desafinado_platform::{
    files::DevDialogs,
    library::{
        LibraryDeps, LibrarySnapshot, RealFileSystem, RootsLoad, RootsStore, add_library_root,
        create_skill, list_library, remove_library_root,
    },
};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::{
    component::input::{Input, InputEvent, InputState},
    prelude::FluentBuilder as _,
    *,
};

use crate::{
    theme::{self, Roles},
    ui::{self, ButtonKind, ButtonSize, ToastTone, Type as _},
};

pub const RAIL_WIDTH: f32 = 40.;
pub const LIBRARY_WIDTH: f32 = 288.;
const TEXT_COLUMN: f32 = 768.;
const TYPE_AHEAD: Duration = Duration::from_millis(700);
const INDENT: f32 = 12.;
const TREE_END_PADDING: f32 = 24.;
const LIST_OVERDRAW: f32 = 200.;
const ROW_HEIGHT_HINT: f32 = 35.;
const LIST_WHEEL_LINE: f32 = 20.;

actions!(library, [SearchUp, SearchDown, SearchEnter, SearchEscape]);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", SearchUp, Some("LibrarySearch > Input")),
        KeyBinding::new("down", SearchDown, Some("LibrarySearch > Input")),
        KeyBinding::new("enter", SearchEnter, Some("LibrarySearch > Input")),
        KeyBinding::new("escape", SearchEscape, Some("LibrarySearch > Input")),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Column,
    Overlay,
}

pub fn placement(viewport_width: Pixels) -> Placement {
    if viewport_width >= px(RAIL_WIDTH + LIBRARY_WIDTH + TEXT_COLUMN) {
        Placement::Column
    } else {
        Placement::Overlay
    }
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/".into())
}

fn with_deps<T>(user_data: &Path, run: impl FnOnce(&LibraryDeps) -> T) -> T {
    let home = home();
    let store = RootsStore::new(user_data);
    run(&LibraryDeps {
        home: &home,
        store: &store,
        file_system: &RealFileSystem,
    })
}

pub fn list(user_data: &Path) -> LibrarySnapshot {
    with_deps(user_data, list_library)
}

pub fn create(user_data: &Path, request: &CreateSkillRequest) -> CreateSkillResult {
    with_deps(user_data, |deps| {
        create_skill(deps, &request.root, &request.name)
    })
}

pub enum LibraryState {
    Loading,
    Ready(LibrarySnapshot),
    Failed,
}

pub enum LibraryEvent {
    Open { path: String, focus_text: bool },
    OpenResult { path: String },
    NewSkill,
    Escape,
    Notify(String, ToastTone),
    RootsChanged,
    SearchLeft,
}

impl EventEmitter<LibraryEvent> for LibraryPanel {}

pub struct LibraryPanel {
    user_data: PathBuf,
    pub state: LibraryState,
    load: u64,
    choices: HashMap<String, bool>,
    focused_id: Option<String>,
    keyboard_hot: bool,
    typed: String,
    typed_at: Instant,
    active_path: Option<String>,
    parents: HashMap<String, Option<String>>,
    parents_of: Option<Rc<TreeView>>,
    pub focus: FocusHandle,
    tree_focus: FocusHandle,
    tree_list: ListState,
    list_of: Option<Rc<TreeView>>,
    note_row: Option<usize>,
    hinted_width: Pixels,
    reveal: Option<String>,
    row_context: Option<Rc<RowContext>>,
    search: Entity<InputState>,
    search_text: String,
    snapshot_rev: u64,
    choices_rev: u64,
    view_memo: Memo<(u64, u64, Option<String>), Rc<TreeView>>,
    results_memo: Memo<(u64, String), Rc<Vec<SearchResult>>>,
    search_active: usize,
    search_focused: bool,
    _subscriptions: Vec<Subscription>,
}

fn dom_key(keystroke: &Keystroke) -> Option<String> {
    let named = match keystroke.key.as_str() {
        "down" => "ArrowDown",
        "up" => "ArrowUp",
        "left" => "ArrowLeft",
        "right" => "ArrowRight",
        "home" => "Home",
        "end" => "End",
        "enter" => "Enter",
        "delete" => "Delete",
        "escape" => "Escape",
        "space" => " ",
        other => {
            return keystroke
                .key_char
                .clone()
                .or_else(|| (other.chars().count() == 1).then(|| other.to_string()));
        }
    };
    Some(named.to_string())
}

impl LibraryPanel {
    pub fn search_focused(&self, window: &Window, cx: &App) -> bool {
        self.search.read(cx).focus_handle(cx).is_focused(window)
    }

    pub fn new(user_data: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search · Ctrl+P"));
        let search_events =
            cx.subscribe_in(&search, window, |this, state, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.search_text = state.read(cx).value().to_string();
                    this.search_active = 0;
                    cx.notify();
                }
            });
        let search_handle = search.read(cx).focus_handle(cx);
        let search_blur = cx.on_blur(&search_handle, window, |_, _, cx| {
            cx.emit(LibraryEvent::SearchLeft);
            cx.notify();
        });
        let search_focus = cx.on_focus(&search_handle, window, |_, _, cx| cx.notify());
        let mut panel = Self {
            user_data,
            state: LibraryState::Loading,
            load: 0,
            choices: HashMap::new(),
            focused_id: None,
            keyboard_hot: false,
            typed: String::new(),
            typed_at: Instant::now(),
            active_path: None,
            parents: HashMap::new(),
            parents_of: None,
            focus: cx.focus_handle(),
            tree_focus: cx.focus_handle(),
            tree_list: ListState::new(1, ListAlignment::Top, px(LIST_OVERDRAW)),
            list_of: None,
            note_row: None,
            hinted_width: px(0.),
            reveal: None,
            row_context: None,
            search,
            search_text: String::new(),
            snapshot_rev: 0,
            choices_rev: 0,
            view_memo: Memo::new(),
            results_memo: Memo::new(),
            search_active: 0,
            search_focused: false,
            _subscriptions: vec![search_events, search_blur, search_focus],
        };
        panel.refresh(cx);
        panel
    }

    pub fn roots(&self) -> &[LibraryRoot] {
        match &self.state {
            LibraryState::Ready(snapshot) => &snapshot.roots,
            _ => &[],
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.state, LibraryState::Ready(_))
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.load += 1;
        let load = self.load;
        let user_data = self.user_data.clone();
        let task =
            cx.background_spawn(async move { std::panic::catch_unwind(|| list(&user_data)).ok() });
        cx.spawn(async move |this, cx| {
            let listed = task.await;
            this.update(cx, |this, cx| {
                if load != this.load {
                    return;
                }
                this.snapshot_rev += 1;
                this.state = match listed {
                    Some(snapshot) => LibraryState::Ready(snapshot),
                    None => LibraryState::Failed,
                };
                cx.notify();
            })
            .log_warn();
        })
        .detach();
    }

    pub fn set_active(&mut self, path: Option<String>, cx: &mut Context<Self>) {
        if self.active_path != path {
            self.active_path = path;
            self.focused_id = None;
            cx.notify();
        }
    }

    pub fn holds_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus.contains_focused(window, cx)
    }

    pub fn focus_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = self.view();
        let items = &view.items;
        let active = view.active_id.clone();
        let has = |id: &Option<String>| {
            id.as_ref()
                .is_some_and(|id| items.iter().any(|item| &item.node.id == id))
        };
        let target = if has(&self.focused_id) {
            self.focused_id.clone()
        } else if has(&active) {
            active
        } else {
            items.first().map(|item| item.node.id.clone())
        };
        self.focused_id = target;
        self.keyboard_hot = true;
        window.focus(&self.tree_focus, cx);
        self.reveal.clone_from(&self.focused_id);
        cx.notify();
    }

    pub fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |state, cx| {
            state.focus(window, cx);
            state.select_all(window, cx);
        });
        cx.notify();
    }

    fn view(&mut self) -> Rc<TreeView> {
        let key = (
            self.snapshot_rev,
            self.choices_rev,
            self.active_path.clone(),
        );
        let roots: &[LibraryRoot] = match &self.state {
            LibraryState::Ready(snapshot) => &snapshot.roots,
            _ => &[],
        };
        let choices = &self.choices;
        Rc::clone(self.view_memo.get_or_compute(key, |(_, _, active)| {
            Rc::new(derive_tree_view(roots, choices, active.as_deref()))
        }))
    }

    fn set_choice(&mut self, id: String, expand: bool) {
        self.choices.insert(id, expand);
        self.choices_rev += 1;
    }

    fn settle_focus(&mut self, view: &Rc<TreeView>) {
        let items = &view.items;
        if let Some(focused) = self.focused_id.clone()
            && !items.iter().any(|item| item.node.id == focused)
        {
            let mut next = self.parents.get(&focused).cloned().flatten();
            while let Some(id) = next.clone() {
                if items.iter().any(|item| item.node.id == id) {
                    break;
                }
                next = self.parents.get(&id).cloned().flatten();
            }
            self.focused_id = next.or_else(|| items.first().map(|item| item.node.id.clone()));
        }
        if self
            .parents_of
            .as_ref()
            .is_some_and(|held| Rc::ptr_eq(held, view))
        {
            return;
        }
        self.parents = items
            .iter()
            .map(|item| (item.node.id.clone(), item.parent_id.clone()))
            .collect();
        self.parents_of = Some(Rc::clone(view));
    }

    fn run(
        &mut self,
        action: TreeAction,
        items: &[TreeItem],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let node_of = |id: &str| {
            items
                .iter()
                .find(|item| item.node.id == id)
                .map(|item| item.node.clone())
        };
        match action {
            TreeAction::None => {}
            TreeAction::Focus { id } => {
                self.reveal = Some(id.clone());
                self.focused_id = Some(id);
            }
            TreeAction::Toggle { id, expand } => {
                self.set_choice(id, expand);
            }
            TreeAction::Open { id, focus_text } => {
                if let Some(node) = node_of(&id) {
                    self.open_node(&node, focus_text, cx);
                }
            }
            TreeAction::Remove { id } => {
                if let Some(node) = node_of(&id) {
                    self.remove_root(&node, window, cx);
                }
            }
        }
        cx.notify();
    }

    fn open_node(&mut self, node: &TreeNode, focus_text: bool, cx: &mut Context<Self>) {
        if node.openable != Some(true) {
            return;
        }
        if let Some(path) = node.path.clone() {
            self.focused_id = Some(node.id.clone());
            cx.emit(LibraryEvent::Open { path, focus_text });
        }
    }

    fn on_tree_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let Some(key) = dom_key(keystroke) else {
            return;
        };
        if key == "Escape" {
            cx.emit(LibraryEvent::Escape);
            cx.stop_propagation();
            return;
        }
        if self.typed_at.elapsed() > TYPE_AHEAD {
            self.typed.clear();
        }
        let view = self.view();
        let items = &view.items;
        let result = tree_key(
            items,
            self.focused_id.as_deref(),
            &TreeKeyEvent {
                key: &key,
                ctrl_key: keystroke.modifiers.control,
                meta_key: keystroke.modifiers.platform,
                alt_key: keystroke.modifiers.alt,
            },
            &self.typed,
        );
        self.typed = result.typed;
        self.typed_at = Instant::now();
        if result.handled {
            self.keyboard_hot = true;
            cx.stop_propagation();
        }
        self.run(result.action, items, window, cx);
    }

    fn click_row(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let view = self.view();
        let Some(item) = view.items.iter().find(|item| item.node.id == id) else {
            return;
        };
        self.focused_id = Some(item.node.id.clone());
        self.keyboard_hot = false;
        window.focus(&self.tree_focus, cx);
        if item.node.kind == TreeKind::File {
            self.open_node(&item.node, true, cx);
        } else if item.expandable {
            self.set_choice(item.node.id.clone(), !item.expanded);
        }
        cx.notify();
    }

    pub fn add_root(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if RootsStore::new(&self.user_data).load() == RootsLoad::Unreadable {
            let result = with_deps(&self.user_data, |deps| add_library_root(deps, &|| None));
            self.added(result, cx);
            return;
        }
        if let Some(dev) = DevDialogs::from_env(&|name| std::env::var(name).ok()) {
            use desafinado_platform::files::FileDialogs as _;
            let picked = dev.pick_folder();
            self.add_picked(picked, cx);
            return;
        }
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let picked = match paths.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            this.update(cx, |this, cx| this.add_picked(picked, cx))
                .log_warn();
        })
        .detach();
    }

    fn add_picked(&mut self, picked: Option<PathBuf>, cx: &mut Context<Self>) {
        let picked = picked.map(|path| path.to_string_lossy().into_owned());
        let result = with_deps(&self.user_data, |deps| {
            add_library_root(deps, &|| picked.clone())
        });
        self.added(result, cx);
    }

    fn added(&mut self, result: AddRootResult, cx: &mut Context<Self>) {
        let (text, tone) = match result {
            AddRootResult::Cancelled => return,
            AddRootResult::Failed { reason } => (
                format!("Could not add the folder: {reason}"),
                ToastTone::Danger,
            ),
            AddRootResult::Added { label } => (format!("Added {label}"), ToastTone::Success),
            AddRootResult::Duplicate { label } => (
                format!("{label} is already in the Library"),
                ToastTone::Neutral,
            ),
        };
        cx.emit(LibraryEvent::Notify(text, tone));
        cx.emit(LibraryEvent::RootsChanged);
        self.refresh(cx);
    }

    fn remove_root(&mut self, node: &TreeNode, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = node.path.clone() else {
            return;
        };
        let view = self.view();
        let tree = &view.tree;
        let at = tree.iter().position(|root| root.id == node.id);
        let neighbor = at.and_then(|at| {
            at.checked_sub(1)
                .and_then(|before| tree.get(before))
                .or_else(|| tree.get(at + 1))
                .map(|root| root.id.clone())
        });
        let result = with_deps(&self.user_data, |deps| remove_library_root(deps, &path));
        let removed = result == RemoveRootResult::Removed;
        cx.emit(match result {
            RemoveRootResult::Removed => {
                LibraryEvent::Notify(format!("Removed {}", node.name), ToastTone::Neutral)
            }
            RemoveRootResult::Failed { reason } => LibraryEvent::Notify(
                format!("Could not remove {}: {reason}", node.name),
                ToastTone::Danger,
            ),
        });
        cx.emit(LibraryEvent::RootsChanged);
        if removed && neighbor.is_some() {
            self.focused_id = neighbor;
            window.focus(&self.tree_focus, cx);
        }
        self.refresh(cx);
    }

    fn results(&mut self) -> Rc<Vec<SearchResult>> {
        let key = (self.snapshot_rev, self.search_text.clone());
        let roots: &[LibraryRoot] = match &self.state {
            LibraryState::Ready(snapshot) => &snapshot.roots,
            _ => &[],
        };
        Rc::clone(
            self.results_memo
                .get_or_compute(key, |(_, text)| Rc::new(search_library(roots, text))),
        )
    }

    fn search_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let results = self.results();
        let current = self.search_active.min(results.len().saturating_sub(1));
        match search_key(
            &SearchKeyEvent {
                key,
                ..SearchKeyEvent::default()
            },
            &SearchKeyState {
                count: results.len(),
                active: current,
                text: &self.search_text,
            },
        ) {
            SearchKeyAction::None => {}
            SearchKeyAction::Leave => cx.emit(LibraryEvent::Escape),
            SearchKeyAction::Clear => {
                self.search
                    .update(cx, |state, cx| state.set_value("", window, cx));
                self.search_text.clear();
                self.search_active = 0;
            }
            SearchKeyAction::Move { to } => self.search_active = to,
            SearchKeyAction::Open { index } => {
                if let Some(result) = results.get(index) {
                    cx.emit(LibraryEvent::OpenResult {
                        path: result.path.clone(),
                    });
                }
            }
        }
        cx.notify();
    }

    fn render_header(&self, roles: Roles) -> impl IntoElement {
        ui::row()
            .px(px(12.))
            .pt(px(24.))
            .pb(px(12.))
            .child(ui::title("Library", roles))
            .when(self.is_ready(), |this| {
                let count = skill_count(self.roots());
                this.child(ui::meta(
                    format!(
                        "\u{a0}· {count} {}",
                        if count == 1 { "skill" } else { "skills" }
                    ),
                    roles,
                ))
            })
    }

    fn render_search(
        &self,
        results: &[SearchResult],
        window: &Window,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let searching = !self.search_text.trim().is_empty();
        let current = self.search_active.min(results.len().saturating_sub(1));
        let show_list = self.search_focused && !results.is_empty();
        let show_empty = self.search_focused && searching && results.is_empty();
        let max_height = window.viewport_size().height * 0.6;
        let list: Vec<AnyElement> = if show_list {
            results
                .iter()
                .enumerate()
                .map(|(index, result)| {
                    self.render_result(index, index == current, result, roles, cx)
                        .into_any_element()
                })
                .collect()
        } else {
            Vec::new()
        };
        div()
            .px(px(12.))
            .pb(px(12.))
            .child(
                div()
                    .key_context("LibrarySearch")
                    .on_action(cx.listener(|this, _: &SearchUp, window, cx| {
                        this.search_key("ArrowUp", window, cx);
                    }))
                    .on_action(cx.listener(|this, _: &SearchDown, window, cx| {
                        this.search_key("ArrowDown", window, cx);
                    }))
                    .on_action(cx.listener(|this, _: &SearchEnter, window, cx| {
                        this.search_key("Enter", window, cx);
                    }))
                    .on_action(cx.listener(|this, _: &SearchEscape, window, cx| {
                        this.search_key("Escape", window, cx);
                    }))
                    .child(ui::field(
                        Input::new(&self.search).appearance(false),
                        self.search_focused,
                        false,
                        roles,
                    )),
            )
            .when(show_list, |this| {
                this.child(
                    div()
                        .id("library-search-list")
                        .mt(px(4.))
                        .max_h(max_height)
                        .bg(roles.overlay)
                        .children(list)
                        .overflow_y_scrollbar(),
                )
            })
            .when(show_empty, |this| {
                this.child(
                    ui::meta(
                        format!("No Skill or file matches “{}”", self.search_text.trim()),
                        roles,
                    )
                    .mt(px(4.))
                    .px(px(8.))
                    .py(px(8.))
                    .bg(roles.overlay),
                )
            })
            .into_any_element()
    }

    fn render_result(
        &self,
        index: usize,
        selected: bool,
        result: &SearchResult,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let path = result.path.clone();
        ui::option_row(("library-search-item", index), selected, false, roles)
            .flex_col()
            .items_start()
            .px(px(8.))
            .py(px(8.))
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .on_mouse_move(cx.listener(move |this, _: &MouseMoveEvent, _, cx| {
                if this.search_active != index {
                    this.search_active = index;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.emit(LibraryEvent::OpenResult { path: path.clone() });
            }))
            .child(
                ui::row()
                    .w_full()
                    .justify_between()
                    .gap(px(8.))
                    .child(
                        div()
                            .min_w_0()
                            .child(marked(&result.label, result.label_match, roles)),
                    )
                    .when(result.kind == SearchKind::Skill, |this| {
                        this.child(ui::meta("Skill", roles).flex_none())
                    }),
            )
            .children(
                result
                    .context
                    .as_ref()
                    .map(|context| ui::meta(format!("in {context}"), roles)),
            )
            .children(result.detail.as_ref().map(|detail| {
                div()
                    .type_small()
                    .line_clamp(2)
                    .child(marked(detail, result.detail_match, roles))
            }))
    }

    fn note_row_of(&self, view: &TreeView, tree_focused: bool) -> Option<usize> {
        let id = self.focused_id.as_ref()?;
        (tree_focused && self.keyboard_hot)
            .then(|| view.items.iter().position(|item| &item.node.id == id))
            .flatten()
            .filter(|index| view.items[*index].node.kind == TreeKind::Skill)
    }

    fn sync_list(&mut self, view: &Rc<TreeView>, tree_focused: bool) -> ListSync {
        let note_row = self.note_row_of(view, tree_focused);
        let same_view = self
            .list_of
            .as_ref()
            .is_some_and(|held| Rc::ptr_eq(held, view));
        let mut sync = ListSync::default();
        if !same_view {
            let old = self.list_of.replace(Rc::clone(view));
            let old_items = old.as_ref().map_or(&[][..], |old| &old.items[..]);
            if let Some(change) = items_change(old_items, &view.items) {
                self.tree_list.splice(change.old, change.new_len);
                if old_items.is_empty() {
                    self.tree_list.scroll_to(ListOffset::default());
                }
                self.hint_rows();
                sync.structural = true;
            }
        } else if note_row != self.note_row {
            for index in [self.note_row, note_row].into_iter().flatten() {
                self.tree_list.remeasure_items(index..index + 1);
            }
            sync.remeasured = true;
        }
        self.note_row = note_row;
        let width = self.tree_list.viewport_bounds().size.width;
        if width > px(0.) && width != self.hinted_width {
            self.hint_rows();
        }
        sync
    }

    fn hint_rows(&mut self) {
        self.hinted_width = self.tree_list.viewport_bounds().size.width;
        self.tree_list
            .clone()
            .with_uniform_item_height(px(ROW_HEIGHT_HINT));
    }

    fn resolve_reveal(
        &mut self,
        view: &TreeView,
        sync: ListSync,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.reveal.take() else {
            return;
        };
        let Some(index) = view.items.iter().position(|item| item.node.id == id) else {
            return;
        };
        let laid_out = self.tree_list.viewport_bounds().size.height > px(0.);
        if sync.structural || !laid_out {
            self.tree_list.scroll_to(ListOffset {
                item_ix: index,
                offset_in_item: px(0.),
            });
        } else if sync.remeasured {
            self.reveal = Some(id);
            cx.on_next_frame(window, |_, _, cx| cx.notify());
        } else if self.tree_list.bounds_for_item(index).is_some() {
            self.tree_list.scroll_to_reveal_item(index);
        } else {
            self.tree_list.scroll_to(ListOffset {
                item_ix: index,
                offset_in_item: px(0.),
            });
        }
    }

    fn render_list_item(
        &mut self,
        index: usize,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(context) = self.row_context.clone() else {
            return div().into_any_element();
        };
        let Some(item) = context.view.items.get(index) else {
            return div().h(px(TREE_END_PADDING)).into_any_element();
        };
        let roles = theme::roles(cx);
        self.render_row(index, item, &context, roles, cx)
    }

    fn render_row(
        &self,
        index: usize,
        item: &TreeItem,
        context: &RowContext,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let node = &item.node;
        let hot = context.tree_focused && self.focused_id.as_ref() == Some(&node.id);
        let is_active =
            node.kind == TreeKind::File && context.view.active_id.as_ref() == Some(&node.id);
        let note_open = item.expanded || (hot && self.keyboard_hot);
        let row_id = node.id.clone();
        let row = ui::focusable_row(("library-row", index), hot, is_active, roles)
            .flex_col()
            .items_start()
            .gap(px(4.))
            .py(px(4.))
            .pr(px(12.))
            .pl(px((item.level as f32 - 1.) * INDENT + 12.))
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .on_click(cx.listener(move |this, _, window, cx| this.click_row(&row_id, window, cx)))
            .when_some(
                (node.kind == TreeKind::File)
                    .then(|| node.path.clone())
                    .flatten(),
                |this, path| {
                    this.tooltip(move |window, cx| {
                        component::tooltip::Tooltip::new(path.clone()).build(window, cx)
                    })
                },
            )
            .child(self.render_row_line(index, item, context, roles, cx))
            .when(node.kind == TreeKind::Skill && note_open, |this| {
                this.child(row_note(index, node, roles))
            });
        div()
            .w_full()
            .child(row)
            .children(
                node.note
                    .as_ref()
                    .map(|note| ui::meta(note.clone(), roles).px(px(12.)).py(px(4.))),
            )
            .into_any_element()
    }

    fn render_row_line(
        &self,
        index: usize,
        item: &TreeItem,
        context: &RowContext,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Div {
        let node = &item.node;
        let is_active =
            node.kind == TreeKind::File && context.view.active_id.as_ref() == Some(&node.id);
        let holds_open = node.kind == TreeKind::Skill && context.view.holding.contains(&node.id);
        let unopenable = node.kind == TreeKind::File && node.openable != Some(true);
        let marker: AnyElement = if node.kind == TreeKind::File {
            div()
                .size(px(6.))
                .when(is_active, |this| this.bg(roles.accent))
                .into_any_element()
        } else {
            div()
                .type_small()
                .text_color(if holds_open {
                    roles.accent
                } else {
                    roles.muted
                })
                .child(match (item.expandable, item.expanded) {
                    (false, _) => "",
                    (true, true) => "▾",
                    (true, false) => "▸",
                })
                .into_any_element()
        };
        let label = div().min_w_0().truncate().child(node.name.clone());
        let label = match node.kind {
            TreeKind::Root => label.type_small().weight_medium().text_color(roles.muted),
            TreeKind::Skill if node.problem.is_some() => label.text_color(roles.danger),
            TreeKind::Skill => label,
            _ if unopenable => label.type_small().text_color(roles.muted),
            _ if is_active => label.type_small().weight_medium(),
            _ => label.type_small(),
        };
        let remove_node = node.clone();
        ui::row()
            .w_full()
            .gap(px(8.))
            .child(
                div()
                    .w(px(8.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(marker),
            )
            .child(label)
            .children(
                node.detail
                    .as_ref()
                    .map(|detail| ui::meta(format!("· {detail}"), roles).flex_none()),
            )
            .when(
                node.kind == TreeKind::Root && node.removable == Some(true),
                |this| {
                    this.child(div().flex_1()).child(
                        ui::small_button(("library-remove", index), "×", ButtonKind::Quiet, roles)
                            .tooltip(|window, cx| {
                                component::tooltip::Tooltip::new(
                                    "Remove from the Library. The folder stays on disk.",
                                )
                                .build(window, cx)
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.remove_root(&remove_node, window, cx);
                            })),
                    )
                },
            )
    }

    fn render_actions(&self, roles: Roles, cx: &mut Context<Self>) -> Div {
        let ready = self.is_ready();
        let tip = |text: &'static str| {
            move |window: &mut Window, cx: &mut App| {
                component::tooltip::Tooltip::new(text).build(window, cx)
            }
        };
        ui::row()
            .px(px(12.))
            .pb(px(12.))
            .gap(px(4.))
            .child(
                ui::sized_button(
                    "library-new",
                    "New",
                    ButtonKind::Default,
                    ButtonSize::Regular,
                    ui::Control::disabled(!ready),
                    roles,
                )
                .tooltip(tip("Create a skill from a template"))
                .when(ready, |this| {
                    this.on_click(cx.listener(|_, _, _, cx| cx.emit(LibraryEvent::NewSkill)))
                }),
            )
            .child(
                ui::button("library-refresh", "Refresh", ButtonKind::Default, roles)
                    .tooltip(tip("Scan the folders again"))
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            )
            .child(
                ui::button("library-add", "Add", ButtonKind::Default, roles)
                    .tooltip(tip(
                        "Add a project folder of skills, such as .claude/skills",
                    ))
                    .on_click(cx.listener(|this, _, window, cx| this.add_root(window, cx))),
            )
    }

    fn render_status(&self, roles: Roles, cx: &mut Context<Self>) -> Div {
        let problem = match &self.state {
            LibraryState::Ready(snapshot) => snapshot.problem.clone(),
            _ => None,
        };
        div()
            .flex()
            .flex_col()
            .px(px(12.))
            .type_small()
            .when(matches!(self.state, LibraryState::Loading), |this| {
                this.child(ui::meta("Reading the skill folders…", roles).py(px(4.)))
            })
            .when(matches!(self.state, LibraryState::Failed), |this| {
                this.child(
                    ui::row()
                        .py(px(4.))
                        .text_color(roles.danger)
                        .child("Could not read the skill folders.\u{a0}")
                        .child(
                            ui::link("library-try-again", "Try again", roles)
                                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                        ),
                )
            })
            .children(problem.map(|problem| {
                ui::error_text(
                    format!(
                        "{problem}. Project folders are not listed, and Add and Remove are off until it is fixed or deleted in the app's user data folder."
                    ),
                    roles,
                )
                .py(px(4.))
            }))
    }
}

fn row_note(index: usize, node: &TreeNode, roles: Roles) -> Div {
    div()
        .pl(px(16.))
        .flex()
        .flex_col()
        .type_small()
        .children(node.description.as_ref().map(|description| {
            let full = description.clone();
            div()
                .id(("library-note", index))
                .text_color(roles.ink)
                .line_clamp(3)
                .tooltip(move |window, cx| {
                    component::tooltip::Tooltip::new(full.clone()).build(window, cx)
                })
                .child(description.clone())
        }))
        .children(
            node.problem
                .as_ref()
                .map(|problem| ui::error_text(format!("Frontmatter: {}", problem.message), roles)),
        )
}

struct RowContext {
    tree_focused: bool,
    view: Rc<TreeView>,
}

#[derive(Default, Clone, Copy)]
struct ListSync {
    structural: bool,
    remeasured: bool,
}

fn marked(text: &str, range: Option<MatchRange>, roles: Roles) -> StyledText {
    let styled = StyledText::new(text.to_string());
    match range {
        Some(range) if range.end <= text.len() && range.start < range.end => styled
            .with_highlights(vec![(
                range.start..range.end,
                HighlightStyle {
                    font_weight: Some(FontWeight::SEMIBOLD),
                    underline: Some(UnderlineStyle {
                        thickness: px(1.),
                        color: Some(roles.ink),
                        wavy: false,
                    }),
                    ..Default::default()
                },
            )]),
        _ => styled,
    }
}

impl Focusable for LibraryPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for LibraryPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        perf_span!("library.render");
        let roles = theme::roles(cx);
        self.search_focused = self.search.read(cx).focus_handle(cx).is_focused(window);
        let view = self.view();
        self.settle_focus(&view);
        let results = self.results();
        let tree_focused = self.tree_focus.is_focused(window);
        let sync = self.sync_list(&view, tree_focused);
        self.resolve_reveal(&view, sync, window, cx);
        self.row_context = Some(Rc::new(RowContext { tree_focused, view }));
        let line = f32::from(ui::body_line_height(window));
        let search = self.render_search(&results, window, roles, cx);
        div()
            .id("library-panel")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(roles.raised)
            .type_body()
            .text_color(roles.ink)
            .child(self.render_header(roles))
            .child(search)
            .child(self.render_actions(roles, cx))
            .child(self.render_status(roles, cx))
            .child(ui::scrolled_rest(
                div()
                    .id("library-tree")
                    .track_focus(&self.tree_focus)
                    .flex_1()
                    .min_h_0()
                    .pr(px(8.))
                    .on_key_down(cx.listener(Self::on_tree_key))
                    .on_scroll(cx.listener(move |this, event: &ScrollEvent, _, cx| {
                        if let ScrollDelta::Lines(lines) = event.delta {
                            let distance = if lines.y == 0. {
                                lines.x * line
                            } else {
                                lines.y * (line - LIST_WHEEL_LINE)
                            };
                            this.tree_list.scroll_by(px(-distance));
                            cx.notify();
                        }
                    }))
                    .child(
                        gpui_kit::list(
                            self.tree_list.clone(),
                            cx.processor(|this, index: usize, window, cx| {
                                this.render_list_item(index, window, cx)
                            }),
                        )
                        .size_full(),
                    ),
                &self.tree_list,
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::{Placement, dom_key, placement};
    use gpui_kit::{Keystroke, px};

    #[test]
    fn the_library_is_a_column_from_1096_pixels_and_lies_over_the_page_below() {
        assert_eq!(placement(px(1096.)), Placement::Column);
        assert_eq!(placement(px(1095.)), Placement::Overlay);
    }

    fn key(key: &str, key_char: Option<&str>) -> Option<String> {
        dom_key(&Keystroke {
            key: key.into(),
            key_char: key_char.map(String::from),
            ..Keystroke::default()
        })
    }

    #[test]
    fn gpui_keys_read_as_the_dom_keys_the_tree_expects() {
        assert_eq!(key("down", None).as_deref(), Some("ArrowDown"));
        assert_eq!(key("space", Some(" ")).as_deref(), Some(" "));
        assert_eq!(key("enter", None).as_deref(), Some("Enter"));
        assert_eq!(key("p", Some("p")).as_deref(), Some("p"));
        assert_eq!(key("f5", None), None);
    }
}
