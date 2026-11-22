use desafinado_core::log_err::LogErr;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use desafinado_core::sample::SAMPLE_DOCUMENT;
use desafinado_features::skills::{
    frontmatter_edit::{FieldEditPlan, plan_field_edit},
    frontmatter_fields::{SkillField, SkillFieldValues},
    frontmatter_panel::{
        FrontmatterContext, describe_frontmatter_problem, inspect_frontmatter, report_problems,
    },
    library::{CreateSkillRequest, CreateSkillResult},
    library_view::{open_skill_file, skill_file_label, skill_of_file},
    scan_skills::is_skill_file,
};
use desafinado_features::{
    evals::{
        ended::ended_text,
        types::EvalRunResult,
        unsaved::{
            self as arrival, UnsavedInput, files_with_unsaved_text, restore_unsaved,
            stored_is_unsaved,
        },
    },
    history::{
        hub::{HubOptions, SwitchableHistory},
        line_changes::LineChange,
        memory_store::MemoryHistoryStore,
        recorder::AcceptedInput,
        session::{DocumentHistory, DocumentHistoryOptions},
        store::{HistoryStore, RecordInput},
        types::{AcceptedChange, SuggestionOrigin, VersionCause, VersionId, VersionSummary},
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
    watch::{ExternalChanges, ExternalChangesController, WatchConfig},
};
use desafinado_runtime::services::Services;
use futures::{StreamExt as _, channel::mpsc::UnboundedReceiver};
use gpui_kit::{component::TitleBar, prelude::FluentBuilder as _, *};

use crate::{
    document::{
        Conflict, DiskState, ExternalFacts, ExternalVerdict, FileSession, SCRATCH_KEY,
        UNTITLED_NAME, decide_external_change, edited_session, has_unsaved_changes, history_key,
        minimal_change, name_of, opened_session, suggested_file_name,
    },
    editor_pane::{EditorPane, PaneEvent, PaneToastAction},
    evals_view::{self, EvalSkill, EvalsEvent, EvalsView},
    frontmatter_view::{FrontmatterEvent, FrontmatterView},
    history_store::DbHistoryStore,
    history_view::EvalsButton,
    key_dialog::{KeyDialog, KeyEvent},
    last_file::{clear_last_file, load_last_file, save_last_file},
    library_panel::{self, LibraryEvent, LibraryPanel, Placement},
    menu_nav::{next_enabled, shifted},
    model_dialog::{ModelDialog, ModelEvent},
    new_skill_dialog::{NewSkillDialog, NewSkillEvent},
    preferences_view::{PREFERENCES_WIDTH, PreferencesEvent, PreferencesPanel},
    single_instance::LaunchMessage,
    spend_view::{SpendEvent, SpendView},
    theme::{self, Roles},
    ui::{self, ButtonKind, ToastTone, Type as _},
};
use desafinado_ai::quick_action_settings::{
    QuickActionSettings, default_quick_action_settings, enabled_quick_actions,
    parse_quick_action_settings, serialize_quick_action_settings,
};
use desafinado_platform::quick_actions::{load_quick_actions, save_quick_actions};

const HISTORY_IDLE_MS: i64 = 30_000;
const UNSAVED_DELAY: Duration = Duration::from_millis(400);
const TOAST_DURATION: Duration = Duration::from_millis(4500);
const ACTION_TOAST_DURATION: Duration = Duration::from_secs(12);
const FLASH_DURATION: Duration = Duration::from_millis(1200);
const HISTORY_FILE: &str = "history.sqlite";

const LEAVE_CHOICES: [LeaveChoice; 3] =
    [LeaveChoice::Cancel, LeaveChoice::Discard, LeaveChoice::Save];

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
        ToggleLibrary,
        SearchLibrary,
        ShowFrontmatterProblems,
        ShowSpend,
        ToggleReading,
        OpenFileMenu,
        OpenEditMenu,
        MenuDown,
        MenuUp,
        MenuNext,
        MenuPrevious,
        MenuSelect,
        MenuLeave,
        LeaveNext,
        LeavePrevious,
        LeavePress,
        LeaveCancel,
    ]
);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-n", NewDocument, None),
        KeyBinding::new("ctrl-o", OpenDocument, None),
        KeyBinding::new("ctrl-s", SaveDocument, None),
        KeyBinding::new("ctrl-shift-s", SaveDocumentAs, None),
        KeyBinding::new("ctrl-,", OpenPreferences, None),
        KeyBinding::new("ctrl-b", ToggleLibrary, None),
        KeyBinding::new("ctrl-p", SearchLibrary, None),
        KeyBinding::new("ctrl-alt-r", ToggleReading, None),
        KeyBinding::new("escape", CloseMenu, Some("Menu")),
        KeyBinding::new("down", MenuDown, Some("Menu")),
        KeyBinding::new("up", MenuUp, Some("Menu")),
        KeyBinding::new("right", MenuNext, Some("Menu")),
        KeyBinding::new("left", MenuPrevious, Some("Menu")),
        KeyBinding::new("enter", MenuSelect, Some("Menu")),
        KeyBinding::new("space", MenuSelect, Some("Menu")),
        KeyBinding::new("tab", MenuLeave, Some("Menu")),
        KeyBinding::new("shift-tab", MenuLeave, Some("Menu")),
        KeyBinding::new("alt-f", OpenFileMenu, None),
        KeyBinding::new("alt-e", OpenEditMenu, None),
        KeyBinding::new("f10", OpenFileMenu, None),
        KeyBinding::new("enter", ConfirmReview, Some("ReviewPopover > Input")),
        KeyBinding::new("escape", CloseMenu, Some("ReviewPopover > Input")),
        KeyBinding::new("escape", CloseHistory, Some("HistoryView")),
        KeyBinding::new("escape", LeaveCancel, Some("LeaveDialog")),
        KeyBinding::new("tab", LeaveNext, Some("LeaveDialog")),
        KeyBinding::new("shift-tab", LeavePrevious, Some("LeaveDialog")),
        KeyBinding::new("enter", LeavePress, Some("LeaveDialog")),
        KeyBinding::new("space", LeavePress, Some("LeaveDialog")),
        KeyBinding::new("left", DotLeft, Some("HistoryView")),
        KeyBinding::new("right", DotRight, Some("HistoryView")),
        KeyBinding::new("home", DotHome, Some("HistoryView")),
        KeyBinding::new("end", DotEnd, Some("HistoryView")),
    ]);
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

