//! The spell checker in the editor: dots under misspelled words and grammar mistakes, found
//! in the background a moment after typing stops, and the menu under a word that fixes it.

use std::{
    collections::HashSet,
    ops::Range,
    path::{Path, PathBuf},
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use desafinado_ai::editor::popover_place::{
    BubbleEnds, PopoverInput, Side, bubble_anchor, place_popover,
};
use desafinado_core::{
    blocks::{BlockKind, extract_blocks},
    log_err::LogErr,
};
use desafinado_features::spelling::{
    Issue, IssueKind, Speller,
    document::{self, Checked, WORDBOOK_FILE_NAME, Wordbook},
    pass::{self, Outcome, Plan, Progress},
    settings::{MenuKey, Press, SPELLING_FILE_NAME, SpellSettings},
};
use desafinado_platform::files::write_atomic_unconditionally;
use futures::StreamExt as _;
use gpui_kit::{prelude::FluentBuilder as _, *};

use crate::{
    editor_pane::{EditorPane, PaneEvent, area_of, hits_text_range, to_f64},
    theme::Roles,
    ui::{self, ToastTone},
};

actions!(
    spelling,
    [
        SpellMenuCtrlPeriod,
        SpellMenuAltEnter,
        NextIssue,
        PreviousIssue,
        SpellUp,
        SpellDown,
        SpellConfirm,
        SpellCancel,
    ]
);

/// How long typing must pause before the changed Blocks are checked.
const PAUSE: Duration = Duration::from_millis(400);
/// The least time between two redraws of the marks while a long check goes on.
const PUBLISH_EVERY: Duration = Duration::from_millis(100);
const MENU_KEY: &str = "\u{0}spell";

static SPELLER: LazyLock<Option<Mutex<Speller>>> =
    LazyLock::new(|| Speller::new().log_err().map(Mutex::new));
static SPELLER_FAILURE_SHOWN: AtomicBool = AtomicBool::new(false);
static EMPTY_WORDBOOK: LazyLock<Wordbook> = LazyLock::new(Wordbook::default);
/// The latest save of the dictionary of added words, and the lock that keeps saves in order.
static WORDBOOK_SAVE: AtomicU64 = AtomicU64::new(0);
static WORDBOOK_WRITE: Mutex<()> = Mutex::new(());

/// The writer's own words, shared by every window, and the file they are saved in.
pub struct WordbookChoice {
    pub wordbook: Wordbook,
    pub file: PathBuf,
}

impl Global for WordbookChoice {}

/// Whether the spell checker runs, and the key that opens its menu; saved in user data.
pub struct SpellChoice {
    pub settings: SpellSettings,
    pub file: PathBuf,
}

impl Global for SpellChoice {}

pub fn init(user_data: &Path, cx: &mut App) {
    let file = user_data.join(SPELLING_FILE_NAME);
    let settings = match std::fs::read_to_string(&file) {
        Ok(text) => SpellSettings::parse(&text).log_warn().unwrap_or_default(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => SpellSettings::default(),
        Err(error) => {
            log::warn!("The spelling settings could not be read: {error}");
            SpellSettings::default()
        }
    };
    cx.set_global(SpellChoice { settings, file });
    let file = user_data.join(WORDBOOK_FILE_NAME);
    let wordbook = match std::fs::read_to_string(&file) {
        Ok(text) => Wordbook::parse(&text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Wordbook::default(),
        Err(error) => {
            log::warn!("The dictionary of added words could not be read: {error}");
            Wordbook::default()
        }
    };
    cx.set_global(WordbookChoice { wordbook, file });
    cx.bind_keys([
        KeyBinding::new("ctrl-.", SpellMenuCtrlPeriod, Some("EditorPane > Input")),
        KeyBinding::new("alt-enter", SpellMenuAltEnter, Some("EditorPane > Input")),
        KeyBinding::new("f8", NextIssue, Some("EditorPane")),
        KeyBinding::new("shift-f8", PreviousIssue, Some("EditorPane")),
        KeyBinding::new("up", SpellUp, Some("SpellMenu")),
        KeyBinding::new("down", SpellDown, Some("SpellMenu")),
        KeyBinding::new("enter", SpellConfirm, Some("SpellMenu")),
        KeyBinding::new("escape", SpellCancel, Some("SpellMenu")),
    ]);
}

/// Loads the dictionaries in the background so the first check does not wait for them.
pub fn preload(cx: &App) {
    if cx.try_global::<SpellChoice>().is_none() || !settings(cx).enabled {
        return;
    }
    cx.background_spawn(async {
        LazyLock::force(&SPELLER);
    })
    .detach();
}

pub fn settings(cx: &App) -> SpellSettings {
    cx.try_global::<SpellChoice>()
        .map(|choice| choice.settings)
        .unwrap_or_default()
}

/// Stores the choice and redraws; returns the write to run off the UI thread.
pub fn choose(
    settings: SpellSettings,
    cx: &mut App,
) -> impl FnOnce() -> std::io::Result<()> + Send + 'static {
    let file = cx.global::<SpellChoice>().file.clone();
    cx.set_global(SpellChoice {
        settings,
        file: file.clone(),
    });
    cx.refresh_windows();
    let text = settings.to_json();
    move || write_atomic_unconditionally(&file, &text)
}

fn wordbook(cx: &App) -> &Wordbook {
    cx.try_global::<WordbookChoice>()
        .map_or(&*EMPTY_WORDBOOK, |choice| &choice.wordbook)
}

/// The spell checker's state in one editor.
pub struct SpellState {
    checked: Arc<Mutex<Checked>>,
    /// Counts the checks started; a check whose number is no longer the latest stops.
    generation: Arc<AtomicU64>,
    /// Where a running check works first: the line on screen, kept up to date by each render.
    anchor: Arc<AtomicUsize>,
    issues: Vec<Issue>,
    ignored: HashSet<String>,
    hovered: Option<Range<usize>>,
    menu: Option<SpellMenu>,
    pub(crate) focus: FocusHandle,
    /// Whether the checker was on at the last render, to notice Preferences turning it.
    enabled: bool,
    /// The mark whose menu was just closed while the caret stayed on it: F8 moves on from it.
    dismissed: Option<Range<usize>>,
    _task: Option<Task<()>>,
}

#[derive(Clone, Debug)]
struct SpellMenu {
    issue: Issue,
    active: usize,
    /// Opened from the keyboard: the menu takes the keys. A click leaves them in the text.
    focused: bool,
}

/// A choice in the menu, in the order it shows.
#[derive(Clone, Debug, PartialEq)]
enum MenuChoice {
    Replace(String),
    AddToDictionary,
    Ignore,
}

impl SpellState {
    pub fn new(cx: &mut App) -> Self {
        Self {
            checked: Arc::new(Mutex::new(Checked::new())),
            generation: Arc::new(AtomicU64::new(0)),
            anchor: Arc::new(AtomicUsize::new(0)),
            issues: Vec::new(),
            ignored: HashSet::new(),
            hovered: None,
            menu: None,
            focus: cx.focus_handle(),
            enabled: settings(cx).enabled,
            dismissed: None,
            _task: None,
        }
    }
}

fn choices(issue: &Issue) -> Vec<MenuChoice> {
    issue
        .replacements
        .iter()
        .cloned()
        .map(MenuChoice::Replace)
        .chain((issue.kind == IssueKind::Spelling).then_some(MenuChoice::AddToDictionary))
        .chain([MenuChoice::Ignore])
        .collect()
}

fn choice_label(choice: &MenuChoice) -> String {
    match choice {
        MenuChoice::Replace(text) if text.is_empty() => "Remove".into(),
        MenuChoice::Replace(text) => text.clone(),
        MenuChoice::AddToDictionary => "Add to dictionary".into(),
        MenuChoice::Ignore => "Ignore".into(),
    }
}

fn checkable(kind: BlockKind) -> bool {
    !matches!(kind, BlockKind::Code | BlockKind::Html | BlockKind::Rule)
}

enum Finished {
    Complete,
    Superseded,
    NoSpeller,
}

/// Checks the Blocks of `text` that changed since the last check, a chunk at a time, and
/// sends the issues found so far on `progress`. Runs off the UI thread.
fn check_document(
    text: &str,
    checked: &Mutex<Checked>,
    anchor: &(dyn Fn() -> usize + Sync),
    live: &(dyn Fn() -> bool + Sync),
    publish: &(dyn Fn(Progress) + Sync),
) -> Finished {
    perf_span!("spelling.check_document");
    let blocks: Vec<(usize, String)> = extract_blocks(text)
        .into_iter()
        .filter(|block| checkable(block.kind))
        .map(|block| (block.from, block.text))
        .collect();
    let Some(speller) = SPELLER.as_ref() else {
        return Finished::NoSpeller;
    };
    let plan = Plan {
        anchor,
        throttle: PUBLISH_EVERY,
        live,
        publish,
    };
    match pass::run(&blocks, checked, speller, &plan) {
        Outcome::Complete => Finished::Complete,
        Outcome::Superseded => Finished::Superseded,
    }
}

impl EditorPane {
    /// Checks the Document again, at once or after a pause in typing. A check still running
    /// stops at its next chunk.
    pub(crate) fn check_spelling(&mut self, pause: bool, cx: &mut Context<Self>) {
        let generation = self.spelling.generation.fetch_add(1, Ordering::SeqCst) + 1;
        if !settings(cx).enabled {
            self.spelling._task = None;
            return;
        }
        let checked = Arc::clone(&self.spelling.checked);
        let latest = Arc::clone(&self.spelling.generation);
        let anchor = Arc::clone(&self.spelling.anchor);
        self.spelling._task = Some(cx.spawn(async move |this, cx| {
            if pause {
                cx.background_executor().timer(PAUSE).await;
            }
            let Some(text) = this
                .update(cx, |this, cx| {
                    this.follow_view(cx);
                    this.text(cx)
                })
                .log_err()
            else {
                return;
            };
            let (sender, mut receiver) = futures::channel::mpsc::unbounded::<Progress>();
            let work = cx.background_spawn(async move {
                let live = || latest.load(Ordering::SeqCst) == generation;
                let publish = |progress| {
                    sender.unbounded_send(progress).log_warn();
                };
                check_document(
                    &text,
                    &checked,
                    &|| anchor.load(Ordering::Relaxed),
                    &live,
                    &publish,
                )
            });
            let mut next = {
                perf_span!("spelling.first_marks");
                receiver.next().await
            };
            while let Some(mut progress) = next {
                while let Ok(newer) = receiver.try_recv() {
                    progress = newer;
                }
                this.update(cx, |this, cx| this.show_progress(progress, generation, cx))
                    .log_err();
                next = receiver.next().await;
            }
            if matches!(work.await, Finished::NoSpeller) {
                this.update(cx, |_, cx| {
                    if !SPELLER_FAILURE_SHOWN.swap(true, Ordering::Relaxed) {
                        cx.emit(PaneEvent::Toast {
                            text: "The spell checker could not load its dictionaries.".into(),
                            tone: ToastTone::Danger,
                            actions: Vec::new(),
                        });
                    }
                })
                .log_err();
            }
        }));
    }

    /// Shows what a check found so far, unless a newer check has begun since.
    fn show_progress(&mut self, progress: Progress, generation: u64, cx: &mut Context<Self>) {
        if self.spelling.generation.load(Ordering::SeqCst) != generation {
            return;
        }
        self.spelling.issues =
            document::merge_progress(&self.spelling.issues, &progress.pending, progress.issues);
        cx.notify();
    }

    /// Points a running check at the first line on screen, or at the caret before the first
    /// layout.
    pub(crate) fn follow_view(&self, cx: &App) {
        let state = self.editor.read(cx);
        let offset = state.visible_row_range().map_or_else(
            || state.selected_range().start,
            |rows| component::RopeExt::line_start_offset(state.text(), rows.start),
        );
        self.spelling.anchor.store(offset, Ordering::Relaxed);
    }

    /// Moves the marks with an edit; the ones it touched wait for the next check.
    pub(crate) fn map_spelling(&mut self, start: usize, old_end: usize, new_end: usize) {
        self.spelling.issues = document::map_issues(&self.spelling.issues, start, old_end, new_end);
        self.spelling.menu = None;
        self.spelling.dismissed = None;
    }

    pub(crate) fn clear_spelling(&mut self) {
        self.spelling.issues.clear();
        self.spelling.menu = None;
        self.spelling.hovered = None;
    }

    /// The marks to show: not on the writer's own words or ignored ones, nor under a Suggestion.
    fn shown_issues(&self, cx: &App) -> Vec<Issue> {
        let covered: Vec<Range<usize>> = self
            .suggestions
            .items
            .iter()
            .map(|item| item.from..item.to)
            .collect();
        let text = self.editor.read(cx).text();
        document::shown(
            &self.spelling.issues,
            |range| words_in(text, range),
            wordbook(cx),
            &self.spelling.ignored,
            &covered,
        )
    }

    pub(crate) fn spell_marks(&self, roles: Roles, cx: &App) -> Vec<ui::SpellMark> {
        let open = self
            .spelling
            .menu
            .as_ref()
            .map(|menu| menu.issue.range.clone());
        self.shown_issues(cx)
            .into_iter()
            .map(|issue| {
                let lit = self.spelling.hovered.as_ref() == Some(&issue.range)
                    || open.as_ref() == Some(&issue.range);
                let color = match (lit, issue.kind) {
                    (false, _) => roles.faint,
                    (true, IssueKind::Spelling) => roles.danger,
                    (true, IssueKind::Grammar) => roles.accent,
                };
                ui::SpellMark {
                    range: issue.range,
                    color,
                }
            })
            .collect()
    }

    fn issue_under(&self, position: Point<Pixels>, cx: &App) -> Option<Issue> {
        if self.over_card(position) {
            return None;
        }
        let state = self.editor.read(cx);
        let column = self.content_left()..self.content_right();
        self.shown_issues(cx)
            .into_iter()
            .filter(|issue| {
                let (Some(from), Some(to)) = (
                    state.range_to_bounds(&(issue.range.start..issue.range.start)),
                    state.range_to_bounds(&(issue.range.end..issue.range.end)),
                ) else {
                    return false;
                };
                hits_text_range(position, from, to, column.clone())
            })
            .min_by_key(|issue| issue.range.len())
    }

    /// Lights the mark under the pointer. Returns whether it changed.
    pub(crate) fn hover_spelling(&mut self, position: Option<Point<Pixels>>, cx: &App) -> bool {
        let next = position
            .and_then(|position| self.issue_under(position, cx))
            .map(|issue| issue.range);
        let changed = next != self.spelling.hovered;
        self.spelling.hovered = next;
        changed
    }

    /// The dots under the text and the open menu, if any.
    /// A menu whose word scrolled out of view closes, and the keys go back to the text.
    pub(crate) fn render_spelling(
        &mut self,
        window: &mut Window,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> (AnyElement, Option<AnyElement>) {
        self.follow_spelling_switch(cx);
        self.follow_view(cx);
        let layer = ui::spelling_layer(self.editor.clone(), self.spell_marks(roles, cx));
        let menu = self.render_spell_menu(roles, cx);
        if menu.is_none() {
            self.spelling.menu = None;
            if self.spelling.focus.is_focused(window) {
                self.focus(window, cx);
            }
        }
        (layer, menu)
    }

    /// Clears the marks when Preferences turn the checker off, and checks again when on.
    fn follow_spelling_switch(&mut self, cx: &mut Context<Self>) {
        let enabled = settings(cx).enabled;
        if enabled == self.spelling.enabled {
            return;
        }
        self.spelling.enabled = enabled;
        if enabled {
            self.check_spelling(false, cx);
        } else {
            self.clear_spelling();
            self.spelling.generation.fetch_add(1, Ordering::SeqCst);
            self.spelling._task = None;
        }
    }

    /// A menu opened by a click closes once the caret leaves its word or selects text.
    pub(crate) fn follow_caret_with_spell_menu(&mut self, selection: &Range<usize>) {
        let stays = self.spelling.menu.as_ref().is_none_or(|menu| {
            menu.focused
                || (selection.is_empty()
                    && menu.issue.range.start <= selection.start
                    && selection.start <= menu.issue.range.end)
        });
        if !stays {
            self.spelling.menu = None;
        }
        let on_dismissed = self.spelling.dismissed.as_ref().is_some_and(|range| {
            selection.is_empty() && range.start <= selection.start && selection.start <= range.end
        });
        if !on_dismissed {
            self.spelling.dismissed = None;
        }
    }

    /// A click on a marked word, with nothing selected, opens its menu.
    pub(crate) fn spell_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let position = event.position;
        let selection = self.editor.read(cx).selected_range();
        if !selection.is_empty() {
            return;
        }
        if let Some(issue) = self.issue_under(position, cx).filter(|issue| {
            issue.range.start <= selection.start && selection.start <= issue.range.end
        }) {
            self.open_spell_menu_on(issue, false, window, cx);
        }
    }

    fn open_spell_menu_on(
        &mut self,
        issue: Issue,
        focused: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.spelling.menu = Some(SpellMenu {
            issue,
            active: 0,
            focused,
        });
        if focused {
            window.focus(&self.spelling.focus, cx);
        }
        cx.notify();
    }

    /// "?" opens the menu of the mark under the caret when Preferences chose it. When it
    /// reaches no mark, it types itself.
    pub(crate) fn spell_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        let press = Press {
            key: &keystroke.key,
            key_char: keystroke.key_char.as_deref(),
            control: keystroke.modifiers.control,
            alt: keystroke.modifiers.alt,
        };
        if MenuKey::Question.matches(press)
            && self.open_spell_menu_by(MenuKey::Question, window, cx)
        {
            cx.stop_propagation();
        }
    }

    /// `Ctrl+.` and `Alt+Enter` are key bindings, so they win over the text field's own; when
    /// they are not the chosen key or reach no mark, the text field gets them.
    pub(crate) fn spell_menu_ctrl_period(
        &mut self,
        _: &SpellMenuCtrlPeriod,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.spell_menu_binding(MenuKey::CtrlPeriod, window, cx);
    }

    pub(crate) fn spell_menu_alt_enter(
        &mut self,
        _: &SpellMenuAltEnter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.spell_menu_binding(MenuKey::AltEnter, window, cx);
    }

    fn spell_menu_binding(&mut self, key: MenuKey, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open_spell_menu_by(key, window, cx) {
            cx.propagate();
        }
    }

    /// Opens the menu of the mark under the caret if `key` is the chosen one and reaches it.
    fn open_spell_menu_by(
        &mut self,
        key: MenuKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let chosen = settings(cx);
        if !chosen.enabled || chosen.menu_key != key || self.spelling.menu.is_some() {
            return false;
        }
        let selection = self.editor.read(cx).selected_range();
        let issues = self.shown_issues(cx);
        let Some(index) = document::issue_to_open(&issues, &selection, key.strictly_inside())
        else {
            return false;
        };
        self.open_spell_menu_on(issues[index].clone(), true, window, cx);
        true
    }

    /// Opens the menu of the mark under the caret, or moves to the next or previous mark and
    /// opens its menu.
    pub(crate) fn step_issue(
        &mut self,
        backward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !settings(cx).enabled {
            return;
        }
        let issues = self.shown_issues(cx);
        if self.spelling.menu.is_none()
            && let Some(index) =
                document::issue_to_open(&issues, &self.editor.read(cx).selected_range(), false)
                    .filter(|&index| self.spelling.dismissed.as_ref() != Some(&issues[index].range))
        {
            self.open_spell_menu_on(issues[index].clone(), true, window, cx);
            return;
        }
        let from = match (&self.spelling.menu, &self.spelling.dismissed) {
            (Some(menu), _) => menu.issue.range.start,
            (None, Some(dismissed)) => dismissed.start,
            (None, None) => self.editor.read(cx).cursor(),
        };
        let Some(index) = document::step_issue(&issues, from, backward) else {
            return;
        };
        let issue = issues[index].clone();
        self.place_caret(issue.range.start, window, cx);
        self.open_spell_menu_on(issue, true, window, cx);
    }

    /// Returns whether a menu was open.
    pub(crate) fn close_spell_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let closed = self.spelling.menu.take();
        let open = closed.is_some();
        self.spelling.dismissed = closed.map(|menu| menu.issue.range);
        if open {
            if self.spelling.focus.is_focused(window) {
                self.focus(window, cx);
            }
            cx.notify();
        }
        open
    }

    pub(crate) fn spell_menu_blurred(&mut self, cx: &mut Context<Self>) {
        if self.spelling.menu.take().is_some() {
            cx.notify();
        }
    }

    fn spell_move(&mut self, step: isize, cx: &mut Context<Self>) {
        if let Some(menu) = self.spelling.menu.as_mut() {
            let count = choices(&menu.issue).len() as isize;
            menu.active = (menu.active as isize + step).rem_euclid(count) as usize;
            cx.notify();
        }
    }

    fn spell_up(&mut self, _: &SpellUp, _: &mut Window, cx: &mut Context<Self>) {
        self.spell_move(-1, cx);
    }

    fn spell_down(&mut self, _: &SpellDown, _: &mut Window, cx: &mut Context<Self>) {
        self.spell_move(1, cx);
    }

    fn spell_confirm(&mut self, _: &SpellConfirm, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = self.spelling.menu.clone() {
            self.choose(menu.active, window, cx);
        }
    }

    fn spell_cancel(&mut self, _: &SpellCancel, window: &mut Window, cx: &mut Context<Self>) {
        self.close_spell_menu(window, cx);
    }

    fn choose(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.spelling.menu.clone() else {
            return;
        };
        let Some(choice) = choices(&menu.issue).into_iter().nth(index) else {
            return;
        };
        self.close_spell_menu(window, cx);
        let words = words_in(self.editor.read(cx).text(), &menu.issue.range);
        match choice {
            MenuChoice::Replace(text) => self.edit_range(menu.issue.range, &text, window, cx),
            MenuChoice::AddToDictionary => add_to_wordbook(&words, cx),
            MenuChoice::Ignore => {
                self.spelling
                    .ignored
                    .insert(document::ignore_key(&menu.issue, &words));
            }
        }
        cx.notify();
    }

    fn render_spell_menu(&self, roles: Roles, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.spelling.menu.as_ref()?;
        let placed = self.place_spell_menu(&menu.issue.range, cx)?;
        let heights = std::rc::Rc::clone(&self.heights);
        let entity = cx.entity().downgrade();
        let spelling = menu.issue.kind == IssueKind::Spelling;
        let active = menu.focused.then_some(menu.active);
        let rows: Vec<AnyElement> = choices(&menu.issue)
            .into_iter()
            .enumerate()
            .map(|(index, choice)| self.render_spell_row(index, &choice, active, roles, cx))
            .collect();
        let first_quiet = menu.issue.replacements.len();
        let (replacements, actions) = {
            let mut rows = rows;
            let actions = rows.split_off(first_quiet);
            (rows, actions)
        };
        Some(
            ui::spell_menu("spell-menu", roles)
                .key_context("SpellMenu")
                .track_focus(&self.spelling.focus)
                .on_action(cx.listener(Self::spell_up))
                .on_action(cx.listener(Self::spell_down))
                .on_action(cx.listener(Self::spell_confirm))
                .on_action(cx.listener(Self::spell_cancel))
                .absolute()
                .left(px(placed.left as f32))
                .top(px(placed.top as f32))
                .child(ui::measure(move |size, cx| {
                    let previous = heights
                        .borrow_mut()
                        .insert(MENU_KEY.to_string(), size.height);
                    if previous != Some(size.height)
                        && let Some(entity) = entity.upgrade()
                    {
                        entity.update(cx, |_, cx| cx.notify());
                    }
                }))
                .child(ui::spell_message(
                    spelling,
                    menu.issue.message.clone(),
                    roles,
                ))
                .children(replacements)
                .when(first_quiet > 0, |this| this.child(ui::spell_divider(roles)))
                .children(actions)
                .into_any_element(),
        )
    }

    fn render_spell_row(
        &self,
        index: usize,
        choice: &MenuChoice,
        active: Option<usize>,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let quiet = !matches!(choice, MenuChoice::Replace(_));
        let is_active = active == Some(index);
        ui::spell_row(("spell-row", index), is_active, quiet, roles)
            .on_click(cx.listener(move |this, _, window, cx| this.choose(index, window, cx)))
            .child(
                div()
                    .when(index == 0 && !quiet, ui::Type::weight_strong)
                    .child(choice_label(choice)),
            )
            .when(is_active, |this| this.child(ui::kbd("Enter", roles)))
            .into_any_element()
    }

    fn place_spell_menu(
        &self,
        range: &Range<usize>,
        cx: &App,
    ) -> Option<desafinado_ai::editor::popover_place::Placed> {
        let ends = BubbleEnds {
            head: self.line_at(range.start, cx)?,
            start: self.line_at(range.start, cx)?,
            end: self.line_at(range.end, cx)?,
        };
        let pane = self.bounds.get();
        let area = area_of(pane);
        let column_left = to_f64(self.content_left() - pane.origin.x);
        let height = self
            .heights
            .borrow()
            .get(MENU_KEY)
            .copied()
            .unwrap_or(px(160.));
        place_popover(&PopoverInput {
            anchor: bubble_anchor(ends, column_left, &area),
            width: to_f64(ui::SPELL_MENU_WIDTH),
            height: to_f64(height),
            area,
            side: Side::Below,
            flip: true,
            gap: 4.,
        })
    }
}

