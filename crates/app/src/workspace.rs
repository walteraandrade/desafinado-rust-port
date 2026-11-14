use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use desafinado_core::sample::SAMPLE_DOCUMENT;
use desafinado_features::{
    evals::unsaved::{self as arrival, restore_unsaved},
    history::{
        hub::{HubOptions, SwitchableHistory},
        line_changes::LineChange,
        memory_store::MemoryHistoryStore,
        recorder::AcceptedInput,
        session::{DocumentHistory, DocumentHistoryOptions},
        store::{HistoryStore, RecordInput},
        types::{AcceptedChange, SuggestionOrigin, VersionCause, VersionId},
        view_mode::{HistoryMode, HistoryModeAction, next_history_mode, visible_mode},
    },
};
use desafinado_platform::{
    FILE_NOT_FOUND_REASON,
    files::{
        FileReadResult, FileSaveResult, SaveRequest, read_markdown, save_markdown,
        save_markdown_as, with_markdown_extension,
    },
    history::{HistoryDb, Unsaved},
    library::LibrarySnapshot,
    watch::{ExternalChanges, ExternalChangesController, WatchConfig},
};
use desafinado_runtime::services::Services;
use futures::{StreamExt as _, channel::mpsc::UnboundedReceiver};
use gpui_kit::{component::TitleBar, prelude::FluentBuilder as _, *};

use crate::{
    document::{
        Conflict, ExternalFacts, ExternalVerdict, FileSession, SCRATCH_KEY, UNTITLED_NAME,
        decide_external_change, edited_session, has_unsaved_changes, history_key, minimal_change,
        name_of, opened_session, suggested_file_name,
    },
    editor_pane::{EditorPane, PaneEvent},
    history_store::DbHistoryStore,
    last_file::{clear_last_file, load_last_file, save_last_file},
    single_instance::LaunchMessage,
    theme::{self, Roles},
    ui::{self, ButtonKind},
};

const HISTORY_IDLE_MS: i64 = 30_000;
const UNSAVED_DELAY: Duration = Duration::from_millis(400);
const TOAST_DURATION: Duration = Duration::from_millis(4500);
const FLASH_DURATION: Duration = Duration::from_millis(1200);
const HISTORY_FILE: &str = "history.sqlite";

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
        CloseHistory,
        DotLeft,
        DotRight,
        DotHome,
        DotEnd,
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
        KeyBinding::new("escape", CloseHistory, Some("HistoryView")),
        KeyBinding::new("left", DotLeft, Some("HistoryView")),
        KeyBinding::new("right", DotRight, Some("HistoryView")),
        KeyBinding::new("home", DotHome, Some("HistoryView")),
        KeyBinding::new("end", DotEnd, Some("HistoryView")),
    ]);
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