pub(crate) fn utc_offset_minutes() -> i64 {
    i64::from(chrono::Local::now().offset().local_minus_utc()) / 60
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuKind {
    File,
    Edit,
}

const MENUS: [MenuKind; 2] = [MenuKind::File, MenuKind::Edit];

#[derive(Clone, Debug, PartialEq)]
enum LeaveIntent {
    New,
    Open,
    OpenPath(PathBuf),
    CreateSkill(CreateSkillRequest),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LeaveChoice {
    Save,
    Discard,
    Cancel,
}

struct MenuRow {
    label: SharedString,
    danger: bool,
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
    notice: Option<(String, ToastTone)>,
    gone: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ToastAction {
    ShowEvals { path: String },
    Pane(PaneToastAction),
}

impl ToastAction {
    fn label(&self) -> &'static str {
        match self {
            Self::ShowEvals { .. } | Self::Pane(PaneToastAction::ShowSuggestion(_)) => "Show",
            Self::Pane(PaneToastAction::RetrySuggestion(_) | PaneToastAction::RetryReview(_)) => {
                "Retry"
            }
        }
    }
}

#[derive(Default)]
struct Leaving {
    library: Option<Task<()>>,
    library_reopened: bool,
    preferences: Option<(Entity<PreferencesPanel>, Task<()>)>,
}

struct Toast {
    serial: u64,
    text: SharedString,
    tone: ToastTone,
    actions: Vec<ToastAction>,
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
            let stored =
                load_unsaved(db, &history_key(Some(&path))).map(|unsaved| arrival::Unsaved {
                    text: unsaved.text,
                    base: unsaved.base,
                });
            let arrived = restore_unsaved(&text, stored.as_ref());
            Reopened {
                text: Some(arrived.text),
                notice: arrived.restored.then(|| {
                    (
                        format!("Restored unsaved changes to {name}"),
                        ToastTone::Neutral,
                    )
                }),
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
            notice: Some((
                with_reason(format!("{failure_text} {}", name_of(path)), Some(&reason)),
                ToastTone::Danger,
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

/// What fills the window under the header while the AI spend page is closed.
/// Evals show only while a Skill is open; otherwise the text does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Text,
    Reading,
    Evals,
}

/// A file the Library asked to open, kept until the open goes through or is cancelled.
struct LibraryRequest {
    path: PathBuf,
    /// Move the focus to the text once the file is open.
    focus_text: bool,
    /// The request came from Library search, so a cancel returns there.
    from_search: bool,
}

/// The file the watcher was last told to follow.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Watching {
    File(PathBuf),
    Nothing,
}

pub struct Workspace {
    pane: Entity<EditorPane>,
    services: Arc<Services>,
    db: Option<Arc<HistoryDb>>,
    session: Option<FileSession>,
    conflict: Option<Conflict>,
    writing: Option<String>,
    save_again: bool,
    noticed: Option<DiskState<String>>,
    last_text: String,
    pub(crate) history: SwitchableHistory<PathBuf>,
    history_tick: Option<Task<()>>,
    unsaved: HashMap<String, Unsaved>,
    unsaved_task: Option<Task<()>>,
    remembered: Option<PathBuf>,
    watcher: Option<ExternalChangesController>,
    watched: Option<Watching>,
    toast: Option<Toast>,
    toast_serial: u64,
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
    leave_focus: FocusHandle,
    leave_index: usize,
    library: Entity<LibraryPanel>,
    library_open: bool,
    leaving: Leaving,
    search_unfolded: bool,
    search_opening: bool,
    library_request: Option<LibraryRequest>,
    viewport_width: Pixels,
    new_skill: Option<Entity<NewSkillDialog>>,
    frontmatter: Entity<FrontmatterView>,
    frontmatter_due: bool,
    frontmatter_scrolled: std::rc::Rc<std::cell::Cell<Pixels>>,
    frontmatter_height: std::rc::Rc<std::cell::Cell<Pixels>>,
    evals: Entity<EvalsView>,
    page: Page,
    unsaved_checked: Option<(String, bool)>,
    menu_active: Option<usize>,
    menu_focus: FocusHandle,
    menu_return: Option<FocusHandle>,
    quick_actions: QuickActionSettings,
    quick_actions_saving: Option<Task<()>>,
    key_present: bool,
    preferences: Option<Entity<PreferencesPanel>>,
    key_dialog: Option<Entity<KeyDialog>>,
    model_dialog: Option<Entity<ModelDialog>>,
    spend: Option<Entity<SpendView>>,
    _dialog_subscriptions: Vec<Subscription>,
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
        let Start {
            db,
            store,
            initial,
            notice,
            file: start,
            file_path: start_path,
            remembered,
            first_initial,
        } = open_start(launch, &user_data);
        let evals_service = evals_view::service(user_data.clone(), Arc::clone(&store));
        let history = document_history(store, start_path, first_initial);
        let (pane, quick_actions) = Self::open_pane(&services, &initial, window, cx);
        let key_present = services.keys.status().present;
        let library = cx.new(|cx| LibraryPanel::new(user_data.clone(), window, cx));
        let frontmatter = cx.new(|cx| {
            FrontmatterView::new(
                desafinado_runtime::ai_backend::backend(Arc::clone(&services)),
                window,
                cx,
            )
        });
        let utc_offset = utc_offset_minutes();
        let evals = cx.new(|_| EvalsView::new(evals_service, utc_offset));
        let subscriptions = Self::subscribe(&pane, &library, &frontmatter, &evals, window, cx);
        let (session, conflict) = session_of(start.as_ref(), &initial);
        let mut workspace = Self {
            pane,
            services,
            db,
            session,
            conflict,
            writing: None,
            save_again: false,
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
            toast_serial: 0,
            mode: HistoryMode::Editing,
            hovered: None,
            dot_hovered: None,
            flash: None,
            compare: None,
            entry_diffs: HashMap::new(),
            history_scroll: ScrollHandle::new(),
            compare_scroll: ScrollHandle::new(),
            timeline_width: std::rc::Rc::new(std::cell::Cell::new(px(560.))),
            utc_offset,
            open_menu: None,
            leave: None,
            review_focus: None,
            after_save: None,
            focus: cx.focus_handle(),
            leave_focus: cx.focus_handle(),
            leave_index: 0,
            library,
            library_open: false,
            leaving: Leaving::default(),
            search_unfolded: false,
            search_opening: false,
            library_request: None,
            viewport_width: px(1280.),
            new_skill: None,
            frontmatter,
            frontmatter_due: true,
            frontmatter_scrolled: std::rc::Rc::new(std::cell::Cell::new(px(0.))),
            frontmatter_height: std::rc::Rc::new(std::cell::Cell::new(px(0.))),
            evals,
            page: Page::Text,
            unsaved_checked: None,
            menu_active: None,
            menu_focus: cx.focus_handle(),
            menu_return: None,
            quick_actions,
            quick_actions_saving: None,
            key_present,
            preferences: None,
            key_dialog: None,
            model_dialog: None,
            spend: None,
            _dialog_subscriptions: Vec::new(),
            _subscriptions: subscriptions,
        };
        workspace.started(launches, notice, window, cx);
        workspace
    }

    fn started(
        &mut self,
        launches: UnboundedReceiver<LaunchMessage>,
        notice: Option<(String, ToastTone)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_watcher(window, cx);
        self.sync_library(cx);
        self.listen_for_launches(launches, window, cx);
        if let Some((text, tone)) = notice {
            self.notify_with(text, tone, Vec::new(), cx);
        }
    }

    fn open_pane(
        services: &Arc<Services>,
        initial: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<EditorPane>, QuickActionSettings) {
        let pane = cx.new(|cx| {
            EditorPane::new(
                desafinado_runtime::ai_backend::backend(Arc::clone(services)),
                window,
                cx,
            )
        });
        let quick_actions = load_quick_actions(&services.user_data)
            .ok()
            .flatten()
            .and_then(|text| parse_quick_action_settings(&text))
            .unwrap_or_else(default_quick_action_settings);
        pane.update(cx, |pane, cx| {
            pane.quick_actions = enabled_quick_actions(&quick_actions, true);
            pane.set_text(initial, window, cx);
        });
        (pane, quick_actions)
    }

    fn subscribe(
        pane: &Entity<EditorPane>,
        library: &Entity<LibraryPanel>,
        frontmatter: &Entity<FrontmatterView>,
        evals: &Entity<EvalsView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        let evals_service = evals.read(cx).service();
        vec![
            cx.subscribe(pane, |this, _, event: &PaneEvent, cx| {
                this.on_pane_event(event, cx);
            }),
            cx.subscribe_in(
                library,
                window,
                |this, _, event: &LibraryEvent, window, cx| {
                    this.on_library_event(event, window, cx);
                },
            ),
            cx.subscribe_in(
                frontmatter,
                window,
                |this, _, event: &FrontmatterEvent, window, cx| match event {
                    FrontmatterEvent::Edit(field, value) => {
                        this.edit_frontmatter(*field, value, window, cx);
                    }
                },
            ),
            cx.observe(library, |this, _, cx| this.sync_library(cx)),
            cx.subscribe_in(evals, window, |this, _, event: &EvalsEvent, window, cx| {
                this.on_evals_event(event, window, cx);
            }),
            cx.observe(evals, |_, _, cx| cx.notify()),
            cx.on_app_quit(move |_, _| {
                let service = Arc::clone(&evals_service);
                async move { service.shutdown() }
            }),
            cx.on_release(|this, _| this.flush_all()),
            cx.on_app_quit(|this, _| {
                this.flush_all();
                this.services.flush_records();
                async {}
            }),
        ]
    }

    fn start_watcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (sender, mut receiver) = futures::channel::mpsc::unbounded::<ExternalChanges>();
        let user_data = self.services.user_data.clone();
        let controller = ExternalChangesController::start(WatchConfig::new(
            move || library_panel::list(&user_data),
            move |changes| {
                sender.unbounded_send(changes).log_warn();
            },
        ));
        self.watcher = Some(controller);
        self.watch();
        cx.spawn_in(window, async move |this, cx| {
            while let Some(changes) = receiver.next().await {
                if this
                    .update_in(cx, |this, window, cx| {
                        this.external_changes(changes, window, cx);
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
        let watching = self
            .session_path()
            .map_or(Watching::Nothing, |path| Watching::File(path.to_path_buf()));
        if self.watched.as_ref() == Some(&watching) {
            return;
        }
        if let Some(watcher) = &self.watcher {
            let file = match &watching {
                Watching::File(path) => path.to_str(),
                Watching::Nothing => None,
            };
            watcher.watch_file(file);
        }
        self.watched = Some(watching);
    }

    pub(crate) fn text(&self, cx: &App) -> String {
        self.pane.read(cx).text(cx)
    }

    fn unsaved_changes(&self, cx: &App) -> bool {
        has_unsaved_changes(self.session.as_ref(), &self.text(cx), SAMPLE_DOCUMENT)
    }

    fn notify(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.notify_with(text, ToastTone::Neutral, Vec::new(), cx);
    }

    fn notify_success(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.notify_with(text, ToastTone::Success, Vec::new(), cx);
    }

    fn notify_failure(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.notify_with(text, ToastTone::Danger, Vec::new(), cx);
    }

    pub(crate) fn notify_with(
        &mut self,
        text: impl Into<SharedString>,
        tone: ToastTone,
        actions: Vec<ToastAction>,
        cx: &mut Context<Self>,
    ) {
        let duration = if actions.is_empty() {
            TOAST_DURATION
        } else {
            ACTION_TOAST_DURATION
        };
        let timer = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(duration).await;
            this.update(cx, |this, cx| {
                this.toast = None;
                cx.notify();
            })
            .log_warn();
        });
        self.toast_serial += 1;
        let serial = self.toast_serial;
        self.toast = Some(Toast {
            serial,
            text: text.into(),
            tone,
            actions,
            _timer: timer,
        });
        cx.notify();
    }

    fn on_pane_event(&mut self, event: &PaneEvent, cx: &mut Context<Self>) {
        match event {
            PaneEvent::Changed => {
                let text = self.text(cx);
                if text != self.last_text {
                    self.last_text.clone_from(&text);
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
            PaneEvent::Toast {
                text,
                tone,
                actions,
            } => {
                let actions = actions.iter().cloned().map(ToastAction::Pane).collect();
                self.notify_with(text.clone(), *tone, actions, cx);
            }
        }
        cx.notify();
    }

    fn text_changed(&mut self, text: &str, cx: &mut Context<Self>) {
        self.session = edited_session(self.session.as_ref(), text);
        self.announce(text, cx);
        self.frontmatter_due = true;
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
            this.update(cx, |this, _| this.flush_unsaved()).log_warn();
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
                log::error!("Unsaved text write failed: {error}");
            }
        }
    }

    fn discard_unsaved(&mut self, path: &Path) {
        let key = history_key(Some(path));
        self.unsaved.remove(&key);
        if let Some(db) = &self.db {
            db.clear_unsaved(&key).log_err();
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
            this.update(cx, |this, cx| {
                this.history.poll(now_ms());
                this.settle_history(cx);
            })
            .log_warn();
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
        self.frontmatter_due = true;
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
        self.last_text.clone_from(&arrived.text);
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
                Opened { name, path, text },
                true,
                Arrival::Opened,
                window,
                cx,
            ),
            FileReadResult::Cancelled => {}
            FileReadResult::Failed { reason } => self.notify_failure(
                with_reason("Could not open the file".into(), Some(&reason)),
                cx,
            ),
        }
    }

    fn reload(&mut self, conflict: Conflict, window: &mut Window, cx: &mut Context<Self>) {
        match read_markdown(&conflict.path) {
            FileReadResult::Opened { path, name, text } => self.arrive(
                Opened { name, path, text },
                false,
                Arrival::Reloaded,
                window,
                cx,
            ),
            FileReadResult::Cancelled => {}
            FileReadResult::Failed { reason } => self.notify_failure(
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
        self.last_text.clone_from(&text);
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
        if changes.library {
            self.library
                .update(cx, super::library_panel::LibraryPanel::refresh);
        }
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
            noticed: self.noticed.as_ref().map(DiskState::as_deref),
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
                self.noticed = Some(DiskState::of(disk));
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
        self.last_text.clone_from(&text);
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
            self.leave_index = 0;
            window.focus(&self.leave_focus, cx);
            cx.notify();
        } else {
            self.continue_with(intent, window, cx);
        }
    }

    fn continue_with(&mut self, intent: LeaveIntent, window: &mut Window, cx: &mut Context<Self>) {
        match intent {
            LeaveIntent::New => self.clear_document(window, cx),
            LeaveIntent::Open => self.prompt_open(window, cx),
            LeaveIntent::OpenPath(path) => {
                self.load(path.clone(), window, cx);
                let request = self.library_request.take();
                if let Some(LibraryRequest {
                    path: requested,
                    focus_text: true,
                    ..
                }) = request
                    && requested == path
                    && self.session_path() == Some(path.as_path())
                {
                    self.focus_text_from_library(window, cx);
                }
            }
            LeaveIntent::CreateSkill(request) => self.create_skill(request, window, cx),
        }
        self.frontmatter_due = true;
    }

    fn create_skill(
        &mut self,
        request: CreateSkillRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match library_panel::create(&self.services.user_data, &request) {
            CreateSkillResult::Failed { reason } => {
                if let Some(dialog) = &self.new_skill {
                    dialog.update(cx, |dialog, cx| dialog.fail(reason, cx));
                }
            }
            CreateSkillResult::Created { path, name } => {
                self.new_skill = None;
                self.library
                    .update(cx, super::library_panel::LibraryPanel::refresh);
                let path = PathBuf::from(path);
                self.load(path.clone(), window, cx);
                if self.session_path() == Some(path.as_path()) {
                    self.notify_success(format!("Created {name}"), cx);
                    self.focus_text(window, cx);
                }
            }
        }
    }

    fn on_library_event(
        &mut self,
        event: &LibraryEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            LibraryEvent::Open { path, focus_text } => {
                self.open_from_library(PathBuf::from(path), *focus_text, window, cx);
            }
            LibraryEvent::OpenResult { path } => {
                self.search_opening = true;
                self.open_from_library(PathBuf::from(path), true, window, cx);
                self.search_opening = false;
            }
            LibraryEvent::NewSkill => self.open_new_skill(window, cx),
            LibraryEvent::Escape => self.focus_text_from_library(window, cx),
            LibraryEvent::Notify(text, tone) => {
                self.notify_with(text.clone(), *tone, Vec::new(), cx);
            }
            LibraryEvent::RootsChanged => {
                if let Some(watcher) = &self.watcher {
                    watcher.roots_changed();
                }
            }
            LibraryEvent::SearchLeft => {
                if !self.search_opening && self.leave.is_none() {
                    self.search_unfolded = false;
                }
            }
        }
        cx.notify();
    }

    fn open_from_library(
        &mut self,
        path: PathBuf,
        focus_text: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session_path() == Some(path.as_path()) {
            if focus_text {
                self.focus_text_from_library(window, cx);
            }
            return;
        }
        self.library_request = Some(LibraryRequest {
            path: path.clone(),
            focus_text,
            from_search: self.search_opening,
        });
        self.guard(LeaveIntent::OpenPath(path), window, cx);
    }

    fn modal_open(&self) -> bool {
        self.leave.is_some()
            || self.new_skill.is_some()
            || self.key_dialog.is_some()
            || self.model_dialog.is_some()
    }

    fn open_skill(&self, cx: &App) -> Option<EvalSkill> {
        let active = self.open_path_string()?;
        skill_of_file(self.library.read(cx).roots(), &|path| path == active).map(EvalSkill::from)
    }

    fn reading(&self) -> bool {
        self.page == Page::Reading
    }

    fn leave_reading(&mut self) {
        if self.reading() {
            self.page = Page::Text;
        }
    }

    fn evals_shown(&self, cx: &App) -> bool {
        self.page == Page::Evals && self.open_skill(cx).is_some()
    }

    fn busy_name(&self, cx: &App) -> Option<String> {
        let evals = self.evals.read(cx);
        let going = evals.going()?;
        if evals.skill().map(|skill| skill.path.as_str()) == Some(going) {
            return None;
        }
        self.library
            .read(cx)
            .roots()
            .iter()
            .flat_map(|root| root.skills.iter())
            .find(|skill| skill.real_path == going)
            .map(|skill| skill.name.clone())
    }

    pub(crate) fn toggle_evals(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.evals_shown(cx) {
            self.close_history(window, cx);
        } else {
            self.open_evals(window, cx);
        }
    }

    fn open_evals(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = HistoryMode::Editing;
        self.page = Page::Evals;
        self.hovered = None;
        self.open_menu = None;
        self.unsaved_checked = None;
        window.focus(&self.focus, cx);
        self.evals.update(cx, super::evals_view::EvalsView::refresh);
        cx.notify();
    }

    fn run_toast_action(
        &mut self,
        action: &ToastAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toast = None;
        match action {
            ToastAction::ShowEvals { path } => {
                if self.open_skill(cx).is_some_and(|skill| &skill.path == path) {
                    self.open_evals(window, cx);
                }
            }
            ToastAction::Pane(action) => {
                self.show_editing(window, cx);
                match action {
                    PaneToastAction::RetrySuggestion(id) => {
                        self.pane.update(cx, |pane, cx| pane.retry(id, window, cx));
                    }
                    PaneToastAction::ShowSuggestion(id) => {
                        self.pane.update(cx, |pane, cx| pane.show(id, window, cx));
                    }
                    PaneToastAction::RetryReview(focus) => {
                        let focus = focus.clone();
                        self.pane
                            .update(cx, |pane, cx| pane.start_review(focus, cx));
                    }
                }
            }
        }
        cx.notify();
    }

    fn unsaved_skill_files(&mut self, skill: &EvalSkill, cx: &App) -> Vec<String> {
        let markdown: Vec<_> = skill.files.iter().filter(|file| file.is_markdown).collect();
        let active = self.open_path_string();
        let is_open = |path: &str| active.as_deref() == Some(path);
        let open_dirty = self.unsaved_changes(cx);
        let stored = markdown
            .iter()
            .filter(|file| !is_open(&file.path))
            .filter(|file| {
                let stored = self.offered(Path::new(&file.path));
                stored.is_some()
                    && stored_is_unsaved(
                        std::fs::read_to_string(&file.path).ok().as_deref(),
                        stored.as_ref(),
                    )
            })
            .map(|file| file.path.clone())
            .collect();
        let paths: Vec<String> = markdown.iter().map(|file| file.path.clone()).collect();
        let unsaved = files_with_unsaved_text(&UnsavedInput {
            files: &paths,
            is_open: &is_open,
            open_dirty,
            stored: &stored,
        });
        markdown
            .iter()
            .filter(|file| unsaved.contains(&file.path))
            .map(|file| file.name.clone())
            .collect()
    }

    fn check_unsaved(&mut self, force: bool, cx: &mut Context<Self>) {
        let Some(skill) = self.open_skill(cx) else {
            return;
        };
        let key = (skill.path.clone(), self.unsaved_changes(cx));
        if !force && self.unsaved_checked.as_ref() == Some(&key) {
            return;
        }
        self.unsaved_checked = Some(key);
        let names = self.unsaved_skill_files(&skill, cx);
        self.evals
            .update(cx, |evals, cx| evals.set_unsaved(names, cx));
    }

    fn on_evals_event(&mut self, event: &EvalsEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            EvalsEvent::Ask => {
                self.evals
                    .update(cx, |evals, cx| evals.set_checking(true, cx));
                self.check_unsaved(true, cx);
                self.evals.update(cx, |evals, cx| {
                    evals.set_checking(false, cx);
                    evals.ask(window, cx);
                });
            }
            EvalsEvent::DialogClosed => window.focus(&self.focus, cx),
            EvalsEvent::Ended { path, name, result } => {
                let here = self.open_skill(cx).is_some_and(|skill| &skill.path == path);
                if here && self.evals_shown(cx) {
                    return;
                }
                self.notify_with(
                    ended_text(name, result),
                    ended_tone(result),
                    here.then(|| ToastAction::ShowEvals { path: path.clone() })
                        .into_iter()
                        .collect(),
                    cx,
                );
            }
        }
    }

    fn placement(&self) -> Placement {
        library_panel::placement(self.viewport_width)
    }

    fn fold_library(&mut self, cx: &mut Context<Self>) {
        self.leaving.library_reopened = false;
        if self.library_open && !cx.reduce_motion() {
            self.leaving.library = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(ui::MOTION_LEAVE).await;
                this.update(cx, |this, cx| {
                    this.leaving.library = None;
                    cx.notify();
                })
                .log_warn();
            }));
        }
        self.library_open = false;
        self.search_unfolded = false;
        cx.notify();
    }

    fn focus_text_from_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let fold_search = self.search_unfolded;
        self.focus_text(window, cx);
        if fold_search || self.placement() == Placement::Overlay {
            self.fold_library(cx);
        }
    }

    fn open_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.library_open = true;
        self.leaving.library_reopened = self.leaving.library.take().is_some();
        self.library
            .update(cx, |library, cx| library.focus_tree(window, cx));
        cx.notify();
    }

    fn fold_and_return(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fold_library(cx);
        self.focus_text(window, cx);
    }

    fn toggle_library(&mut self, _: &ToggleLibrary, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal_open() {
            return;
        }
        self.open_menu = None;
        if !self.library_open {
            self.open_library(window, cx);
        } else if self.library.read(cx).holds_focus(window, cx) {
            self.fold_and_return(window, cx);
        } else {
            self.library
                .update(cx, |library, cx| library.focus_tree(window, cx));
        }
    }

    fn toggle_library_from_rail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.library_open {
            self.fold_and_return(window, cx);
        } else {
            self.open_library(window, cx);
        }
    }

    fn search_library(&mut self, _: &SearchLibrary, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal_open() {
            return;
        }
        self.open_menu = None;
        if !self.library_open {
            self.search_unfolded = true;
            self.library_open = true;
            self.leaving.library_reopened = self.leaving.library.take().is_some();
        }
        self.library
            .update(cx, |library, cx| library.focus_search(window, cx));
        cx.notify();
    }

    fn open_new_skill(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let roots = self.library.read(cx).roots().to_vec();
        let dialog = cx.new(|cx| NewSkillDialog::new(&roots, window, cx));
        let events = cx.subscribe_in(
            &dialog,
            window,
            |this, _, event: &NewSkillEvent, window, cx| {
                match event {
                    NewSkillEvent::Close => {
                        this.new_skill = None;
                        this.library
                            .update(cx, |library, cx| library.focus_tree(window, cx));
                    }
                    NewSkillEvent::Create(request) => {
                        this.guard(LeaveIntent::CreateSkill(request.clone()), window, cx);
                        if this.leave.is_none()
                            && let Some(dialog) = &this.new_skill
                        {
                            dialog.update(cx, |dialog, cx| dialog.set_busy(false, cx));
                        }
                    }
                }
                cx.notify();
            },
        );
        self._subscriptions.push(events);
        self.new_skill = Some(dialog);
        cx.notify();
    }

    fn open_path_string(&self) -> Option<String> {
        self.session_path().map(|path| {
            std::fs::canonicalize(path)
                .unwrap_or_else(|_| path.to_path_buf())
                .to_string_lossy()
                .into_owned()
        })
    }

    fn sync_library(&mut self, cx: &mut Context<Self>) {
        let active = self.open_path_string();
        self.library
            .update(cx, |library, cx| library.set_active(active, cx));
        self.frontmatter_due = true;
    }

    fn file_label(&self, cx: &App) -> Option<String> {
        let active = self.open_path_string()?;
        skill_file_label(self.library.read(cx).roots(), &|path| path == active)
    }

    fn refresh_frontmatter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.frontmatter_due = false;
        let text = self.text(cx);
        let active = self.open_path_string();
        let open = active.as_ref().and_then(|active| {
            open_skill_file(self.library.read(cx).roots(), &|path| path == active)
        });
        let file_name = open
            .as_ref()
            .map(|open| open.file_name.clone())
            .or_else(|| self.session.as_ref().map(|session| session.name.clone()));
        let folder_name = if is_skill_file(file_name.as_deref()) {
            open.as_ref()
                .map(|open| open.folder_name.clone())
                .or_else(|| {
                    self.session_path()
                        .and_then(Path::parent)
                        .and_then(Path::file_name)
                        .map(|name| name.to_string_lossy().into_owned())
                })
        } else {
            None
        };
        let report = inspect_frontmatter(
            &text,
            &FrontmatterContext {
                file_name: file_name.as_deref(),
                folder_name: folder_name.as_deref(),
            },
        );
        let is_skill = is_skill_file(file_name.as_deref());
        self.pane.update(cx, |pane, _| pane.is_skill = is_skill);
        self.frontmatter
            .update(cx, |view, cx| view.refresh(&text, report, window, cx));
    }

    fn frontmatter_problems(&self, cx: &App) -> Vec<String> {
        report_problems(&self.frontmatter.read(cx).report)
    }

    fn edit_frontmatter(
        &mut self,
        field: SkillField,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.text(cx);
        let values = match field {
            SkillField::Name => SkillFieldValues::name(value),
            SkillField::Description => SkillFieldValues::description(value),
        };
        match plan_field_edit(&text, &values) {
            FieldEditPlan::Unchanged => {}
            FieldEditPlan::Refused { problem } => {
                self.notify_failure(describe_frontmatter_problem(&problem), cx);
            }
            FieldEditPlan::Change { changes } => {
                let from = changes.iter().map(|change| change.from).min().unwrap_or(0);
                let to = changes.iter().map(|change| change.to).max().unwrap_or(0);
                let next =
                    desafinado_features::skills::frontmatter_edit::apply_changes(&text, &changes);
                let end = to + next.len() - text.len();
                let insert = next[from..end].to_string();
                self.pane.update(cx, |pane, cx| {
                    pane.edit_range(from..to, &insert, window, cx);
                });
            }
        }
    }

    fn show_frontmatter_problems(
        &mut self,
        _: &ShowFrontmatterProblems,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_menu = None;
        self.leave_reading();
        self.close_history(window, cx);
        self.frontmatter
            .update(cx, |view, cx| view.focus_panel(window, cx));
        cx.notify();
    }

    fn answer_leave(&mut self, choice: LeaveChoice, window: &mut Window, cx: &mut Context<Self>) {
        let Some(intent) = self.leave.take() else {
            return;
        };
        match choice {
            LeaveChoice::Cancel => {
                self.focus_text(window, cx);
                if let LeaveIntent::OpenPath(_) = intent
                    && let Some(request) = self.library_request.take()
                {
                    self.library.update(cx, |library, cx| {
                        if request.from_search {
                            library.focus_search(window, cx);
                        } else {
                            library.focus_tree(window, cx);
                        }
                    });
                }
                if let LeaveIntent::CreateSkill(_) = intent
                    && let Some(dialog) = &self.new_skill
                {
                    dialog.update(cx, |dialog, cx| dialog.set_busy(false, cx));
                }
            }
            LeaveChoice::Discard => {
                if let Some(path) = self.session_path().map(Path::to_path_buf) {
                    self.discard_unsaved(&path);
                }
                self.continue_with(intent, window, cx);
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
                this.update_in(cx, |this, window, cx| this.load(path, window, cx))
                    .log_warn();
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
        if self.writing.is_some() {
            self.save_again = true;
            return;
        }
        let text = self.text(cx);
        self.writing = Some(text.clone());
        self.history.saving(text.clone(), now_ms());
        self.settle_history(cx);
        let name = self.session.as_ref().map_or_else(
            || suggested_file_name(&text),
            |session| session.name.clone(),
        );
        match self.session.clone() {
            Some(FileSession {
                path: Some(path),
                saved_text,
                ..
            }) if !save_as => {
                let request = SaveRequest {
                    path,
                    text: text.clone(),
                    expected: saved_text,
                };
                let write = cx.background_spawn(async move { save_markdown(&request) });
                cx.spawn_in(window, async move |this, cx| {
                    let result = write.await;
                    this.update_in(cx, |this, window, cx| {
                        this.finish_save(result, text, name, window, cx);
                    })
                    .log_warn();
                })
                .detach();
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
                Ok(Ok(Some(chosen))) => {
                    let text = text.clone();
                    cx.background_spawn(async move { save_as_target(&chosen, &text) })
                        .await
                }
                _ => FileSaveResult::Cancelled,
            };
            this.update_in(cx, |this, window, cx| {
                this.finish_save(result, text, name, window, cx);
            })
            .log_warn();
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
            FileSaveResult::Failed { reason } => self.notify_failure(
                with_reason(format!("Could not save {name}"), Some(&reason)),
                cx,
            ),
            FileSaveResult::Conflict => match self.session_path().map(Path::to_path_buf) {
                Some(path) => self.conflict = Some(Conflict { name, path }),
                None => self.notify_failure(
                    format!("Could not save {name}: the file changed on disk"),
                    cx,
                ),
            },
            FileSaveResult::Saved { path, name } => {
                let current = self.text(cx);
                self.session = Some(FileSession {
                    name: name.clone(),
                    path: Some(path),
                    saved_text: text.clone(),
                    dirty: current != text,
                });
                self.arrived();
                self.history.saved(text, name.clone(), now_ms());
                self.settle_history(cx);
                self.announce(&current, cx);
                self.notify_success(format!("Saved {name}"), cx);
                self.watch();
            }
        }
        if std::mem::take(&mut self.save_again) && saved {
            self.save(false, window, cx);
            cx.notify();
            return;
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
        let from_evals = self.evals_shown(cx);
        let was_editing = self.visible_mode() == HistoryMode::Editing && !from_evals;
        let from = if from_evals {
            HistoryMode::Editing
        } else {
            self.visible_mode()
        };
        if self.page == Page::Evals {
            self.page = Page::Text;
        }
        self.mode = next_history_mode(&ids, from, action);
        let editing = self.visible_mode() == HistoryMode::Editing;
        if editing {
            self.hovered = None;
            if !was_editing {
                self.focus_text(window, cx);
            }
        } else {
            self.open_menu = None;
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    fn show_editing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.leave_reading();
        if self.visible_mode() != HistoryMode::Editing {
            self.close_history(window, cx);
        }
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
            this.update(cx, |this, cx| {
                this.flash = None;
                cx.notify();
            })
            .log_warn();
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
        if self.evals_shown(cx) {
            return;
        }
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
        if self.open_menu.is_some() {
            self.leave_menu(window, cx);
            return;
        }
        if self.review_focus.take().is_some() {
            self.focus_text(window, cx);
        }
        cx.notify();
    }

    fn enabled_rows(&self, kind: MenuKind, cx: &App) -> Vec<bool> {
        self.menu_rows(kind, cx)
            .iter()
            .map(|row| !row.disabled)
            .collect()
    }

    fn open_menu_at(
        &mut self,
        kind: MenuKind,
        focus_first: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.open_menu.is_none() {
            self.menu_return = window.focused(cx);
        }
        self.review_focus = None;
        self.open_menu = Some(kind);
        self.menu_active = if focus_first {
            next_enabled(&self.enabled_rows(kind, cx), None, true)
        } else {
            None
        };
        window.focus(&self.menu_focus, cx);
        cx.notify();
    }

    fn leave_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = None;
        self.menu_active = None;
        match self.menu_return.take() {
            Some(handle) => window.focus(&handle, cx),
            None => self.focus_back(window, cx),
        }
        cx.notify();
    }

    fn toggle_menu(&mut self, kind: MenuKind, window: &mut Window, cx: &mut Context<Self>) {
        if self.open_menu == Some(kind) {
            self.leave_menu(window, cx);
        } else {
            self.open_menu_at(kind, true, window, cx);
        }
    }

    fn move_in_menu(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(kind) = self.open_menu else {
            return;
        };
        self.menu_active = next_enabled(&self.enabled_rows(kind, cx), self.menu_active, forward);
        cx.notify();
    }

    fn shift_menu(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(kind) = self.open_menu else {
            return;
        };
        let index = MENUS.iter().position(|menu| *menu == kind).unwrap_or(0);
        self.open_menu_at(
            MENUS[shifted(index, MENUS.len(), forward)],
            true,
            window,
            cx,
        );
    }

    fn select_menu_row(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(kind) = self.open_menu else {
            return;
        };
        let Some(row) = self.menu_rows(kind, cx).into_iter().nth(index) else {
            return;
        };
        if row.disabled {
            return;
        }
        self.leave_menu(window, cx);
        window.dispatch_action(row.action, cx);
    }

    fn focus_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match &self.preferences {
            Some(preferences) => {
                let handle = preferences.read(cx).focus_handle(cx);
                window.focus(&handle, cx);
            }
            None => self.focus_text(window, cx),
        }
    }

    fn open_preferences(
        &mut self,
        _: &OpenPreferences,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_menu = None;
        if let Some(preferences) = &self.preferences {
            let handle = preferences.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
            return;
        }
        let settings = self.quick_actions.clone();
        let panel = cx.new(|cx| PreferencesPanel::new(settings, window, cx));
        let events = cx.subscribe_in(
            &panel,
            window,
            |this, panel, event: &PreferencesEvent, window, cx| match event {
                _ if this.preferences.as_ref() != Some(panel) => {}
                PreferencesEvent::Close => {
                    this.close_preferences(cx);
                    this.focus_text(window, cx);
                    cx.notify();
                }
                PreferencesEvent::OpenModel => this.open_model(window, cx),
                PreferencesEvent::OpenKey => this.open_key(window, cx),
                PreferencesEvent::OpenSpend => this.show_spend(&ShowSpend, window, cx),
                PreferencesEvent::QuickActions(settings) => {
                    this.change_quick_actions(settings.clone(), cx);
                }
            },
        );
        self._dialog_subscriptions.push(events);
        self.preferences = Some(panel);
        self.leaving.preferences = None;
        cx.notify();
    }

    fn close_preferences(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.preferences.take() else {
            return;
        };
        if cx.reduce_motion() {
            return;
        }
        let done = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ui::MOTION_LEAVE).await;
            this.update(cx, |this, cx| {
                this.leaving.preferences = None;
                cx.notify();
            })
            .log_warn();
        });
        self.leaving.preferences = Some((panel, done));
    }

    fn change_quick_actions(&mut self, settings: QuickActionSettings, cx: &mut Context<Self>) {
        self.quick_actions.clone_from(&settings);
        self.pane.update(cx, |pane, cx| {
            pane.quick_actions = enabled_quick_actions(&settings, true);
            cx.notify();
        });
        let user_data = self.services.user_data.clone();
        let text = serialize_quick_action_settings(&settings);
        let previous = self.quick_actions_saving.take();
        let write = cx.background_spawn(async move { save_quick_actions(&user_data, &text) });
        self.quick_actions_saving = Some(cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            if let Err(error) = write.await {
                log::error!("Quick actions could not be saved: {error}");
                this.update(cx, |this, cx| {
                    this.notify("Quick actions could not be saved.", cx);
                })
                .log_warn();
            }
        }));
    }

    fn open_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dialog = cx.new(|cx| KeyDialog::new(Arc::clone(&self.services), window, cx));
        let events = cx.subscribe_in(&dialog, window, |this, _, event: &KeyEvent, window, cx| {
            match event {
                KeyEvent::State(state) => this.key_present = state.present,
                KeyEvent::Saved(message) => this.notify_success(message.clone(), cx),
                KeyEvent::Close => {
                    this.key_dialog = None;
                    this.focus_back(window, cx);
                }
            }
            cx.notify();
        });
        self._dialog_subscriptions.push(events);
        self.key_dialog = Some(dialog);
        cx.notify();
    }

    fn open_model(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dialog = cx.new(|cx| ModelDialog::new(Arc::clone(&self.services), window, cx));
        let events = cx.subscribe_in(
            &dialog,
            window,
            |this, _, event: &ModelEvent, window, cx| {
                match event {
                    ModelEvent::Saved(message) => this.notify(message.clone(), cx),
                    ModelEvent::Close => {
                        this.model_dialog = None;
                        this.focus_back(window, cx);
                    }
                }
                cx.notify();
            },
        );
        self._dialog_subscriptions.push(events);
        self.model_dialog = Some(dialog);
        cx.notify();
    }

    fn show_spend(&mut self, _: &ShowSpend, window: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = None;
        self.close_preferences(cx);
        if let Some(spend) = &self.spend {
            spend.update(cx, super::spend_view::SpendView::reload);
            return;
        }
        let view = cx.new(|cx| SpendView::new(Arc::clone(&self.services), cx));
        let events = cx.subscribe_in(&view, window, |this, _, event: &SpendEvent, window, cx| {
            match event {
                SpendEvent::Close => {
                    this.spend = None;
                    this.focus_text(window, cx);
                }
            }
            cx.notify();
        });
        self._dialog_subscriptions.push(events);
        self.spend = Some(view);
        cx.notify();
    }

    fn menu_rows(&self, kind: MenuKind, cx: &App) -> Vec<MenuRow> {
        let open = self.pane.read(cx).open_review_count();
        match kind {
            MenuKind::File => {
                let problems = self.frontmatter_problems(cx).len();
                let mut rows = vec![
                    row("New", Some("Ctrl+N"), Box::new(NewDocument), false),
                    row("Open…", Some("Ctrl+O"), Box::new(OpenDocument), false),
                    row("Save", Some("Ctrl+S"), Box::new(SaveDocument), false),
                    row(
                        "Save as…",
                        Some("Ctrl+Shift+S"),
                        Box::new(SaveDocumentAs),
                        false,
                    ),
                ];
                if problems > 0 {
                    rows.push(MenuRow {
                        danger: true,
                        separated: true,
                        ..row(
                            format!(
                                "Frontmatter: {}",
                                if problems == 1 {
                                    "1 problem".to_string()
                                } else {
                                    format!("{problems} problems")
                                }
                            ),
                            None,
                            Box::new(ShowFrontmatterProblems),
                            false,
                        )
                    });
                }
                rows.push(MenuRow {
                    separated: true,
                    ..row(
                        "Preferences…",
                        Some("Ctrl+,"),
                        Box::new(OpenPreferences),
                        false,
                    )
                });
                rows.push(row("AI spend…", None, Box::new(ShowSpend), false));
                rows
            }
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
                MenuRow {
                    separated: true,
                    ..row(
                        if self.reading() {
                            "Edit text"
                        } else {
                            "Reading view"
                        },
                        Some("Ctrl+Alt+R"),
                        Box::new(ToggleReading),
                        false,
                    )
                },
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
        let problems = match kind {
            MenuKind::File => self.frontmatter_problems(cx).len(),
            MenuKind::Edit => 0,
        };
        ui::menu_button(label, label, open, roles)
            .when(problems > 0, |this| {
                this.child(ui::badge(problems.to_string(), ui::Tone::Danger, roles))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    if this.open_menu == Some(kind) {
                        this.leave_menu(window, cx);
                    } else {
                        this.open_menu_at(kind, false, window, cx);
                    }
                }),
            )
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                if *hovered && this.open_menu.is_some() && this.open_menu != Some(kind) {
                    this.open_menu_at(kind, false, window, cx);
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
        let active = self.menu_active;
        let menu = ui::popover_frame("menu", roles)
            .key_context("Menu")
            .track_focus(&self.menu_focus)
            .on_action(cx.listener(|this, _: &MenuDown, _, cx| this.move_in_menu(true, cx)))
            .on_action(cx.listener(|this, _: &MenuUp, _, cx| this.move_in_menu(false, cx)))
            .on_action(
                cx.listener(|this, _: &MenuNext, window, cx| this.shift_menu(true, window, cx)),
            )
            .on_action(cx.listener(|this, _: &MenuPrevious, window, cx| {
                this.shift_menu(false, window, cx);
            }))
            .on_action(cx.listener(|this, _: &MenuSelect, window, cx| {
                if let Some(index) = this.menu_active {
                    this.select_menu_row(index, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &MenuLeave, window, cx| this.leave_menu(window, cx)))
            .absolute()
            .top(px(36.))
            .left(left)
            .min_w(px(240.))
            .py(px(4.))
            .children(
                self.menu_rows(kind, cx)
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| self.render_menu_row(index, item, active, roles, cx)),
            );
        ui::enter(
            match kind {
                MenuKind::File => "menu-in-file",
                MenuKind::Edit => "menu-in-edit",
            },
            menu,
            ui::Enter::Rise(-4.),
            ui::MOTION_ENTER,
        )
    }

    fn render_menu_row(
        &self,
        index: usize,
        item: MenuRow,
        active: Option<usize>,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shortcut = item
            .shortcut
            .map(|shortcut| ui::shortcut_hint(shortcut, false, roles));
        let row = if item.disabled {
            ui::disabled_row(("menu-row", index), roles)
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    if *hovered {
                        this.menu_active = None;
                        cx.notify();
                    }
                }))
                .child(item.label)
                .children(shortcut)
        } else {
            ui::option_row(("menu-row", index), active == Some(index), false, roles)
                .when(item.danger, |this| this.text_color(roles.danger))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                    if *hovered {
                        this.menu_active = Some(index);
                        cx.notify();
                    }
                }))
                .on_click(
                    cx.listener(move |this, _, window, cx| this.select_menu_row(index, window, cx)),
                )
                .child(item.label)
                .children(shortcut)
        };
        row.h(px(28.))
            .px(px(12.))
            .justify_between()
            .gap(px(24.))
            .when(item.separated, |this| this.mt(px(8.)))
            .into_any_element()
    }

    fn open_review(&mut self, _: &StartReview, window: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = None;
        self.leave_reading();
        if self.pane.read(cx).review.is_some() {
            return;
        }
        let input = cx.new(|cx| {
            gpui_kit::component::input::InputState::new(window, cx)
                .placeholder("Focus (optional): tone, concision…")
        });
        input.update(cx, |state, cx| state.focus(window, cx));
        self.review_focus = Some(input);
        cx.notify();
    }

    fn confirm_review(&mut self, _: &ConfirmReview, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.review_focus.take() else {
            return;
        };
        let focus = input.read(cx).value().trim().to_string();
        self.show_editing(window, cx);
        window.focus(&self.focus, cx);
        self.pane
            .update(cx, |pane, cx| pane.start_review(focus, cx));
        cx.notify();
    }

    fn stop_review(&mut self, _: &StopReview, _: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = None;
        self.pane
            .update(cx, super::editor_pane::EditorPane::stop_review);
        cx.notify();
    }

    fn render_review_popover(
        &self,
        input: &Entity<gpui_kit::component::input::InputState>,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let left = px(52.);
        let width = px(384.).min(self.viewport_width - left - px(8.));
        let popover = ui::popover_frame("review-popover", roles)
            .key_context("ReviewPopover")
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                if this.review_focus.take().is_some() {
                    cx.notify();
                }
            }))
            .absolute()
            .top(px(36.))
            .left(left)
            .w(width)
            .p(px(8.))
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(div().flex_1().min_w_0().child(ui::field(
                gpui_kit::component::input::Input::new(input).appearance(false),
                true,
                false,
                roles,
            )))
            .child(
                ui::button("review-start", "Review", ButtonKind::Strong, roles).on_click(
                    cx.listener(|this, _, window, cx| {
                        this.confirm_review(&ConfirmReview, window, cx);
                    }),
                ),
            );
        ui::enter(
            "review-popover-in",
            popover,
            ui::Enter::Rise(-4.),
            ui::MOTION_ENTER,
        )
    }

    fn render_leave(&self, roles: Roles, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.leave_index;
        let choice_button = |index: usize, choice: LeaveChoice| {
            let (id, label, kind) = match choice {
                LeaveChoice::Cancel => ("leave-cancel", "Cancel", ButtonKind::Default),
                LeaveChoice::Discard => ("leave-discard", "Discard", ButtonKind::Danger),
                LeaveChoice::Save => ("leave-save", "Save", ButtonKind::Strong),
            };
            ui::sized_button(
                id,
                label,
                kind,
                ui::ButtonSize::Regular,
                ui::Control::focused(focused == index),
                roles,
            )
            .on_click(cx.listener(move |this, _, window, cx| this.answer_leave(choice, window, cx)))
        };
        ui::modal_backdrop(
            "leave-backdrop",
            roles,
            ui::dialog_frame("leave", px(416.), roles)
                .key_context("LeaveDialog")
                .track_focus(&self.leave_focus)
                .on_action(cx.listener(|this, _: &LeaveCancel, window, cx| {
                    this.answer_leave(LeaveChoice::Cancel, window, cx);
                }))
                .on_action(cx.listener(|this, _: &LeaveNext, _, cx| {
                    this.leave_index = (this.leave_index + 1) % LEAVE_CHOICES.len();
                    cx.notify();
                }))
                .on_action(cx.listener(|this, _: &LeavePrevious, _, cx| {
                    this.leave_index =
                        (this.leave_index + LEAVE_CHOICES.len() - 1) % LEAVE_CHOICES.len();
                    cx.notify();
                }))
                .on_action(cx.listener(|this, _: &LeavePress, window, cx| {
                    let choice = LEAVE_CHOICES[this.leave_index.min(LEAVE_CHOICES.len() - 1)];
                    this.answer_leave(choice, window, cx);
                }))
                .child(ui::title(
                    format!("Save changes to {}?", self.file_name()),
                    roles,
                ))
                .child(ui::detail(
                    "Discard removes your unsaved changes. Cancel keeps you here.",
                    roles,
                ))
                .child(
                    ui::actions_row().children(
                        LEAVE_CHOICES
                            .iter()
                            .enumerate()
                            .map(|(index, choice)| choice_button(index, *choice)),
                    ),
                ),
        )
    }

    fn file_name(&self) -> SharedString {
        self.session
            .as_ref()
            .map_or_else(|| UNTITLED_NAME.into(), |session| session.name.clone())
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
                .type_body()
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
                                .weight_medium()
                                .text_color(roles.heading)
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
                                    this.resolve_conflict(false, window, cx);
                                })),
                        ),
                ),
        )
        .into_any_element()
    }

    fn render_toast(&self, roles: Roles, cx: &mut Context<Self>) -> Option<AnyElement> {
        let toast = self.toast.as_ref()?;
        let actions = toast.actions.clone();
        Some(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(px(64.))
                .flex()
                .justify_center()
                .px(px(24.))
                .child(ui::enter(
                    ("toast-in", toast.serial),
                    ui::toast_frame("toast", toast.tone, roles)
                        .child(toast.text.clone())
                        .children(actions.into_iter().enumerate().map(|(index, action)| {
                            ui::small_button(
                                ("toast-action", index),
                                action.label(),
                                ButtonKind::Default,
                                roles,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.run_toast_action(&action, window, cx);
                                },
                            ))
                        })),
                    ui::Enter::Lift(10.),
                    ui::MOTION_TOAST,
                ))
                .into_any_element(),
        )
    }
}