/// The text a mark covers; empty when an edit has moved the text out from under it.
fn words_in(text: &gpui_kit::base::input::Rope, range: &Range<usize>) -> String {
    text_in(text, range).unwrap_or_default()
}

/// The text in `range`, or `None` when the range is not in the text. Ropey's `try_slice`
/// still panics on an end past the text, so the bounds are checked here first.
pub(crate) fn text_in(text: &gpui_kit::base::input::Rope, range: &Range<usize>) -> Option<String> {
    if range.start > range.end || range.end > text.len() {
        return None;
    }
    text.try_slice(range.clone())
        .ok()
        .map(|slice| slice.to_string())
}

fn add_to_wordbook(word: &str, cx: &mut Context<EditorPane>) {
    let Some(choice) = cx.try_global::<WordbookChoice>() else {
        return;
    };
    let wordbook = choice.wordbook.with(word);
    let file = choice.file.clone();
    let text = wordbook.to_text();
    cx.set_global(WordbookChoice {
        wordbook,
        file: file.clone(),
    });
    let save = WORDBOOK_SAVE.fetch_add(1, Ordering::SeqCst) + 1;
    cx.spawn(async move |this, cx| {
        let saved = cx
            .background_spawn(async move { save_wordbook(save, &file, &text) })
            .await;
        if let Err(error) = saved {
            log::error!("The dictionary of added words could not be saved: {error}");
            this.update(cx, |_, cx| {
                cx.emit(PaneEvent::Toast {
                    text: format!("The word could not be saved to the dictionary: {error}"),
                    tone: ToastTone::Danger,
                    actions: Vec::new(),
                });
            })
            .log_err();
        }
    })
    .detach();
    cx.refresh_windows();
}