fn utc_offset_minutes() -> i64 {
    i64::from(chrono::Local::now().offset().local_minus_utc()) / 60
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

pub struct Startup {
    pub launch: Option<PathBuf>,
    pub services: Arc<Services>,
    pub launches: UnboundedReceiver<LaunchMessage>,
}

#[derive(Clone, Debug)]
struct StartFile {
    name: String,
    path: PathBuf,
    saved_text: String,
    disk_changed: bool,
}

#[derive(Clone, Debug, Default)]
struct Reopened {
    text: Option<String>,
    start: Option<StartFile>,
    notice: Option<String>,
    gone: bool,
}

struct Toast {
    text: SharedString,
    _timer: Task<()>,
}

struct Opened {
    name: String,
    path: PathBuf,
    text: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arrival {
    Opened,
    Reloaded,
}

fn with_reason(text: String, reason: Option<&str>) -> String {
    match reason {
        Some(reason) => format!("{text}: {reason}"),
        None => text,
    }
}

fn load_unsaved(db: Option<&HistoryDb>, key: &str) -> Option<Unsaved> {
    db.and_then(|db| db.load_unsaved(key).ok().flatten())
}

fn reopen(db: Option<&HistoryDb>, path: &Path, failure_text: &str) -> Reopened {
    match read_markdown(path) {
        FileReadResult::Opened { path, name, text } => {
            let path = PathBuf::from(path);
            let stored =
                load_unsaved(db, &history_key(Some(&path))).map(|unsaved| arrival::Unsaved {
                    text: unsaved.text,
                    base: unsaved.base,
                });
            let arrived = restore_unsaved(&text, stored.as_ref());
            Reopened {
                text: Some(arrived.text),
                notice: arrived
                    .restored
                    .then(|| format!("Restored unsaved changes to {name}")),
                start: Some(StartFile {
                    name,
                    path,
                    saved_text: arrived.saved_text,
                    disk_changed: arrived.disk_changed,
                }),
                gone: false,
            }
        }
        FileReadResult::Cancelled => Reopened::default(),
        FileReadResult::Failed { reason } => Reopened {
            notice: Some(with_reason(
                format!("{failure_text} {}", name_of(path)),
                Some(&reason),
            )),
            gone: reason == FILE_NOT_FOUND_REASON,
            ..Reopened::default()
        },
    }
}

fn touches(path: &Path, changed: &str) -> bool {
    let path = path.to_string_lossy();
    path == changed || path.starts_with(&format!("{changed}/"))
}

pub struct Workspace {
    pane: Entity<EditorPane>,
    services: Arc<Services>,
    db: Option<Arc<HistoryDb>>,
    session: Option<FileSession>,
    conflict: Option<Conflict>,
    writing: Option<String>,
    noticed: Option<Option<String>>,
    last_text: String,
    pub(crate) history: SwitchableHistory<PathBuf>,
    history_tick: Option<Task<()>>,
    unsaved: HashMap<String, Unsaved>,
    unsaved_task: Option<Task<()>>,
    remembered: Option<PathBuf>,
    watcher: Option<ExternalChangesController>,
    watched: Option<Option<PathBuf>>,
    toast: Option<Toast>,
    pub(crate) mode: HistoryMode,
    pub(crate) hovered: Option<VersionId>,
    pub(crate) dot_hovered: Option<VersionId>,
    pub(crate) flash: Option<(VersionId, Task<()>)>,
    pub(crate) compare: Option<(VersionId, String, Option<Vec<LineChange>>)>,
    pub(crate) entry_diffs: HashMap<(VersionId, VersionId), Option<Vec<LineChange>>>,
    pub(crate) history_scroll: ScrollHandle,
    pub(crate) compare_scroll: ScrollHandle,
    pub(crate) timeline_width: std::rc::Rc<std::cell::Cell<Pixels>>,
    pub(crate) utc_offset: i64,
    open_menu: Option<MenuKind>,
    leave: Option<LeaveIntent>,
    review_focus: Option<Entity<gpui_kit::component::input::InputState>>,
    after_save: Option<LeaveIntent>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(startup: Startup, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let Startup {
            launch,
            services,
            launches,
        } = startup;
        let user_data = services.user_data.clone();
        let db = HistoryDb::open(&user_data.join(HISTORY_FILE))
            .map_err(|error| eprintln!("History could not open: {error}"))
            .ok()
            .map(Arc::new);
        let store: Arc<dyn HistoryStore> = match &db {
            Some(db) => Arc::new(DbHistoryStore::new(db.clone(), now_ms)),
            None => Arc::new(MemoryHistoryStore::new()),
        };
        let from_launch = launch
            .map(|path| reopen(db.as_deref(), &path, "Could not open"))
            .unwrap_or_default();
        let launched = from_launch.start.as_ref().map(|start| start.path.clone());
        if let Some(path) = &launched {
            save_last_file(&user_data, path);
        }
        let last_file = launched.clone().or_else(|| load_last_file(&user_data));
        let reopened = match (&launched, &last_file) {
            (Some(_), _) => from_launch.clone(),
            (None, Some(path)) => reopen(db.as_deref(), path, "Could not reopen"),
            (None, None) => Reopened::default(),
        };
        let notice = from_launch.notice.clone().or(reopened.notice.clone());
        let initial = reopened
            .text
            .clone()
            .or_else(|| load_unsaved(db.as_deref(), SCRATCH_KEY).map(|unsaved| unsaved.text))
            .unwrap_or_else(|| SAMPLE_DOCUMENT.to_string());
        if reopened.gone
            && let Some(path) = &last_file
        {
            clear_last_file(&user_data);
            let key = history_key(Some(path));
            if let Some(db) = &db
                && load_unsaved(Some(db), &key).is_none_or(|unsaved| unsaved.text == initial)
            {
                let _ = db.clear_unsaved(&key);
            }
        }
        let start = reopened.start.clone();
        let start_path = start.as_ref().map(|start| start.path.clone());
        let start_key = history_key(start_path.as_deref());
        let _ = store.record(
            &start_key,
            RecordInput {
                at: now_ms(),
                cause: VersionCause::Edited,
                text: initial.clone(),
            },
        );
        let first_initial = store.list(&start_key).unwrap_or_default();
        let history_store = store.clone();
        let history = SwitchableHistory::new(HubOptions {
            key_of: Box::new(|reference: Option<&PathBuf>| {
                history_key(reference.map(PathBuf::as_path))
            }),
            create: Box::new(move |key, initial| {
                let mut options =
                    DocumentHistoryOptions::new(history_store.clone(), key, HISTORY_IDLE_MS);
                options.initial = initial;
                options.on_error = Box::new(|error| eprintln!("History write failed: {error}"));
                DocumentHistory::new(options)
            }),
            first: start_path.clone(),
            first_initial,
        });
        let pane = cx.new(|cx| {
            EditorPane::new(
                desafinado_runtime::ai_backend::backend(services.clone()),
                window,
                cx,
            )
        });
        pane.update(cx, |pane, cx| pane.set_text(&initial, window, cx));
        let events = cx.subscribe(&pane, |this, _, event: &PaneEvent, cx| {
            this.on_pane_event(event, cx)
        });
        let release = cx.on_release(|this, _| this.flush_all());
        let quit = cx.on_app_quit(|this, _| {
            this.flush_all();
            async {}
        });
        let session = start.as_ref().map(|start| FileSession {
            name: start.name.clone(),
            path: Some(start.path.clone()),
            saved_text: start.saved_text.clone(),
            dirty: initial != start.saved_text,
        });
        let conflict = start
            .as_ref()
            .filter(|start| start.disk_changed)
            .map(|start| Conflict {
                name: start.name.clone(),
                path: start.path.clone(),
            });
        let remembered = if reopened.gone {
            None
        } else {
            start_path.clone().or(last_file)
        };
        let mut workspace = Self {
            pane,
            services,
            db,
            session,
            conflict,
            writing: None,
            noticed: None,
            last_text: initial,
            history,
            history_tick: None,
            unsaved: HashMap::new(),
            unsaved_task: None,
            remembered,
            watcher: None,
            watched: None,
            toast: None,
            mode: HistoryMode::Editing,
            hovered: None,
            dot_hovered: None,
            flash: None,
            compare: None,
            entry_diffs: HashMap::new(),
            history_scroll: ScrollHandle::new(),
            compare_scroll: ScrollHandle::new(),
            timeline_width: std::rc::Rc::new(std::cell::Cell::new(px(560.))),
            utc_offset: utc_offset_minutes(),
            open_menu: None,
            leave: None,
            review_focus: None,
            after_save: None,
            focus: cx.focus_handle(),
            _subscriptions: vec![events, release, quit],
        };
        workspace.start_watcher(window, cx);
        workspace.listen_for_launches(launches, window, cx);
        if let Some(notice) = notice {
            workspace.notify(notice, cx);
        }
        workspace
    }

    fn start_watcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (sender, mut receiver) = futures::channel::mpsc::unbounded::<ExternalChanges>();
        let controller = ExternalChangesController::start(WatchConfig::new(
            || LibrarySnapshot {
                roots: Vec::new(),
                problem: None,
            },
            move |changes| {
                let _ = sender.unbounded_send(changes);
            },
        ));
        self.watcher = Some(controller);
        self.watch();
        cx.spawn_in(window, async move |this, cx| {
            while let Some(changes) = receiver.next().await {
                if this
                    .update_in(cx, |this, window, cx| {
                        this.external_changes(changes, window, cx)
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn listen_for_launches(
        &mut self,
        mut launches: UnboundedReceiver<LaunchMessage>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            while let Some(message) = launches.next().await {
                if this
                    .update_in(cx, |this, window, cx| this.launched(message, window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn launched(&mut self, message: LaunchMessage, window: &mut Window, cx: &mut Context<Self>) {
        window.activate_window();
        let Some(path) = desafinado_platform::launch::launch_file_of(&message.argv, &message.cwd)
        else {
            return;
        };
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        if self.session_path() == Some(path.as_path()) {
            return;
        }
        self.guard(LeaveIntent::OpenPath(path), window, cx);
    }

    fn session_path(&self) -> Option<&Path> {
        self.session
            .as_ref()
            .and_then(|session| session.path.as_deref())
    }

    fn watch(&mut self) {
        let path = self.session_path().map(Path::to_path_buf);
        if self.watched.as_ref() == Some(&path) {
            return;
        }
        if let Some(watcher) = &self.watcher {
            watcher.watch_file(path.as_deref().and_then(Path::to_str));
        }
        self.watched = Some(path);
    }

    pub(crate) fn text(&self, cx: &App) -> String {
        self.pane.read(cx).text(cx)
    }

    fn unsaved_changes(&self, cx: &App) -> bool {
        has_unsaved_changes(self.session.as_ref(), &self.text(cx), SAMPLE_DOCUMENT)
    }

    fn notify(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        let timer = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TOAST_DURATION).await;
            let _ = this.update(cx, |this, cx| {
                this.toast = None;
                cx.notify();
            });
        });
        self.toast = Some(Toast {
            text: text.into(),
            _timer: timer,
        });
        cx.notify();
    }

    fn on_pane_event(&mut self, event: &PaneEvent, cx: &mut Context<Self>) {
        match event {
            PaneEvent::Changed => {
                let text = self.text(cx);
                if text != self.last_text {
                    self.last_text = text.clone();
                    self.text_changed(&text, cx);
                    self.history.edited(text, now_ms());
                    self.schedule_history(cx);
                }
            }
            PaneEvent::Accepted {
                before,
                text,
                changes,
            } => {
                self.last_text = self.text(cx);
                let current = self.last_text.clone();
                self.text_changed(&current, cx);
                self.history.accepted(
                    AcceptedInput {
                        before: before.clone(),
                        text: text.clone(),
                        changes: changes
                            .iter()
                            .map(|change| AcceptedChange {
                                origin: match change.origin {
                                    desafinado_core::suggestions::SuggestionOrigin::Ask => {
                                        SuggestionOrigin::Ask
                                    }
                                    desafinado_core::suggestions::SuggestionOrigin::Review => {
                                        SuggestionOrigin::Review
                                    }
                                },
                                instruction: change.instruction.clone(),
                                rationale: change.rationale.clone(),
                            })
                            .collect(),
                    },
                    now_ms(),
                );
                self.settle_history(cx);
            }
        }
        cx.notify();
    }

    fn text_changed(&mut self, text: &str, cx: &mut Context<Self>) {
        self.session = edited_session(self.session.as_ref(), text);
        self.announce(text, cx);
    }

    fn announce(&mut self, text: &str, cx: &mut Context<Self>) {
        self.unsaved.insert(
            SCRATCH_KEY.to_string(),
            Unsaved {
                text: text.to_string(),
                base: String::new(),
            },
        );
        if let Some(session) = &self.session
            && let Some(path) = &session.path
        {
            self.unsaved.insert(
                history_key(Some(path)),
                Unsaved {
                    text: text.to_string(),
                    base: session.saved_text.clone(),
                },
            );
            if self.remembered.as_ref() != Some(path) {
                self.remembered = Some(path.clone());
                save_last_file(&self.services.user_data, path);
            }
        }
        self.unsaved_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(UNSAVED_DELAY).await;
            let _ = this.update(cx, |this, _| this.flush_unsaved());
        }));
    }

    fn flush_unsaved(&mut self) {
        self.unsaved_task = None;
        let pending = std::mem::take(&mut self.unsaved);
        let Some(db) = &self.db else {
            return;
        };
        for (key, unsaved) in pending {
            let result = if unsaved.text == unsaved.base {
                db.clear_unsaved(&key)
            } else {
                db.save_unsaved(&key, &unsaved, now_ms())
            };
            if let Err(error) = result {
                eprintln!("Unsaved text write failed: {error}");
            }
        }
    }

    fn discard_unsaved(&mut self, path: &Path) {
        let key = history_key(Some(path));
        self.unsaved.remove(&key);
        if let Some(db) = &self.db {
            let _ = db.clear_unsaved(&key);
        }
    }

    fn offered(&mut self, path: &Path) -> Option<arrival::Unsaved> {
        self.flush_unsaved();
        load_unsaved(self.db.as_deref(), &history_key(Some(path))).map(|unsaved| arrival::Unsaved {
            text: unsaved.text,
            base: unsaved.base,
        })
    }

    fn flush_all(&mut self) {
        self.flush_unsaved();
        self.history.flush(now_ms());
        self.history.settled();
    }

    fn settle_history(&mut self, cx: &mut Context<Self>) {
        self.history.settled();
        self.schedule_history(cx);
        cx.notify();
    }

    fn schedule_history(&mut self, cx: &mut Context<Self>) {
        let Some(due) = self.history.next_due() else {
            self.history_tick = None;
            return;
        };
        let wait = Duration::from_millis((due - now_ms()).max(0) as u64);
        self.history_tick = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update(cx, |this, cx| {
                this.history.poll(now_ms());
                this.settle_history(cx);
            });
        }));
    }

    fn settle_before(&mut self, next: &str, cx: &App) {
        let before = self.text(cx);
        if before != next {
            self.history.replaced(before, now_ms());
        }
    }

    fn arrived(&mut self) {
        self.conflict = None;
        self.noticed = None;
        let path = self.session_path().map(Path::to_path_buf);
        self.history.select(path.as_ref(), now_ms());
    }

    fn replaced(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_history(window, cx);
        self.compare = None;
        self.entry_diffs.clear();
    }

    fn arrive(
        &mut self,
        opened: Opened,
        keep_unsaved: bool,
        kind: Arrival,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Opened { name, path, text } = opened;
        let stored = if keep_unsaved {
            self.offered(&path)
        } else {
            None
        };
        let arrived = restore_unsaved(&text, stored.as_ref());
        self.settle_before(&arrived.text, cx);
        self.session = Some(opened_session(
            &name,
            &path,
            &arrived.text,
            &arrived.saved_text,
        ));
        self.last_text = arrived.text.clone();
        self.pane
            .update(cx, |pane, cx| pane.replace_text(&arrived.text, window, cx));
        self.arrived();
        match kind {
            Arrival::Opened => self
                .history
                .opened(arrived.text.clone(), name.clone(), now_ms()),
            Arrival::Reloaded => self.history.external(arrived.text.clone(), now_ms()),
        }
        self.settle_history(cx);
        self.replaced(window, cx);
        self.announce(&arrived.text, cx);
        let done = match kind {
            Arrival::Opened => "Opened",
            Arrival::Reloaded => "Reloaded",
        };
        self.notify(
            if arrived.restored {
                format!("Restored unsaved changes to {name}")
            } else {
                format!("{done} {name}")
            },
            cx,
        );
        if arrived.disk_changed {
            self.conflict = Some(Conflict { name, path });
        }
        self.watch();
        cx.notify();
    }

    fn load(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match read_markdown(&path) {
            FileReadResult::Opened { path, name, text } => self.arrive(
                Opened {
                    name,
                    path: PathBuf::from(path),
                    text,
                },
                true,
                Arrival::Opened,
                window,
                cx,
            ),
            FileReadResult::Cancelled => {}
            FileReadResult::Failed { reason } => self.notify(
                with_reason("Could not open the file".into(), Some(&reason)),
                cx,
            ),
        }
    }

    fn reload(&mut self, conflict: Conflict, window: &mut Window, cx: &mut Context<Self>) {
        match read_markdown(&conflict.path) {
            FileReadResult::Opened { path, name, text } => self.arrive(
                Opened {
                    name,
                    path: PathBuf::from(path),
                    text,
                },
                false,
                Arrival::Reloaded,
                window,
                cx,
            ),
            FileReadResult::Cancelled => {}
            FileReadResult::Failed { reason } => self.notify(
                with_reason(format!("Could not reload {}", conflict.name), Some(&reason)),
                cx,
            ),
        }
    }

    fn reload_from(&mut self, opened: Opened, window: &mut Window, cx: &mut Context<Self>) {
        let Opened { name, path, text } = opened;
        self.settle_before(&text, cx);
        self.session = Some(opened_session(&name, &path, &text, &text));
        let change = minimal_change(&self.text(cx), &text);
        self.last_text = text.clone();
        self.pane
            .update(cx, |pane, cx| pane.refresh_text(&change, window, cx));
        self.arrived();
        self.history.external(text.clone(), now_ms());
        self.settle_history(cx);
        self.replaced(window, cx);
        self.announce(&text, cx);
        self.notify(format!("Reloaded {name}: it changed on disk"), cx);
        self.watch();
    }

    fn external_changes(
        &mut self,
        changes: ExternalChanges,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.session_path().map(Path::to_path_buf) else {
            return;
        };
        let real = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !changes
            .paths
            .iter()
            .any(|changed| touches(&path, changed) || touches(&real, changed))
        {
            return;
        }
        let read = read_markdown(&path);
        let disk = match &read {
            FileReadResult::Opened { text, .. } => Some(text.clone()),
            _ => None,
        };
        let Some(session) = self.session.clone() else {
            return;
        };
        let document = self.text(cx);
        let verdict = decide_external_change(&ExternalFacts {
            disk: disk.as_deref(),
            saved_text: &session.saved_text,
            document: &document,
            writing: self.writing.as_deref(),
            noticed: self.noticed.as_ref().map(|noticed| noticed.as_deref()),
        });
        match (verdict, read) {
            (ExternalVerdict::Settled, _) => {
                self.noticed = None;
                self.conflict = None;
            }
            (ExternalVerdict::Reload, FileReadResult::Opened { name, text, .. }) => {
                self.noticed = None;
                self.reload_from(Opened { name, path, text }, window, cx);
            }
            (ExternalVerdict::Nothing, _) => {}
            _ => {
                self.noticed = Some(disk);
                self.conflict = Some(Conflict {
                    name: session.name,
                    path,
                });
            }
        }
        cx.notify();
    }

    fn resolve_conflict(&mut self, reload: bool, window: &mut Window, cx: &mut Context<Self>) {
        if reload {
            if let Some(conflict) = self.conflict.clone() {
                self.reload(conflict, window, cx);
            }
        } else {
            self.conflict = None;
        }
        cx.notify();
    }

    pub(crate) fn restore(
        &mut self,
        from: VersionId,
        at: i64,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settle_before(&text, cx);
        self.last_text = text.clone();
        self.pane
            .update(cx, |pane, cx| pane.restore_text(&text, window, cx));
        self.text_changed(&text, cx);
        self.history.restored(text, from, now_ms());
        self.settle_history(cx);
        self.replaced(window, cx);
        let clock = desafinado_features::history::time::format_clock(at, now_ms(), self.utc_offset);
        self.notify(format!("Restored the Version from {clock}"), cx);
    }

    fn guard(&mut self, intent: LeaveIntent, window: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = None;
        if self.unsaved_changes(cx) {
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
            LeaveChoice::Discard => {
                if let Some(path) = self.session_path().map(Path::to_path_buf) {
                    self.discard_unsaved(&path);
                }
                self.continue_with(intent, window, cx)
            }
            LeaveChoice::Save => {
                self.after_save = Some(intent);
                self.save(false, window, cx);
            }
        }
        cx.notify();
    }

    fn new_document(&mut self, _: &NewDocument, window: &mut Window, cx: &mut Context<Self>) {
        self.guard(LeaveIntent::New, window, cx);
    }

    fn clear_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settle_before("", cx);
        self.session = None;
        self.last_text = String::new();
        self.pane
            .update(cx, |pane, cx| pane.replace_text("", window, cx));
        self.arrived();
        self.history.opened("", UNTITLED_NAME, now_ms());
        self.settle_history(cx);
        self.replaced(window, cx);
        self.announce("", cx);
        self.notify("New Document", cx);
        self.watch();
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
        self.save(false, window, cx);
    }

    fn save_document_as(
        &mut self,
        _: &SaveDocumentAs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save(true, window, cx);
    }

    fn save(&mut self, save_as: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = None;
        let text = self.text(cx);
        self.writing = Some(text.clone());
        self.history.saving(text.clone(), now_ms());
        self.settle_history(cx);
        let name = self
            .session
            .as_ref()
            .map(|session| session.name.clone())
            .unwrap_or_else(|| suggested_file_name(&text));
        match self.session.clone() {
            Some(FileSession {
                path: Some(path),
                saved_text,
                ..
            }) if !save_as => {
                let result = save_markdown(&SaveRequest {
                    path: path.to_string_lossy().into_owned(),
                    text: text.clone(),
                    expected: saved_text,
                });
                self.finish_save(result, text, name, window, cx);
            }
            _ => self.prompt_save_as(text, name, window, cx),
        }
    }

    fn prompt_save_as(
        &mut self,
        text: String,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let directory = self
            .session_path()
            .and_then(Path::parent)
            .map(PathBuf::from)
            .or_else(dirs_home)
            .unwrap_or_else(|| PathBuf::from("."));
        let target = cx.prompt_for_new_path(&directory, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let result = match target.await {
                Ok(Ok(Some(chosen))) => save_as_target(&chosen, &text),
                _ => FileSaveResult::Cancelled,
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish_save(result, text, name, window, cx)
            });
        })
        .detach();
    }

    fn finish_save(
        &mut self,
        result: FileSaveResult,
        text: String,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.writing = None;
        let saved = matches!(result, FileSaveResult::Saved { .. });
        match result {
            FileSaveResult::Cancelled => {}
            FileSaveResult::Failed { reason } => self.notify(
                with_reason(format!("Could not save {name}"), Some(&reason)),
                cx,
            ),
            FileSaveResult::Conflict => match self.session_path().map(Path::to_path_buf) {
                Some(path) => self.conflict = Some(Conflict { name, path }),
                None => self.notify(
                    format!("Could not save {name}: the file changed on disk"),
                    cx,
                ),
            },
            FileSaveResult::Saved { path, name } => {
                let current = self.text(cx);
                self.session = Some(FileSession {
                    name: name.clone(),
                    path: Some(PathBuf::from(path)),
                    saved_text: text.clone(),
                    dirty: current != text,
                });
                self.arrived();
                self.history.saved(text, name.clone(), now_ms());
                self.settle_history(cx);
                self.announce(&current, cx);
                self.notify(format!("Saved {name}"), cx);
                self.watch();
            }
        }
        match self.after_save.take() {
            Some(intent) if saved => self.continue_with(intent, window, cx),
            _ => {}
        }
        cx.notify();
    }

    pub(crate) fn history_ids(&self) -> Vec<VersionId> {
        self.history
            .snapshot()
            .iter()
            .map(|version| version.id)
            .collect()
    }

    pub(crate) fn visible_mode(&self) -> HistoryMode {
        visible_mode(self.mode, &self.history_ids())
    }

    pub(crate) fn apply_mode(
        &mut self,
        action: HistoryModeAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ids = self.history_ids();
        let was_editing = self.visible_mode() == HistoryMode::Editing;
        self.mode = next_history_mode(&ids, self.visible_mode(), action);
        let editing = self.visible_mode() == HistoryMode::Editing;
        if editing {
            self.hovered = None;
            if !was_editing {
                self.pane.update(cx, |pane, cx| pane.focus(window, cx));
            }
        } else {
            self.open_menu = None;
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    pub(crate) fn close_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_mode(HistoryModeAction::Close, window, cx);
    }

    pub(crate) fn on_dot(&mut self, id: VersionId, window: &mut Window, cx: &mut Context<Self>) {
        if self.visible_mode() != HistoryMode::Full {
            self.apply_mode(HistoryModeAction::Show { id }, window, cx);
            self.compare_scroll.set_offset(point(px(0.), px(0.)));
            return;
        }
        if let Some(index) = self
            .history
            .snapshot()
            .iter()
            .rev()
            .position(|version| version.id == id)
        {
            self.history_scroll.scroll_to_top_of_item(index + 1);
        }
        let flash = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FLASH_DURATION).await;
            let _ = this.update(cx, |this, cx| {
                this.flash = None;
                cx.notify();
            });
        });
        self.flash = Some((id, flash));
        cx.notify();
    }

    pub(crate) fn view_version(
        &mut self,
        id: VersionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mode = HistoryMode::Editing;
        self.apply_mode(HistoryModeAction::Show { id }, window, cx);
        self.compare_scroll.set_offset(point(px(0.), px(0.)));
    }

    pub(crate) fn toggle_full(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_mode(HistoryModeAction::ToggleFull, window, cx);
        self.history_scroll.set_offset(point(px(0.), px(0.)));
    }

    fn step_dot(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.history_ids();
        let current = match self.visible_mode() {
            HistoryMode::Version { id } => Some(id),
            _ => None,
        };
        if let Some(target) =
            desafinado_features::history::view_mode::dot_target(&ids, current, key)
            && Some(target) != current
        {
            self.on_dot(target, window, cx);
        }
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
        self.session
            .as_ref()
            .map(|session| session.name.clone())
            .unwrap_or_else(|| UNTITLED_NAME.into())
            .into()
    }

    fn render_conflict(
        &self,
        conflict: &Conflict,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        column_row(
            div()
                .id("external-change")
                .w_full()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_x(px(16.))
                .gap_y(px(8.))
                .bg(roles.raised)
                .px(px(16.))
                .py(px(8.))
                .text_size(px(14.))
                .text_color(roles.ink)
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .child(conflict.name.clone()),
                        )
                        .child("\u{a0}changed on disk. Save will not overwrite it."),
                )
                .child(
                    ui::row()
                        .gap(px(8.))
                        .child(
                            ui::button(
                                "reload-from-disk",
                                "Reload from disk",
                                ButtonKind::Default,
                                roles,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| this.resolve_conflict(true, window, cx),
                            )),
                        )
                        .child(
                            ui::button("keep-mine", "Keep mine", ButtonKind::Default, roles)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.resolve_conflict(false, window, cx)
                                })),
                        ),
                ),
        )
        .into_any_element()
    }

    fn render_toast(&self, roles: Roles) -> Option<AnyElement> {
        let toast = self.toast.as_ref()?;
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(64.))
                .flex()
                .justify_center()
                .px(px(24.))
                .child(
                    div()
                        .id("toast")
                        .border_1()
                        .border_color(roles.hairline)
                        .bg(roles.raised)
                        .px(px(16.))
                        .py(px(8.))
                        .text_size(px(14.))
                        .text_color(roles.ink)
                        .child(toast.text.clone()),
                )
                .into_any_element(),
        )
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