fn row(
    label: impl Into<SharedString>,
    shortcut: Option<&'static str>,
    action: Box<dyn Action>,
    disabled: bool,
) -> MenuRow {
    MenuRow {
        label: label.into(),
        danger: false,
        shortcut,
        action,
        disabled,
        separated: false,
    }
}

struct Start {
    db: Option<Arc<HistoryDb>>,
    store: Arc<dyn HistoryStore>,
    initial: String,
    notice: Option<(String, ToastTone)>,
    file: Option<StartFile>,
    file_path: Option<PathBuf>,
    remembered: Option<PathBuf>,
    first_initial: Vec<VersionSummary>,
}

fn open_start(launch: Option<PathBuf>, user_data: &Path) -> Start {
    let db = HistoryDb::open(&user_data.join(HISTORY_FILE))
        .map_err(|error| log::error!("History could not open: {error}"))
        .ok()
        .map(Arc::new);
    let store: Arc<dyn HistoryStore> = match &db {
        Some(db) => Arc::new(DbHistoryStore::new(Arc::clone(db), now_ms)),
        None => Arc::new(MemoryHistoryStore::new()),
    };
    let from_launch = launch
        .map(|path| reopen(db.as_deref(), &path, "Could not open"))
        .unwrap_or_default();
    let launched = from_launch.start.as_ref().map(|start| start.path.clone());
    if let Some(path) = &launched {
        save_last_file(user_data, path);
    }
    let last_file = launched.clone().or_else(|| load_last_file(user_data));
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
        clear_last_file(user_data);
        let key = history_key(Some(path));
        if let Some(db) = &db
            && load_unsaved(Some(db), &key).is_none_or(|unsaved| unsaved.text == initial)
        {
            db.clear_unsaved(&key).log_err();
        }
    }
    let start = reopened.start.clone();
    let start_path = start.as_ref().map(|start| start.path.clone());
    let start_key = history_key(start_path.as_deref());
    store
        .record(
            &start_key,
            RecordInput {
                at: now_ms(),
                cause: VersionCause::Edited,
                text: initial.clone(),
            },
        )
        .log_err();
    let first_initial = store.list(&start_key).unwrap_or_default();
    let remembered = if reopened.gone {
        None
    } else {
        start_path.clone().or(last_file)
    };
    Start {
        db,
        store,
        initial,
        notice,
        file: start,
        file_path: start_path,
        remembered,
        first_initial,
    }
}