/// Writes the dictionary unless a later save has its newer text: saves end in the order made.
fn save_wordbook(save: u64, file: &Path, text: &str) -> std::io::Result<()> {
    let _turn = WORDBOOK_WRITE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if WORDBOOK_SAVE.load(Ordering::SeqCst) != save {
        return Ok(());
    }
    write_atomic_unconditionally(file, text)
}

#[cfg(test)]
mod tests {
    use super::{
        BlockKind, Checked, Finished, Issue, IssueKind, MenuChoice, Mutex, Progress,
        check_document, checkable, choice_label, choices, text_in,
    };

    #[test]
    fn a_mark_past_the_end_of_the_text_covers_nothing_instead_of_panicking() {
        let text = gpui_kit::base::input::Rope::from("Hans Lippershey");
        assert_eq!(text_in(&text, &(5..15)).as_deref(), Some("Lippershey"));
        assert_eq!(text_in(&text, &(5..20)), None);
        assert_eq!(text_in(&text, &(16..20)), None);
        let (start, end) = (9, 5);
        assert_eq!(text_in(&text, &(start..end)), None);
    }

    fn issue(kind: IssueKind, replacements: &[&str]) -> Issue {
        Issue {
            range: 0..3,
            kind,
            message: "m".into(),
            replacements: replacements.iter().map(|r| (*r).to_string()).collect(),
        }
    }