fn save_as_target(chosen: &Path, text: &str) -> FileSaveResult {
    let chosen_text = chosen.to_string_lossy().into_owned();
    let target = with_markdown_extension(&chosen_text);
    if target != chosen_text && std::fs::symlink_metadata(&target).is_ok() {
        return FileSaveResult::Failed {
            reason: format!("{} already exists", name_of(Path::new(&target))),
        };
    }
    save_markdown_as(Path::new(&target), text)
}

pub(crate) fn column_row(content: impl IntoElement) -> Div {
    div().w_full().flex().justify_center().child(
        div()
            .w(px(768.))
            .max_w_full()
            .px(px(24.))
            .pb(px(12.))
            .child(content),
    )
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let roles = theme::roles(cx);
        let dirty = self.session.as_ref().is_some_and(|session| session.dirty);
        let review = self.pane.read(cx).review.as_ref().map(|run| run.added);
        let open = self.pane.read(cx).open_count();
        let mode = self.visible_mode();
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
        let conflict = self
            .conflict
            .clone()
            .map(|conflict| self.render_conflict(&conflict, roles, cx));
        let timeline = self.render_timeline(mode, roles, cx);
        let version_bar = self.render_version_bar(mode, roles, cx);
        let content: AnyElement = match mode {
            HistoryMode::Editing => self.pane.clone().into_any_element(),
            HistoryMode::Version { id } => self.render_compare(id, roles, cx).into_any_element(),
            HistoryMode::Full => self.render_full_history(roles, cx).into_any_element(),
        };
        let toast = self.render_toast(roles);
        div()
            .id("workspace")
            .key_context(if mode == HistoryMode::Editing {
                "Workspace"
            } else {
                "HistoryView"
            })
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::new_document))
            .on_action(cx.listener(Self::open_document))
            .on_action(cx.listener(Self::save_document))
            .on_action(cx.listener(Self::save_document_as))
            .on_action(cx.listener(Self::close_menu))
            .on_action(cx.listener(Self::open_review))
            .on_action(cx.listener(Self::confirm_review))
            .on_action(cx.listener(Self::stop_review))
            .on_action(
                cx.listener(|this, _: &CloseHistory, window, cx| this.close_history(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &DotLeft, window, cx| this.step_dot("ArrowLeft", window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &DotRight, window, cx| {
                    this.step_dot("ArrowRight", window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &DotHome, window, cx| this.step_dot("Home", window, cx)),
            )
            .on_action(cx.listener(|this, _: &DotEnd, window, cx| this.step_dot("End", window, cx)))
            .on_action(cx.listener(|this, _: &AcceptAll, window, cx| {
                this.open_menu = None;
                this.pane.update(cx, |pane, cx| pane.accept_all(window, cx));
            }))
            .on_action(cx.listener(|this, _: &RejectAll, _, cx| {
                this.open_menu = None;
                this.pane.update(cx, |pane, cx| pane.reject_all(cx));
            }))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(roles.canvas)
            .text_color(roles.ink)
            .font_family("Inter")
            .child(title_bar)
            .child(header)
            .child(
                div()
                    .w_full()
                    .pt(px(12.))
                    .flex()
                    .flex_col()
                    .children(conflict)
                    .child(timeline)
                    .children(version_bar),
            )
            .child(div().flex_1().min_h_0().child(content))
            .children(toast)
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