fn document_history(
    store: Arc<dyn HistoryStore>,
    first: Option<PathBuf>,
    first_initial: Vec<VersionSummary>,
) -> SwitchableHistory<PathBuf> {
    SwitchableHistory::new(HubOptions {
        key_of: Box::new(|reference: Option<&PathBuf>| {
            history_key(reference.map(PathBuf::as_path))
        }),
        create: Box::new(move |key, initial| {
            let mut options = DocumentHistoryOptions::new(Arc::clone(&store), key, HISTORY_IDLE_MS);
            options.initial = initial;
            options.on_error = Box::new(|error| log::error!("History write failed: {error}"));
            DocumentHistory::new(options)
        }),
        first,
        first_initial,
    })
}

fn session_of(start: Option<&StartFile>, initial: &str) -> (Option<FileSession>, Option<Conflict>) {
    let session = start.map(|start| FileSession {
        name: start.name.clone(),
        path: Some(start.path.clone()),
        saved_text: start.saved_text.clone(),
        dirty: initial != start.saved_text,
    });
    let conflict = start
        .filter(|start| start.disk_changed)
        .map(|start| Conflict {
            name: start.name.clone(),
            path: start.path.clone(),
        });
    (session, conflict)
}

fn ended_tone(result: &EvalRunResult) -> ToastTone {
    match result {
        EvalRunResult::Completed { .. } => ToastTone::Success,
        EvalRunResult::NotComplete { .. } => ToastTone::Neutral,
        _ => ToastTone::Danger,
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

fn preferences_motion(panel: Entity<PreferencesPanel>, open: bool) -> AnyElement {
    let panel = div().relative().size_full().child(panel);
    if open {
        ui::enter(
            "preferences-in",
            panel,
            ui::Enter::Slide(16.),
            ui::MOTION_ENTER,
        )
        .into_any_element()
    } else {
        ui::leave(
            "preferences-out",
            panel.child(ui::input_cover("preferences-cover")),
            ui::Enter::Slide(16.),
        )
        .into_any_element()
    }
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let roles = theme::roles(cx);
        self.viewport_width = window.viewport_size().width;
        let active = self.open_path_string();
        self.library
            .update(cx, |library, cx| library.set_active(active, cx));
        if self.frontmatter_due {
            self.refresh_frontmatter(window, cx);
        }
        let open_skill = self.open_skill(cx);
        let evals_skill = open_skill.clone();
        self.evals
            .update(cx, |evals, cx| evals.set_skill(evals_skill, window, cx));
        let busy_name = self.busy_name(cx);
        self.evals
            .update(cx, |evals, _| evals.set_busy_name(busy_name));
        let evals_shown = self.page == Page::Evals && open_skill.is_some();
        if evals_shown {
            self.check_unsaved(false, cx);
        }
        if self.spend.is_some() || self.visible_mode() != HistoryMode::Editing {
            self.leave_reading();
        }
        let text_shown = !evals_shown
            && !self.reading()
            && self.spend.is_none()
            && self.visible_mode() == HistoryMode::Editing;
        if !text_shown {
            self.pane.update(cx, |pane, _| pane.forget_bubble());
        }
        let mode = if evals_shown {
            HistoryMode::Editing
        } else {
            self.visible_mode()
        };
        let evals_button = open_skill.as_ref().map(|_| EvalsButton {
            pressed: evals_shown,
            busy: self.evals.read(cx).going().is_some(),
        });
        let main = self.render_main(mode, evals_shown, evals_button, roles, cx);
        let root = self
            .with_actions(
                div()
                    .id("workspace")
                    .key_context(if mode == HistoryMode::Editing && !evals_shown {
                        "Workspace"
                    } else {
                        "HistoryView"
                    })
                    .track_focus(&self.focus),
                cx,
            )
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(roles.canvas)
            .text_color(roles.ink)
            .child(
                TitleBar::new().child(
                    h_flex_row()
                        .gap(px(4.))
                        .child(self.render_menu_button(MenuKind::File, "File", roles, cx))
                        .child(self.render_menu_button(MenuKind::Edit, "Edit", roles, cx)),
                ),
            )
            .child(main);
        self.with_overlays(root, roles, cx)
    }
}

impl Workspace {
    fn render_editing(&self, cx: &mut Context<Self>) -> AnyElement {
        let has_panel = self.frontmatter.read(cx).has_panel();
        let height = self.frontmatter_height.get();
        let scrolled = self.frontmatter_scrolled.get().min(height);
        let measured = Rc::clone(&self.frontmatter_height);
        let entity = cx.entity().downgrade();
        let wheel_entity = entity.clone();
        let wheel_scrolled = Rc::clone(&self.frontmatter_scrolled);
        let wheel_height = Rc::clone(&self.frontmatter_height);
        let editor = self.pane.read(cx).editor.clone();
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .when(has_panel, |this| {
                this.child(
                    div()
                        .flex_none()
                        .overflow_hidden()
                        .when(height > px(0.), |this| this.h(height - scrolled))
                        .child(
                            div()
                                .relative()
                                .mt(-scrolled)
                                .child(column_row(self.frontmatter.clone()).pt(px(4.)))
                                .child(
                                    canvas(
                                        move |bounds, _, cx| {
                                            if measured.replace(bounds.size.height)
                                                != bounds.size.height
                                                && let Some(entity) = entity.upgrade()
                                            {
                                                entity.update(cx, |_, cx| cx.notify());
                                            }
                                        },
                                        |_, (), _, _| {},
                                    )
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .size_full(),
                                ),
                        ),
                )
            })
            .child(div().flex_1().min_h_0().child(self.pane.clone()))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, (), window, _| {
                        if !has_panel {
                            return;
                        }
                        let scrolled = Rc::clone(&wheel_scrolled);
                        let height = Rc::clone(&wheel_height);
                        let entity = wheel_entity.clone();
                        let editor = editor.clone();
                        window.on_mouse_event(move |event: &ScrollEvent, phase, _, cx| {
                            if phase != DispatchPhase::Capture || !bounds.contains(&event.position)
                            {
                                return;
                            }
                            let full = height.get();
                            let delta = event.delta.pixel_delta(px(24.)).y;
                            let at_top = editor.read(cx).scroll_offset().y >= px(0.);
                            let current = scrolled.get().min(full);
                            let next = frontmatter_scroll(current, full, delta, at_top);
                            if next != current {
                                scrolled.set(next);
                                cx.stop_propagation();
                                if let Some(entity) = entity.upgrade() {
                                    entity.update(cx, |_, cx| cx.notify());
                                }
                            }
                        });
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }

    fn render_missing_key(&self, roles: Roles, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("missing-key")
            .w_full()
            .flex_none()
            .bg(roles.raised)
            .px(px(24.))
            .py(px(12.))
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            .gap(px(12.))
            .type_body()
            .text_color(roles.ink)
            .child("OpenRouter key missing.")
            .child(
                ui::button("set-key", "Set the key", ButtonKind::Strong, roles)
                    .on_click(cx.listener(|this, _, window, cx| this.open_key(window, cx))),
            )
    }

    fn render_rail(&self, roles: Roles, cx: &mut Context<Self>) -> impl IntoElement {
        let tip = |tip: &'static str| {
            move |window: &mut Window, cx: &mut App| {
                component::tooltip::Tooltip::new(tip).build(window, cx)
            }
        };
        div()
            .w(px(library_panel::RAIL_WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.))
            .pt(px(24.))
            .bg(roles.raised)
            .child(
                ui::icon_button(
                    "rail-library",
                    component::IconName::BookOpen,
                    self.library_open,
                    roles,
                )
                .tooltip(tip("Library (Ctrl+B)"))
                .on_click(
                    cx.listener(|this, _, window, cx| this.toggle_library_from_rail(window, cx)),
                ),
            )
            .child(
                ui::icon_button("rail-search", component::IconName::Search, false, roles)
                    .tooltip(tip("Search the Library (Ctrl+P)"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.search_library(&SearchLibrary, window, cx);
                    })),
            )
    }
}