    #[test]
    fn spelling_offers_the_replacements_then_the_dictionary_then_ignore() {
        assert_eq!(
            choices(&issue(IssueKind::Spelling, &["the", "tech"])),
            vec![
                MenuChoice::Replace("the".into()),
                MenuChoice::Replace("tech".into()),
                MenuChoice::AddToDictionary,
                MenuChoice::Ignore,
            ]
        );
    }

    #[test]
    fn grammar_cannot_be_added_to_the_dictionary() {
        assert_eq!(
            choices(&issue(IssueKind::Grammar, &["have"])),
            vec![MenuChoice::Replace("have".into()), MenuChoice::Ignore]
        );
    }

    #[test]
    fn an_empty_replacement_reads_remove() {
        assert_eq!(choice_label(&MenuChoice::Replace(String::new())), "Remove");
        assert_eq!(choice_label(&MenuChoice::Replace("a lot".into())), "a lot");
    }

    #[test]
    fn code_html_and_rules_are_not_checked() {
        assert!(!checkable(BlockKind::Code));
        assert!(!checkable(BlockKind::Html));
        assert!(!checkable(BlockKind::Rule));
        assert!(checkable(BlockKind::Paragraph));
        assert!(checkable(BlockKind::Heading));
    }

    #[test]
    fn checks_only_prose_blocks_and_places_issues_in_the_document() {
        let text = "# Title\n\nI belive it.\n\n```\nbelive\n```\n";
        let checked = Mutex::new(Checked::new());
        let last = Mutex::new(None::<Progress>);
        let publish = |progress| *last.lock().expect("lock") = Some(progress);
        let finished = check_document(text, &checked, &|| 0, &|| true, &publish);
        assert!(matches!(finished, Finished::Complete));
        let issues = last.lock().expect("lock").take().expect("published").issues;
        let ranges: Vec<&str> = issues
            .iter()
            .map(|issue| &text[issue.range.clone()])
            .collect();
        assert_eq!(ranges, vec!["belive"]);
        assert_eq!(issues[0].range.start, text.find("belive").expect("word"));
        assert_eq!(checked.lock().expect("lock").len(), 2);
    }
}