fn h_flex_row() -> Div {
    div().flex().flex_row().items_center()
}

fn frontmatter_scroll(current: Pixels, full: Pixels, delta: Pixels, editor_at_top: bool) -> Pixels {
    if delta < px(0.) && current < full {
        (current - delta).min(full)
    } else if delta > px(0.) && editor_at_top && current > px(0.) {
        (current - delta).max(px(0.))
    } else {
        current
    }
}

impl Workspace {
    fn with_actions(&self, root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(Self::new_document))
            .on_action(cx.listener(Self::open_document))
            .on_action(cx.listener(Self::save_document))
            .on_action(cx.listener(Self::save_document_as))
            .on_action(cx.listener(Self::close_menu))
            .on_action(cx.listener(Self::open_review))
            .on_action(cx.listener(Self::confirm_review))
            .on_action(cx.listener(Self::stop_review))
            .on_action(cx.listener(Self::toggle_library))
            .on_action(cx.listener(Self::search_library))
            .on_action(cx.listener(Self::show_frontmatter_problems))
            .on_action(cx.listener(Self::open_preferences))
            .on_action(cx.listener(Self::show_spend))
            .on_action(cx.listener(Self::toggle_reading))
            .on_action(cx.listener(|this, _: &OpenFileMenu, window, cx| {
                this.toggle_menu(MenuKind::File, window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenEditMenu, window, cx| {
                this.toggle_menu(MenuKind::Edit, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &CloseHistory, window, cx| this.close_history(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &DotLeft, window, cx| this.step_dot("ArrowLeft", window, cx)),
            )
            .on_action(cx.listener(|this, _: &DotRight, window, cx| {
                this.step_dot("ArrowRight", window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &DotHome, window, cx| this.step_dot("Home", window, cx)),
            )
            .on_action(cx.listener(|this, _: &DotEnd, window, cx| this.step_dot("End", window, cx)))
            .on_action(cx.listener(|this, _: &AcceptAll, window, cx| {
                this.open_menu = None;
                this.accept_all(window, cx);
            }))
            .on_action(cx.listener(|this, _: &RejectAll, window, cx| {
                this.open_menu = None;
                this.reject_all(window, cx);
            }))
    }

    fn accept_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_editing(window, cx);
        self.pane.update(cx, |pane, cx| pane.accept_all(window, cx));
    }

    fn reject_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_editing(window, cx);
        self.pane.update(cx, |pane, cx| pane.reject_all(window, cx));
    }

    fn focus_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.reading() {
            window.focus(&self.focus, cx);
        } else {
            self.pane.update(cx, |pane, cx| pane.focus(window, cx));
        }
    }

    fn reading_toggle_allowed(&self, window: &Window, cx: &App) -> bool {
        !self.library.read(cx).search_focused(window, cx)
            && !self
                .frontmatter
                .focus_handle(cx)
                .contains_focused(window, cx)
            && !self.pane.read(cx).typing_outside_text(window, cx)
    }

    fn toggle_reading(&mut self, _: &ToggleReading, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal_open()
            || self.preferences.is_some()
            || self.review_focus.is_some()
            || self.evals.read(cx).dialog().is_some()
            || !self.reading_toggle_allowed(window, cx)
        {
            return;
        }
        let next = if self.reading() {
            Page::Text
        } else {
            Page::Reading
        };
        self.open_menu = None;
        self.spend = None;
        self.show_editing(window, cx);
        self.page = next;
        if next == Page::Reading {
            self.pane.update(cx, |pane, cx| pane.put_away(window, cx));
            self.focus.focus(window, cx);
        } else {
            let editor = self.pane.read(cx).editor.clone();
            editor.update(cx, |editor, cx| editor.focus(window, cx));
        }
        cx.notify();
    }

    fn render_main(
        &mut self,
        mode: HistoryMode,
        evals_shown: bool,
        evals_button: Option<EvalsButton>,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Div {
        let header = self.render_header(roles, cx);
        let conflict = self
            .conflict
            .clone()
            .map(|conflict| self.render_conflict(&conflict, roles, cx));
        let timeline = self.render_timeline(mode, evals_button, roles, cx);
        let version_bar = self.render_version_bar(mode, roles, cx);
        let content: AnyElement = match mode {
            _ if self.spend.is_some() => self.spend.clone().map_or_else(
                || div().into_any_element(),
                gpui_kit::IntoElement::into_any_element,
            ),
            _ if evals_shown => self.evals.clone().into_any_element(),
            HistoryMode::Editing if self.reading() => crate::reading_view::render_reading(
                &self.text(cx),
                roles,
                crate::theme::reading_code(cx),
            ),
            HistoryMode::Editing => self.render_editing(cx),
            HistoryMode::Version { id } => self.render_compare(id, roles, cx).into_any_element(),
            HistoryMode::Full => self.render_full_history(roles, cx).into_any_element(),
        };
        let placement = self.placement();
        div()
            .flex_1()
            .min_h_0()
            .w_full()
            .relative()
            .flex()
            .flex_row()
            .child(self.render_rail(roles, cx))
            .when(
                self.library_shown() && placement == Placement::Column,
                |this| this.child(self.render_library_column()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
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
                    .when(!self.key_present, |this| {
                        this.child(self.render_missing_key(roles, cx))
                    }),
            )
            .when(
                self.library_shown() && placement == Placement::Overlay,
                |this| {
                    this.child(
                        div()
                            .id("library-overlay")
                            .occlude()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(px(library_panel::RAIL_WIDTH))
                            .w(px(library_panel::LIBRARY_WIDTH))
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                if this.library_open && this.placement() == Placement::Overlay {
                                    this.fold_library(cx);
                                }
                            }))
                            .child(self.library_in()),
                    )
                },
            )
    }

    fn render_library_column(&self) -> AnyElement {
        let column = div().h_full().flex_none().overflow_hidden().child(
            div()
                .w(px(library_panel::LIBRARY_WIDTH))
                .h_full()
                .child(self.library_in()),
        );
        if self.library_open && self.leaving.library_reopened {
            return column
                .w(px(library_panel::LIBRARY_WIDTH))
                .into_any_element();
        }
        ui::column_motion(
            if self.library_open {
                "library-column-in"
            } else {
                "library-column-out"
            },
            column,
            px(library_panel::LIBRARY_WIDTH),
            self.library_open,
        )
        .into_any_element()
    }

    fn library_shown(&self) -> bool {
        self.library_open || self.leaving.library.is_some()
    }

    fn library_in(&self) -> AnimationElement<Div> {
        let panel = div().relative().size_full().child(self.library.clone());
        if self.library_open && self.leaving.library_reopened {
            return ui::enter(
                "library-back",
                panel,
                ui::Enter::Slide(0.),
                std::time::Duration::from_millis(1),
            );
        }
        if self.library_open {
            ui::enter(
                "library-in",
                panel,
                ui::Enter::Slide(-16.),
                ui::MOTION_ENTER,
            )
        } else {
            ui::leave(
                "library-out",
                panel.child(ui::input_cover("library-cover")),
                ui::Enter::Slide(-16.),
            )
        }
    }

    fn render_header(&self, roles: Roles, cx: &mut Context<Self>) -> Div {
        let label = self
            .file_label(cx)
            .map_or_else(|| self.file_name(), SharedString::from);
        let dirty = self.session.as_ref().is_some_and(|session| session.dirty);
        let review = self.pane.read(cx).review.as_ref().map(|run| run.added);
        let open = self.pane.read(cx).open_review_count();
        h_flex_row()
            .h(px(48.))
            .px(px(16.))
            .gap(px(12.))
            .bg(roles.raised)
            .child(ui::wordmark(roles))
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .type_body()
                    .text_color(roles.ink)
                    .child(format!("{label}{}", if dirty { " •" } else { "" })),
            )
            .child(div().flex_1())
            .child(
                ui::row()
                    .flex_none()
                    .gap(px(8.))
                    .type_small()
                    .text_color(roles.muted)
                    .child(if self.reading() {
                        "Reading. Edit with"
                    } else {
                        "Select text and press"
                    })
                    .child(ui::kbd(
                        if self.reading() {
                            "Ctrl+Alt+R"
                        } else {
                            "Ctrl+K"
                        },
                        roles,
                    )),
            )
            .when(open > 0, |this| {
                this.child(
                    ui::row()
                        .gap(px(8.))
                        .child(
                            ui::small_button(
                                "header-accept-all",
                                "Accept all",
                                ButtonKind::Primary,
                                roles,
                            )
                            .on_click(
                                cx.listener(|this, _, window, cx| this.accept_all(window, cx)),
                            ),
                        )
                        .child(
                            ui::small_button(
                                "header-reject-all",
                                "Reject all",
                                ButtonKind::Default,
                                roles,
                            )
                            .on_click(
                                cx.listener(|this, _, window, cx| this.reject_all(window, cx)),
                            ),
                        ),
                )
            })
            .when_some(review, |this, added| {
                this.child(self.render_reviewing(added, roles, cx))
            })
    }

    fn render_reviewing(
        &self,
        added: usize,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        ui::chip("header-reviewing", roles)
            .tooltip(|window, cx| {
                gpui_kit::component::tooltip::Tooltip::new("Stop the Review pass").build(window, cx)
            })
            .on_click(cx.listener(|this, _, window, cx| this.stop_review(&StopReview, window, cx)))
            .child(div().size(px(8.)).bg(roles.ai))
            .child("Reviewing…")
            .when(added > 0, |this| {
                this.child(div().text_color(roles.ai).child(format!(
                    "{added} {}",
                    if added == 1 {
                        "suggestion"
                    } else {
                        "suggestions"
                    }
                )))
            })
            .child(div().ml(px(4.)).weight_strong().child("Stop"))
    }

    fn with_overlays(
        &self,
        root: Stateful<Div>,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let toast = self.render_toast(roles, cx);
        let preferences = self
            .preferences
            .clone()
            .map(|panel| (panel, true))
            .or_else(|| {
                self.leaving
                    .preferences
                    .as_ref()
                    .map(|(panel, _)| (panel.clone(), false))
            });
        root.when_some(preferences, |this, (preferences, open)| {
            this.child(
                div()
                    .absolute()
                    .top(component::TITLE_BAR_HEIGHT)
                    .right_0()
                    .bottom_0()
                    .w(px(PREFERENCES_WIDTH))
                    .max_w_full()
                    .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                        if !this.modal_open() && this.preferences.is_some() {
                            this.close_preferences(cx);
                            this.focus_text(window, cx);
                            cx.notify();
                        }
                    }))
                    .child(preferences_motion(preferences, open)),
            )
        })
        .children(toast)
        .when_some(self.model_dialog.clone(), gpui_kit::ParentElement::child)
        .when_some(self.key_dialog.clone(), gpui_kit::ParentElement::child)
        .when_some(self.new_skill.clone(), gpui_kit::ParentElement::child)
        .when_some(self.evals.read(cx).dialog(), |this, dialog| {
            this.child(dialog)
        })
        .when(self.leave.is_some(), |this| {
            this.child(self.render_leave(roles, cx))
        })
        .when_some(self.review_focus.clone(), |this, input| {
            this.child(self.render_review_popover(&input, roles, cx))
        })
        .when_some(self.open_menu, |this, kind| {
            this.child(
                ui::scrim("menu-scrim").on_click(cx.listener(|this, _, _, cx| {
                    this.open_menu = None;
                    cx.notify();
                })),
            )
            .child(self.render_menu(kind, roles, cx))
        })
    }
}

#[cfg(test)]
mod frontmatter_scroll_tests {
    use super::frontmatter_scroll;
    use gpui_kit::px;

    #[test]
    fn scrolling_down_hides_the_panel_before_the_editor_moves() {
        assert_eq!(
            frontmatter_scroll(px(0.), px(300.), px(-120.), true),
            px(120.)
        );
        assert_eq!(
            frontmatter_scroll(px(250.), px(300.), px(-120.), true),
            px(300.)
        );
        assert_eq!(
            frontmatter_scroll(px(300.), px(300.), px(-120.), true),
            px(300.)
        );
    }

    #[test]
    fn scrolling_up_brings_the_panel_back_only_once_the_editor_is_at_the_top() {
        assert_eq!(
            frontmatter_scroll(px(300.), px(300.), px(120.), false),
            px(300.)
        );
        assert_eq!(
            frontmatter_scroll(px(300.), px(300.), px(120.), true),
            px(180.)
        );
        assert_eq!(
            frontmatter_scroll(px(60.), px(300.), px(120.), true),
            px(0.)
        );
    }
}
