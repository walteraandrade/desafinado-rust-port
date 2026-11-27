use desafinado_core::log_err::LogErr;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    ops::Range,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use desafinado_ai::{
    editor::{
        ai_caret::ai_caret_of,
        ask_bubble::{
            BubbleChange, BubbleRange, BubbleState, BubbleView, bubble_visible, held_selection,
            next_bubble,
        },
        ask_menu::{self, AskMenu, AskRow},
        card_placement::{CardPlacement, MARGIN_CARD_WIDTH, margin_card_left, placement_for},
        caret_travel::Spot,
        focus_cycle::cycle_index,
        geometry::{Rect, Viewport},
        inline_diff::{block_replacement, inline_inserts, rows_before, zone_rows},
        popover_place::{
            BubbleEnds, LineAt, Placed, PopoverInput, Side, bubble_anchor, place_popover,
        },
        shown_cards::{CardKind, CardStatus, shown_cards},
    },
    edits::{MAX_REVIEW_EDITS, ReviewValidationContext},
    prompts::{
        RefinePromptInput, ReviewPromptInput, TargetPromptInput, build_refine_prompt,
        build_review_prompt, build_target_prompt,
    },
    quick_actions::{QuickAction, default_quick_actions},
};
use desafinado_core::{
    blocks::{
        Block, BlockKind, Selection, Target, extract_blocks, get_neighbors, resolve_target,
        target_from_range,
    },
    change_set::ChangeSet,
    diff::DiffHunk,
    suggestions::{
        ClientError, Direction, Instruction, PlaceReviewEditInput, ReadySuggestion, RefineState,
        ReviewPlacement, Suggestion, SuggestionOrigin, SuggestionsState, accept_all_plan,
        accept_suggestions, active_suggestion, begin_refine, begin_suggestion, cancel_refine,
        change_label, change_ranges, describe_error, empty_suggestions, fail_refine,
        fail_suggestion, find_suggestion, first_review_suggestion, instruction_history,
        instruction_label, map_suggestions, newly_failed, next_from, notice_message,
        occupied_block_ids, open_review_count, place_review_edit, refine_task_label,
        reject_review_suggestions, reject_suggestion, resolve_refine, resolve_suggestion,
        restore_suggestion, retry_refine, retry_suggestion, review_notice_text, review_summary,
        review_task_label, review_too_large, review_too_large_text, show_version, shows_changes,
        step_change, step_suggestion, suggestion_at, task_label,
    },
};
use futures::{StreamExt as _, channel::mpsc};
use gpui_kit::base::input::{BlockZone, Inlay, Rope};

use crate::rope_text::{edit_change, frontmatter_of};
use gpui_kit::{
    component::input::{
        Editor, EditorState, Input, InputEditsEvent, InputState, RangeDecoration,
        RangeDecorationCollection, RangeDecorationStyle, Redo, TextDecoration,
        TextDecorationCollection, Undo,
    },
    prelude::FluentBuilder as _,
    *,
};

use desafinado_runtime::ai_backend::{AiBackend, ReviewCall, ReviewEvent, TargetCall};

use crate::{
    caret::{self, CaretMarks},
    theme::{self, Roles},
    ui::{self, ButtonKind, ButtonSize, ToastTone, Type as _},
};

actions!(
    editor_pane,
    [
        AskAi,
        AcceptActive,
        EscapeAtCursor,
        PreviousVersion,
        NextVersion,
        NextSuggestion,
        PreviousSuggestion,
        FocusCard,
        FocusCardBack,
        PaneUndo,
        PaneRedo,
        BubbleUp,
        BubbleDown,
        BubbleConfirm,
        BubbleCancel,
        RefineConfirm,
        RefineCancel,
        CardNext,
        CardPrevious,
        CardPress,
        CardLeave,
        Stay,
    ]
);

const COLUMN_PADDING: f32 = 24.;
const BUBBLE_WIDTH: f32 = 360.;
const BUBBLE_KEY: &str = "\u{0}bubble";
/// Space above and below an element drawn in the rows reserved under a Suggestion.
const ZONE_TOP: f32 = 4.;
const ZONE_BOTTOM: f32 = 4.;
/// A thin space between struck text and the insertion that follows it.
const INLAY_GAP_TEXT: &str = "\u{2009}";

fn zone_key(id: &str) -> String {
    format!("\u{0}zone-{id}")
}

const EDITOR_NOT_REFINING: &str = "EditorPane > Input && !RefineInput";
const CARD_NOT_REFINING: &str = "SuggestionCard && !RefineInput";

pub fn init(cx: &mut App) {
    cx.bind_keys(key_bindings());
}

fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("ctrl-shift-z", Redo, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-y", Redo, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-z", Undo, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-shift-z", PaneRedo, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-y", PaneRedo, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-z", PaneUndo, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-k", AskAi, Some(EDITOR_NOT_REFINING)),
        KeyBinding::new("ctrl-enter", AcceptActive, Some(EDITOR_NOT_REFINING)),
        KeyBinding::new("escape", EscapeAtCursor, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-h", PreviousVersion, Some(EDITOR_NOT_REFINING)),
        KeyBinding::new("ctrl-l", NextVersion, Some(EDITOR_NOT_REFINING)),
        KeyBinding::new("alt-]", NextSuggestion, Some(EDITOR_NOT_REFINING)),
        KeyBinding::new("alt-[", PreviousSuggestion, Some(EDITOR_NOT_REFINING)),
        KeyBinding::new("alt-down", NextSuggestion, Some(EDITOR_NOT_REFINING)),
        KeyBinding::new("alt-up", PreviousSuggestion, Some(EDITOR_NOT_REFINING)),
        KeyBinding::new("tab", FocusCard, Some("EditorPane > Input")),
        KeyBinding::new("shift-tab", FocusCardBack, Some("EditorPane > Input")),
        KeyBinding::new("up", BubbleUp, Some("AskBubble > Input")),
        KeyBinding::new("down", BubbleDown, Some("AskBubble > Input")),
        KeyBinding::new("enter", BubbleConfirm, Some("AskBubble > Input")),
        KeyBinding::new("escape", BubbleCancel, Some("AskBubble > Input")),
        KeyBinding::new("tab", Stay, Some("AskBubble > Input")),
        KeyBinding::new("shift-tab", Stay, Some("AskBubble > Input")),
        KeyBinding::new("enter", RefineConfirm, Some("RefineInput > Input")),
        KeyBinding::new("escape", RefineCancel, Some("RefineInput > Input")),
        KeyBinding::new("tab", CardNext, Some("RefineInput > Input")),
        KeyBinding::new("shift-tab", CardPrevious, Some("RefineInput > Input")),
        KeyBinding::new("tab", CardNext, Some("SuggestionCard")),
        KeyBinding::new("shift-tab", CardPrevious, Some("SuggestionCard")),
        KeyBinding::new("right", CardNext, Some(CARD_NOT_REFINING)),
        KeyBinding::new("down", CardNext, Some(CARD_NOT_REFINING)),
        KeyBinding::new("left", CardPrevious, Some(CARD_NOT_REFINING)),
        KeyBinding::new("up", CardPrevious, Some(CARD_NOT_REFINING)),
        KeyBinding::new("enter", CardPress, Some(CARD_NOT_REFINING)),
        KeyBinding::new("space", CardPress, Some(CARD_NOT_REFINING)),
        KeyBinding::new("escape", CardLeave, Some("SuggestionCard")),
        KeyBinding::new("ctrl-h", PreviousVersion, Some(CARD_NOT_REFINING)),
        KeyBinding::new("ctrl-l", NextVersion, Some(CARD_NOT_REFINING)),
    ]
}

pub struct AcceptedChange {
    pub origin: SuggestionOrigin,
    pub instruction: String,
    pub rationale: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaneToastAction {
    RetrySuggestion(String),
    ShowSuggestion(String),
    RetryReview(String),
}

pub enum PaneEvent {
    Changed,
    Accepted {
        before: String,
        text: String,
        changes: Vec<AcceptedChange>,
    },
    Toast {
        text: String,
        tone: ToastTone,
        actions: Vec<PaneToastAction>,
    },
}

impl EventEmitter<PaneEvent> for EditorPane {}

struct RefineInput {
    id: String,
    input: Entity<InputState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardControl {
    PreviousVersion,
    NextVersion,
    PreviousChange,
    NextChange,
    Accept,
    Reject,
    Refine,
    RefineField,
    RetryRefine,
}

enum Applying {
    One(String),
    Many(Vec<String>, ChangeSet),
}

type CardRects = Rc<RefCell<Vec<(Bounds<Pixels>, CardKind)>>>;

#[derive(Clone)]
struct GhostRow {
    label: SharedString,
    skill: bool,
}

#[derive(Clone)]
struct BubbleGhost {
    left: Pixels,
    top: Pixels,
    height: Pixels,
    above: bool,
    query: SharedString,
    hint: Option<SharedString>,
    rows: Vec<GhostRow>,
    active: usize,
}

pub struct EditorPane {
    pub editor: Entity<EditorState>,
    pub suggestions: SuggestionsState,
    strike: TextDecorationCollection,
    fills: RangeDecorationCollection,
    pub(crate) caret: CaretMarks,
    _caret_blink: Task<()>,
    bubble_input: Entity<InputState>,
    bubble: BubbleState,
    menu: AskMenu,
    bubble_shown: bool,
    typing_in_bubble: bool,
    last_bubble: RefCell<Option<BubbleGhost>>,
    bubble_leaving: ui::Presence<BubbleGhost>,
    pointer_down: Rc<Cell<bool>>,
    pub(crate) last_selection: Range<usize>,
    refine: Option<RefineInput>,
    card_focus: FocusHandle,
    card_index: usize,
    applying: Option<Applying>,
    tasks: HashMap<String, Task<()>>,
    pub(crate) next_id: usize,
    pub(crate) next_review_id: usize,
    rejected: Vec<ReadySuggestion>,
    restored: Vec<ReadySuggestion>,
    pub quick_actions: Vec<QuickAction>,
    pub is_skill: bool,
    pub(crate) ai: Arc<dyn AiBackend>,
    pub(crate) bounds: Rc<Cell<Bounds<Pixels>>>,
    pub(crate) heights: Rc<RefCell<HashMap<String, Pixels>>>,
    card_rects: CardRects,
    /// The Suggestions whose Card the last frame drew.
    pub(crate) drawn_cards: RefCell<Vec<String>>,
    hovered: Option<String>,
    pub review: Option<ReviewRun>,
    pub(crate) pending: Option<crate::comment_review::PendingReview>,
    pub(crate) mode: Option<crate::comment_review::ReviewModeState>,
    pub notes: Option<desafinado_ai::notes::NoteBoard>,
    pub(crate) notes_scroll: ScrollHandle,
    /// Suggestions asked for from a Note, by Suggestion id, so accepting one resolves its Note.
    pub(crate) note_rewrites: Vec<(String, usize)>,
    reveal_pending: bool,
    pub(crate) text_map: crate::text_map_view::TextMapState,
    pub(crate) jobs: crate::jobs_tray::JobsState,
    pub(crate) spelling: crate::spelling::SpellState,
    _subscriptions: Vec<Subscription>,
}

pub struct ReviewRun {
    pub added: usize,
    focus: String,
    stop: Arc<AtomicBool>,
    changes: ChangeSet,
    _task: Task<()>,
}

fn has_room_for_review(blocks: &[Block], occupied: &std::collections::BTreeSet<String>) -> bool {
    blocks
        .iter()
        .any(|block| block.kind != BlockKind::Code && !occupied.contains(&block.id))
}

struct BubbleAnchor {
    above: bool,
    head: usize,
    span: Range<usize>,
}

fn shortcut_hint(key: &str) -> String {
    format!("Ctrl+{key}")
}

impl EditorPane {
    pub fn new(ai: Arc<dyn AiBackend>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("markdown")
                .line_number(false)
                .soft_wrap(true)
        });
        let (strike, fills) = editor.update(cx, |state, cx| {
            (
                state.create_decorations_collection(vec![], cx),
                state.create_range_decorations_collection(vec![], cx),
            )
        });
        let caret = CaretMarks::new(&editor, cx);
        let edits = cx.subscribe(&editor, |this, editor, event: &InputEditsEvent, cx| {
            this.on_edits(&editor, event, cx);
        });
        let moves = cx.observe(&editor, |this, editor, cx| this.on_moved(&editor, cx));
        let bubble_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Ask AI, or type an action…"));
        let typing = cx.subscribe(
            &bubble_input,
            |this, input, event: &gpui_kit::component::input::InputEvent, cx| {
                if matches!(event, gpui_kit::component::input::InputEvent::Change) {
                    this.menu = ask_menu::type_ask_menu(&input.read(cx).value());
                    cx.notify();
                }
            },
        );
        let bubble_focus = bubble_input.read(cx).focus_handle(cx);
        let bubble_focused = cx.on_focus(&bubble_focus, window, |this, _, cx| {
            this.hold_selection(true, cx);
        });
        let bubble_blurred = cx.on_blur(&bubble_focus, window, |this, _, cx| {
            this.hold_selection(false, cx);
        });
        let spelling = crate::spelling::SpellState::new(cx);
        let spell_blurred = cx.on_blur(&spelling.focus, window, |this, _, cx| {
            this.spell_menu_blurred(cx);
        });
        Self {
            editor,
            suggestions: empty_suggestions(),
            strike,
            fills,
            caret,
            _caret_blink: caret::start_blink(window, cx),
            bubble_input,
            bubble: BubbleState::default(),
            menu: AskMenu::default(),
            bubble_shown: false,
            typing_in_bubble: false,
            last_bubble: RefCell::new(None),
            bubble_leaving: ui::Presence::absent(),
            pointer_down: Rc::new(Cell::new(false)),
            last_selection: 0..0,
            refine: None,
            card_focus: cx.focus_handle(),
            card_index: 0,
            applying: None,
            tasks: HashMap::new(),
            next_id: 0,
            next_review_id: 0,
            rejected: Vec::new(),
            restored: Vec::new(),
            quick_actions: default_quick_actions(),
            is_skill: false,
            ai,
            bounds: Rc::new(Cell::new(Bounds::default())),
            heights: Rc::new(RefCell::new(HashMap::new())),
            card_rects: Rc::new(RefCell::new(Vec::new())),
            drawn_cards: RefCell::new(Vec::new()),
            hovered: None,
            review: None,
            pending: None,
            mode: None,
            notes: None,
            notes_scroll: ScrollHandle::new(),
            note_rewrites: Vec::new(),
            reveal_pending: false,
            text_map: crate::text_map_view::TextMapState::default(),
            jobs: crate::jobs_tray::JobsState::default(),
            spelling,
            _subscriptions: vec![
                edits,
                moves,
                typing,
                bubble_focused,
                bubble_blurred,
                spell_blurred,
            ],
        }
    }

    fn hold_selection(&mut self, typing: bool, cx: &mut Context<Self>) {
        if self.typing_in_bubble != typing {
            self.typing_in_bubble = typing;
            self.refresh_decorations(cx);
            cx.notify();
        }
    }

    fn on_edits(
        &mut self,
        editor: &Entity<EditorState>,
        event: &InputEditsEvent,
        cx: &mut Context<Self>,
    ) {
        perf_span!("pane.on_edits");
        self.map_edited(cx);
        self.caret.restart(editor, cx);
        let frontmatter = frontmatter_of(editor.read(cx).text());
        let applying = self.applying.take();
        let mut next = self.suggestions.clone();
        for (edit, rope) in event.edits() {
            let changes = edit_change(edit.start_byte, edit.old_end_byte, edit.new_end_byte, rope);
            if let Some(review) = self.review.as_mut() {
                review.changes = review.changes.compose(&changes);
            }
            self.map_notes(&changes);
            self.map_spelling(edit.start_byte, edit.old_end_byte, edit.new_end_byte);
            next = match &applying {
                Some(Applying::One(id)) => accept_suggestions(
                    &next,
                    std::slice::from_ref(id),
                    &changes,
                    frontmatter.as_ref(),
                ),
                Some(Applying::Many(ids, planned)) => {
                    accept_suggestions(&next, ids, planned, frontmatter.as_ref())
                }
                None => map_suggestions(&next, &changes, frontmatter.as_ref()),
            };
        }
        self.rejected.clear();
        self.restored.clear();
        self.set_suggestions(next, cx);
        self.check_spelling(true, cx);
        cx.emit(PaneEvent::Changed);
    }

    fn on_moved(&mut self, editor: &Entity<EditorState>, cx: &mut Context<Self>) {
        let selection = editor.read(cx).selected_range();
        if selection == self.last_selection {
            return;
        }
        self.last_selection = selection.clone();
        self.caret.restart(editor, cx);
        self.follow_caret_with_spell_menu(&selection);
        self.bubble = next_bubble(
            self.bubble,
            BubbleChange {
                selection: BubbleRange {
                    from: selection.start,
                    to: selection.end,
                },
                moved: true,
                opened: false,
                closed: false,
            },
        );
        self.refine = None;
        self.refresh_decorations(cx);
        cx.notify();
    }

    pub fn text(&self, cx: &App) -> String {
        self.editor.read(cx).text().to_string()
    }

    /// The text without copying it: a Rope shares its content with the editor's.
    pub fn rope(&self, cx: &App) -> Rope {
        self.editor.read(cx).text().clone()
    }

    pub(crate) fn set_suggestions(&mut self, next: SuggestionsState, cx: &mut Context<Self>) {
        perf_span!("pane.set_suggestions");
        let notice = next.notice.filter(|notice| {
            self.suggestions.notice.map(|before| before.serial) != Some(notice.serial)
        });
        let failed: Vec<(String, String)> = newly_failed(&self.suggestions, &next)
            .into_iter()
            .map(|failed| (failed.id.clone(), describe_error(&failed.error)))
            .collect();
        self.suggestions = next;
        self.drop_dead_tasks();
        if let Some(notice) = notice {
            cx.emit(PaneEvent::Toast {
                text: notice_message(notice.kind).to_string(),
                tone: ToastTone::Neutral,
                actions: Vec::new(),
            });
        }
        for (id, text) in failed {
            cx.emit(PaneEvent::Toast {
                text,
                tone: ToastTone::Danger,
                actions: vec![
                    PaneToastAction::RetrySuggestion(id.clone()),
                    PaneToastAction::ShowSuggestion(id),
                ],
            });
        }
        self.refresh_decorations(cx);
        cx.notify();
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        self.stop_review_quietly(cx);
        self.pending = None;
        self.mode = None;
        self.notes = None;
        self.note_rewrites.clear();
        self.tasks.clear();
        self.jobs.clear();
        self.suggestions = empty_suggestions();
        self.bubble = BubbleState::default();
        self.refine = None;
        self.rejected.clear();
        self.restored.clear();
    }

    pub fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.reset(cx);
        self.editor.update(cx, |state, cx| {
            state.set_value(text.to_string(), window, cx);
        });
        self.suggestions = empty_suggestions();
        self.clear_spelling();
        self.check_spelling(false, cx);
        self.refresh_decorations(cx);
        cx.notify();
    }

    pub fn replace_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.set_text(text, window, cx);
    }

    pub fn refresh_text(
        &mut self,
        change: &crate::document::MinimalChange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.reset(cx);
        let cursor = self.cursor(cx);
        let insert = change.insert.clone();
        let range = change.from..change.to;
        self.editor.update(cx, |state, cx| {
            state.set_selected_range(range, cx);
            state.replace(insert, window, cx);
            let text = state.value();
            let mut at = cursor.min(text.len());
            while !text.is_char_boundary(at) {
                at -= 1;
            }
            state.set_selected_range(at..at, cx);
        });
        self.suggestions = empty_suggestions();
        self.refresh_decorations(cx);
        cx.notify();
    }

    pub fn restore_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.reset(cx);
        self.editor.update(cx, |state, cx| {
            state.replace_all(text.to_string(), window, cx);
        });
        self.suggestions = empty_suggestions();
        self.check_spelling(false, cx);
        self.refresh_decorations(cx);
        cx.notify();
    }

    pub fn edit_range(
        &mut self,
        range: Range<usize>,
        insert: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cursor = self.cursor(cx);
        let at = if cursor <= range.start {
            cursor
        } else if cursor >= range.end {
            cursor + insert.len() - (range.end - range.start)
        } else {
            range.start + insert.len()
        };
        let insert = insert.to_string();
        self.editor.update(cx, |state, cx| {
            state.set_selected_range(range, cx);
            state.replace(insert, window, cx);
            state.set_selected_range(at..at, cx);
        });
        self.refresh_decorations(cx);
        cx.notify();
    }

    pub fn open_review_count(&self) -> usize {
        open_review_count(&self.suggestions)
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |state, cx| state.focus(window, cx));
    }

    fn drop_dead_tasks(&mut self) {
        let live: Vec<String> = self
            .suggestions
            .items
            .iter()
            .filter(|item| match item {
                Suggestion::Pending(_) => true,
                Suggestion::Ready(ready) => matches!(ready.refine, RefineState::Working { .. }),
                Suggestion::Failed(_) => false,
            })
            .map(|item| item.id.clone())
            .collect();
        self.tasks.retain(|id, _| live.contains(id));
        self.jobs.keep_started(&live);
    }

    fn cursor(&self, cx: &App) -> usize {
        self.editor.read(cx).cursor()
    }

    fn selection(&self, cx: &App) -> Selection {
        let range = self.editor.read(cx).selected_range();
        Selection {
            from: range.start,
            to: range.end,
        }
    }

    fn current_target(&self, cx: &App) -> Option<Target> {
        let text = self.text(cx);
        resolve_target(&extract_blocks(&text), &text, self.selection(cx))
    }

    fn active_at_cursor(&self, cx: &App) -> Option<ReadySuggestion> {
        let cursor = self.cursor(cx);
        active_suggestion(&self.suggestions, Some(cursor))
            .filter(|active| active.from <= cursor && cursor <= active.to)
            .cloned()
    }

    fn active(&self, cx: &App) -> Option<ReadySuggestion> {
        active_suggestion(&self.suggestions, Some(self.cursor(cx))).cloned()
    }

    fn editor_focused(&self, window: &Window, cx: &App) -> bool {
        self.editor.read(cx).focus_handle(cx).is_focused(window)
    }

    fn pane_focused(&self, window: &Window, cx: &App) -> bool {
        self.editor_focused(window, cx)
            || self.card_focus.is_focused(window)
            || self
                .bubble_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
            || self
                .refine
                .as_ref()
                .is_some_and(|refine| refine.input.read(cx).focus_handle(cx).is_focused(window))
    }

    fn target_preview(&self, cx: &App) -> Option<Range<usize>> {
        if !self.bubble.ask_open || {
            let s = self.selection(cx);
            s.from != s.to
        } {
            return None;
        }
        self.current_target(cx).map(|target| target.from..target.to)
    }

    fn held_selection_fill(&self, roles: Roles, cx: &App) -> Option<RangeDecoration> {
        let selection = self.selection(cx);
        let held = held_selection(
            self.typing_in_bubble,
            BubbleRange {
                from: selection.from,
                to: selection.to,
            },
        )?;
        Some(
            RangeDecoration::new(held.from..held.to)
                .with_style(RangeDecorationStyle::Fill)
                .with_color(roles.selection),
        )
    }

    pub fn refresh_decorations(&mut self, cx: &mut Context<Self>) {
        perf_span!("pane.refresh_decorations");
        let roles = theme::roles(cx);
        let active_id = self.active(cx).map(|item| item.id.clone());
        let strikes: Vec<TextDecoration> = self
            .suggestions
            .items
            .iter()
            .filter_map(Suggestion::as_ready)
            .flat_map(|ready| {
                let from = ready.from;
                ready.diff.hunks.iter().filter_map(move |hunk| match hunk {
                    DiffHunk::Deleted {
                        from: start,
                        to: end,
                        ..
                    } if start < end => Some(TextDecoration::new(
                        from + start..from + end,
                        HighlightStyle {
                            color: Some(roles.muted),
                            strikethrough: Some(StrikethroughStyle {
                                thickness: px(1.),
                                color: Some(roles.ink),
                            }),
                            ..Default::default()
                        },
                    )),
                    _ => None,
                })
            })
            .collect();
        let mut fills: Vec<RangeDecoration> = self
            .suggestions
            .items
            .iter()
            .filter(|item| item.to > item.from && !item.is_pending())
            .map(|item| {
                let active = Some(&item.id) == active_id.as_ref();
                let decoration = RangeDecoration::new(item.from..item.to);
                match item {
                    Suggestion::Ready(ready)
                        if matches!(ready.refine, RefineState::Failed { .. }) =>
                    {
                        decoration
                            .with_style(RangeDecorationStyle::Fill)
                            .with_color(roles.danger_fill)
                    }
                    Suggestion::Ready(_) if active => decoration
                        .with_style(RangeDecorationStyle::Fill)
                        .with_color(roles.ai_fill),
                    Suggestion::Ready(_) => decoration
                        .with_style(RangeDecorationStyle::Fill)
                        .with_color(roles.fill),
                    Suggestion::Pending(_) => decoration
                        .with_style(RangeDecorationStyle::Fill)
                        .with_color(roles.ai_fill),
                    Suggestion::Failed(_) => decoration
                        .with_style(RangeDecorationStyle::Fill)
                        .with_color(roles.danger_fill),
                }
            })
            .collect();
        if let Some(range) = self
            .active_note_range()
            .filter(|range| range.end > range.start)
        {
            fills.push(
                RangeDecoration::new(range)
                    .with_style(RangeDecorationStyle::Fill)
                    .with_color(roles.ai_fill),
            );
        }
        if let Some(preview) = self
            .target_preview(cx)
            .filter(|range| range.end > range.start)
        {
            fills.push(
                RangeDecoration::new(preview)
                    .with_style(RangeDecorationStyle::Fill)
                    .with_color(roles.selection),
            );
        }
        fills.extend(self.held_selection_fill(roles, cx));
        self.strike.set(strikes, cx);
        self.fills.set(fills, cx);
        let inlays = self.inline_inlays(roles);
        let zones = self.block_zones(cx);
        self.editor.update(cx, |state, cx| {
            state.set_inlays(inlays, cx);
            state.set_block_zones(zones, cx);
        });
    }

    fn inline_inlays(&self, roles: Roles) -> Vec<Inlay> {
        let inserted = HighlightStyle {
            color: Some(roles.ai),
            background_color: Some(roles.ai_fill),
            ..Default::default()
        };
        self.suggestions
            .items
            .iter()
            .filter_map(Suggestion::as_ready)
            .flat_map(|ready| inline_inserts(ready.from, &ready.diff))
            .flat_map(|insert| {
                let gap = insert
                    .separated
                    .then(|| Inlay::new(insert.offset, INLAY_GAP_TEXT, HighlightStyle::default()));
                gap.into_iter()
                    .chain([Inlay::new(insert.offset, insert.text, inserted)])
            })
            .collect()
    }

    /// The Suggestions whose Replacement or error shows in rows below their range.
    fn zone_items(&self) -> Vec<&Suggestion> {
        self.suggestions
            .items
            .iter()
            .filter(|item| match item {
                Suggestion::Ready(ready) => block_replacement(&ready.diff).is_some(),
                Suggestion::Failed(_) => true,
                Suggestion::Pending(_) => false,
            })
            .collect()
    }

    /// Each zone's Suggestion and the rows it reserves, in the order zones stack under a line.
    fn zone_layout(&self, cx: &App) -> Vec<(&Suggestion, usize)> {
        let line_height = self.line_height(cx);
        let heights = self.heights.borrow();
        let mut layout: Vec<(&Suggestion, usize)> = self
            .zone_items()
            .into_iter()
            .map(|item| {
                let height = heights
                    .get(&zone_key(&item.id))
                    .copied()
                    .unwrap_or(line_height);
                let rows = zone_rows(
                    f32::from(height + px(ZONE_TOP + ZONE_BOTTOM)),
                    f32::from(line_height),
                );
                (item, rows)
            })
            .collect();
        layout.sort_by_key(|(item, _)| item.to);
        layout
    }

    fn block_zones(&self, cx: &App) -> Vec<BlockZone> {
        self.zone_layout(cx)
            .into_iter()
            .map(|(item, rows)| BlockZone::new(item.to, rows))
            .collect()
    }

    /// The buffer line holding `offset`.
    fn line_of(&self, offset: usize, cx: &App) -> usize {
        let text = self.editor.read(cx).text();
        component::RopeExt::offset_to_point(text, offset.min(text.len())).row
    }

    /// The bottom of the zones reserved under the line holding `offset`, if it has any, so
    /// a floating Card does not cover them.
    fn below_zones(&self, offset: usize, cx: &App) -> Option<Pixels> {
        let line = self.line_of(offset, cx);
        let rows: usize = self
            .zone_layout(cx)
            .into_iter()
            .filter(|(item, _)| self.line_of(item.to, cx) == line)
            .map(|(_, rows)| rows)
            .sum();
        if rows == 0 {
            return None;
        }
        let (line_end, _) = self.line_bounds(line, cx)?;
        Some(line_end.bottom() + self.rows_height(rows, cx))
    }

    fn rows_height(&self, rows: usize, cx: &App) -> Pixels {
        self.line_height(cx) * f32::from(u16::try_from(rows).unwrap_or(u16::MAX))
    }

    /// The bounds of the last and first rows of a buffer line.
    fn line_bounds(&self, line: usize, cx: &App) -> Option<(Bounds<Pixels>, Bounds<Pixels>)> {
        let state = self.editor.read(cx);
        let text = state.text();
        let start = component::RopeExt::line_start_offset(text, line);
        let end = component::RopeExt::line_end_offset(text, line);
        let line_end = state.range_to_bounds(&(end..end))?;
        let line_start = state.range_to_bounds(&(start..start)).unwrap_or(line_end);
        Some((line_end, line_start))
    }

    fn line_height(&self, cx: &App) -> Pixels {
        self.editor
            .read(cx)
            .range_to_bounds(&(0..0))
            .map_or(ui::TEXT_DOCUMENT * 1.5, |bounds| bounds.size.height)
    }

    fn sync_zones(&self, cx: &mut Context<Self>) {
        let zones = self.block_zones(cx);
        self.editor
            .update(cx, |state, cx| state.set_block_zones(zones, cx));
    }

    fn ask(&mut self, _: &AskAi, window: &mut Window, cx: &mut Context<Self>) {
        let empty = {
            let s = self.selection(cx);
            s.from == s.to
        };
        if let Some(active) = self.active_at_cursor(cx).filter(|_| empty) {
            if matches!(active.refine, RefineState::Working { .. }) {
                cx.propagate();
                return;
            }
            self.open_refine(&active.id, window, cx);
            return;
        }
        if self.current_target(cx).is_none() {
            cx.propagate();
            return;
        }
        let selection = self.selection(cx);
        self.bubble = next_bubble(
            self.bubble,
            BubbleChange {
                selection: BubbleRange {
                    from: selection.from,
                    to: selection.to,
                },
                moved: false,
                opened: true,
                closed: false,
            },
        );
        self.bubble_input
            .update(cx, |state, cx| state.focus(window, cx));
        self.refresh_decorations(cx);
        cx.notify();
    }

    fn visible_actions(&self) -> Vec<QuickAction> {
        desafinado_ai::quick_actions::quick_actions_for(self.is_skill, &self.quick_actions)
    }

    pub fn typing_outside_text(&self, window: &Window, cx: &App) -> bool {
        self.bubble_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
            || self
                .refine
                .as_ref()
                .is_some_and(|refine| refine.input.read(cx).focus_handle(cx).is_focused(window))
    }

    pub fn forget_bubble(&mut self) {
        self.bubble_shown = false;
        self.last_bubble.replace(None);
        self.bubble_leaving = ui::Presence::absent();
    }

    pub fn put_away(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refine = None;
        self.last_bubble.replace(None);
        self.dismiss_bubble(window, cx);
    }

    fn dismiss_bubble(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selection = self.selection(cx);
        self.bubble = next_bubble(
            self.bubble,
            BubbleChange {
                selection: BubbleRange {
                    from: selection.from,
                    to: selection.to,
                },
                moved: false,
                opened: false,
                closed: true,
            },
        );
        self.clear_bubble(window, cx);
        self.focus(window, cx);
        self.refresh_decorations(cx);
        cx.notify();
    }

    fn clear_bubble(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = AskMenu::default();
        self.bubble_input
            .update(cx, |state, cx| state.set_value("", window, cx));
    }

    fn bubble_move(&mut self, step: i8, cx: &mut Context<Self>) {
        self.menu = ask_menu::move_ask_menu(&self.visible_actions(), &self.menu, step);
        cx.notify();
    }

    fn bubble_up(&mut self, _: &BubbleUp, _: &mut Window, cx: &mut Context<Self>) {
        self.bubble_move(-1, cx);
    }

    fn bubble_down(&mut self, _: &BubbleDown, _: &mut Window, cx: &mut Context<Self>) {
        self.bubble_move(1, cx);
    }

    fn bubble_cancel(&mut self, _: &BubbleCancel, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_bubble(window, cx);
    }

    fn bubble_confirm(&mut self, _: &BubbleConfirm, window: &mut Window, cx: &mut Context<Self>) {
        let instruction = match ask_menu::chosen_row(&self.visible_actions(), &self.menu) {
            Some(AskRow::Action { action, .. }) => Instruction::QuickAction {
                id: action.id.clone(),
                label: action.label.clone(),
                text: action.instruction.clone(),
            },
            Some(AskRow::Free { text }) => Instruction::FreeText { text },
            None => return,
        };
        self.last_bubble.replace(None);
        self.clear_bubble(window, cx);
        self.ask_with(instruction, cx);
        self.focus(window, cx);
    }

    pub(crate) fn ask_with(&mut self, instruction: Instruction, cx: &mut Context<Self>) {
        let text = self.text(cx);
        let blocks = extract_blocks(&text);
        let Some(target) = resolve_target(&blocks, &text, self.selection(cx)) else {
            return;
        };
        self.next_id += 1;
        let id = format!("s{}", self.next_id);
        let next = begin_suggestion(
            &self.suggestions,
            desafinado_core::suggestions::SuggestionDraft {
                id: id.clone(),
                from: target.from,
                to: target.to,
                source: target.source,
                original: target.text.clone(),
                instruction: instruction.clone(),
                origin: None,
            },
        );
        if target.source == desafinado_core::suggestions::TargetSource::Selection {
            let at = target.to;
            self.editor.update(cx, |state, cx| {
                state.set_selected_range(at..at, cx);
            });
        }
        let selection = self.selection(cx);
        self.last_selection = selection.from..selection.to;
        self.bubble = next_bubble(
            self.bubble,
            BubbleChange {
                selection: BubbleRange {
                    from: selection.from,
                    to: selection.to,
                },
                moved: false,
                opened: false,
                closed: true,
            },
        );
        self.set_suggestions(next, cx);
        let call = self.target_call(&blocks, &target, &instruction);
        self.run(id, call, false, cx);
    }

    fn target_call(
        &self,
        blocks: &[Block],
        target: &Target,
        instruction: &Instruction,
    ) -> TargetCall {
        let ids: Vec<&str> = target
            .blocks
            .iter()
            .map(|block| block.id.as_str())
            .collect();
        TargetCall {
            task: task_label(target.source, &ids, instruction),
            parts: build_target_prompt(&TargetPromptInput {
                target: target.clone(),
                neighbors: get_neighbors(blocks, target),
                instruction: instruction.text().to_string(),
            }),
            original: target.text.clone(),
        }
    }

    fn run(&mut self, id: String, call: TargetCall, refine: bool, cx: &mut Context<Self>) {
        let ai = Arc::clone(&self.ai);
        let task_id = id.clone();
        let work = cx.background_spawn(async move { ai.target(call) });
        let task = cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| {
                this.finish(&task_id, result, refine, cx);
            })
            .log_warn();
        });
        let text = self.text(cx);
        self.jobs.started(&id, &self.suggestions, &text);
        self.tasks.insert(id, task);
        self.keep_tray_ticking(cx);
        self.refresh_decorations(cx);
        cx.notify();
    }

    fn finish(
        &mut self,
        id: &str,
        result: Result<desafinado_core::suggestions::SuggestionResult, ClientError>,
        refine: bool,
        cx: &mut Context<Self>,
    ) {
        if self.tasks.remove(id).is_none() {
            return;
        }
        let next = match (result, refine) {
            (Ok(result), false) => resolve_suggestion(&self.suggestions, id, &result),
            (Ok(result), true) => resolve_refine(&self.suggestions, id, &result),
            (Err(error), false) => fail_suggestion(&self.suggestions, id, error),
            (Err(error), true) => fail_refine(&self.suggestions, id, error),
        };
        if !refine && find_suggestion(&next, id).is_some_and(Suggestion::is_ready) {
            self.jobs.answered(id, &self.suggestions, &next);
        }
        self.set_suggestions(next, cx);
        cx.emit(PaneEvent::Changed);
    }

    fn open_refine(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Refine: make it shorter…"));
        input.update(cx, |state, cx| state.focus(window, cx));
        self.refine = Some(RefineInput {
            id: id.to_string(),
            input,
        });
        self.card_index = 0;
        cx.notify();
    }

    fn toggle_refine(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.refine.as_ref().is_some_and(|refine| refine.id == id) {
            self.refine = None;
            self.focus(window, cx);
            cx.notify();
        } else {
            self.open_refine(id, window, cx);
        }
    }

    fn refine_confirm(&mut self, _: &RefineConfirm, window: &mut Window, cx: &mut Context<Self>) {
        let Some(refine) = self.refine.as_ref() else {
            return;
        };
        let text = refine.input.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        let id = refine.id.clone();
        self.refine = None;
        self.refine_with(
            &id,
            desafinado_core::suggestions::free_text_instruction(&text),
            false,
            cx,
        );
        self.focus(window, cx);
    }

    fn refine_cancel(&mut self, _: &RefineCancel, window: &mut Window, cx: &mut Context<Self>) {
        self.refine = None;
        self.focus(window, cx);
        cx.notify();
    }

    fn refine_with(
        &mut self,
        id: &str,
        instruction: Instruction,
        retry: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(found) = find_suggestion(&self.suggestions, id)
            .and_then(Suggestion::as_ready)
            .cloned()
        else {
            return;
        };
        let allowed = if retry {
            matches!(found.refine, RefineState::Failed { .. })
        } else {
            !matches!(found.refine, RefineState::Working { .. })
        };
        if !allowed {
            return;
        }
        let text = self.text(cx);
        let blocks = extract_blocks(&text);
        let target = target_from_range(
            &blocks,
            &text,
            Selection {
                from: found.from,
                to: found.to,
            },
            found.source,
        );
        let ids: Vec<&str> = target
            .blocks
            .iter()
            .map(|block| block.id.as_str())
            .collect();
        let call = TargetCall {
            task: refine_task_label(&ids, &instruction),
            parts: build_refine_prompt(&RefinePromptInput {
                target: target.clone(),
                neighbors: get_neighbors(&blocks, &target),
                instruction: instruction.text().to_string(),
                history: instruction_history(&found),
                latest_replacement: found.replacement.clone(),
            }),
            original: target.text.clone(),
        };
        let next = if retry {
            retry_refine(&self.suggestions, id)
        } else {
            begin_refine(&self.suggestions, id, instruction)
        };
        self.set_suggestions(next, cx);
        self.run(id.to_string(), call, true, cx);
    }

    fn retry_refine_of(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let instruction =
            match find_suggestion(&self.suggestions, id).and_then(Suggestion::as_ready) {
                Some(ReadySuggestion {
                    refine: RefineState::Failed { instruction, .. },
                    ..
                }) => instruction.clone(),
                _ => return,
            };
        self.refine_with(id, instruction, true, cx);
        self.focus(window, cx);
    }

    pub fn retry(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Suggestion::Failed(found)) = find_suggestion(&self.suggestions, id).cloned()
        else {
            return;
        };
        let text = self.text(cx);
        let blocks = extract_blocks(&text);
        let target = target_from_range(
            &blocks,
            &text,
            Selection {
                from: found.from,
                to: found.to,
            },
            found.source,
        );
        let call = self.target_call(&blocks, &target, &found.instruction);
        let next = retry_suggestion(&self.suggestions, id);
        self.set_suggestions(next, cx);
        self.run(id.to_string(), call, false, cx);
        self.focus(window, cx);
    }

    pub fn show(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(found) = find_suggestion(&self.suggestions, id) else {
            return;
        };
        let at = found.to;
        self.editor.update(cx, |state, cx| {
            state.set_selected_range(at..at, cx);
            state.focus(window, cx);
        });
        cx.notify();
    }

    pub fn accept(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ready) = find_suggestion(&self.suggestions, id)
            .and_then(Suggestion::as_ready)
            .cloned()
        else {
            return;
        };
        let before = self.text(cx);
        self.applying = Some(Applying::One(ready.id.clone()));
        let replacement = ready.replacement.clone();
        let at = ready.from + replacement.len();
        self.editor.update(cx, |state, cx| {
            state.set_selected_range(ready.from..ready.to, cx);
            state.replace(replacement, window, cx);
            state.set_selected_range(at..at, cx);
        });
        if self.applying.take().is_some() {
            let next = reject_suggestion(&self.suggestions, &ready.id);
            self.set_suggestions(next, cx);
        }
        self.resolve_rewritten_note(&ready.id, cx);
        self.emit_accepted(before, vec![accepted_change(&ready)], cx);
    }

    fn emit_accepted(
        &mut self,
        before: String,
        changes: Vec<AcceptedChange>,
        cx: &mut Context<Self>,
    ) {
        if !changes.is_empty() {
            let text = self.text(cx);
            cx.emit(PaneEvent::Accepted {
                before,
                text,
                changes,
            });
        }
        self.refresh_decorations(cx);
        cx.notify();
    }

    pub fn reject(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(found) = find_suggestion(&self.suggestions, id).cloned() else {
            return;
        };
        self.tasks.remove(id);
        if let Suggestion::Ready(ready) = found {
            self.rejected.push(ready);
            self.restored.clear();
        }
        let next = reject_suggestion(&self.suggestions, id);
        self.set_suggestions(next, cx);
    }

    pub fn accept_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let plan = accept_all_plan(&self.suggestions);
        let (Some(first), Some(last)) = (plan.changes.first(), plan.changes.last()) else {
            return;
        };
        let accepted: Vec<AcceptedChange> = plan
            .ids
            .iter()
            .filter_map(|id| find_suggestion(&self.suggestions, id).and_then(Suggestion::as_ready))
            .map(accepted_change)
            .collect();
        let before = self.text(cx);
        let changes = ChangeSet::of(&plan.changes, before.len());
        let after = changes.apply(&before);
        let from = first.from;
        let to = last.to;
        let new_to = changes.map_pos(to, desafinado_core::change_set::Assoc::After);
        let insert = after[from..new_to].to_string();
        self.applying = Some(Applying::Many(plan.ids.clone(), changes));
        self.editor.update(cx, |state, cx| {
            state.set_selected_range(from..to, cx);
            state.replace(insert, window, cx);
            state.set_selected_range(new_to..new_to, cx);
        });
        if self.applying.take().is_some() {
            let next =
                desafinado_core::suggestions::reject_suggestions(&self.suggestions, &plan.ids);
            self.set_suggestions(next, cx);
        }
        self.emit_accepted(before, accepted, cx);
        self.focus(window, cx);
    }

    pub fn reject_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open_review_count() == 0 {
            return;
        }
        let next = reject_review_suggestions(&self.suggestions);
        self.set_suggestions(next, cx);
        self.focus(window, cx);
    }

    pub(crate) fn start_revise(&mut self, focus: String, cx: &mut Context<Self>) {
        if self.review.is_some() {
            return;
        }
        let text = self.text(cx);
        if review_too_large(text.len()) {
            cx.emit(PaneEvent::Toast {
                text: review_too_large_text(text.len()),
                tone: ToastTone::Neutral,
                actions: Vec::new(),
            });
            return;
        }
        let focus = focus.trim().to_string();
        let blocks = extract_blocks(&text);
        let occupied = occupied_block_ids(&self.suggestions, &blocks);
        if !has_room_for_review(&blocks, &occupied) {
            cx.emit(PaneEvent::Toast {
                text: review_summary(0, false),
                tone: ToastTone::Neutral,
                actions: Vec::new(),
            });
            return;
        }
        let call = ReviewCall {
            task: review_task_label(&focus),
            parts: build_review_prompt(&ReviewPromptInput {
                blocks: blocks.clone(),
                focus: Some(focus.clone()).filter(|focus| !focus.is_empty()),
            }),
            context: ReviewValidationContext { blocks, occupied },
        };
        let stop = Arc::new(AtomicBool::new(false));
        let (sender, mut receiver) = mpsc::unbounded::<ReviewEvent>();
        let ai = Arc::clone(&self.ai);
        let flag = Arc::clone(&stop);
        cx.background_spawn(async move {
            ai.review(call, &mut |event| {
                let open = sender.unbounded_send(event).is_ok();
                open && !flag.load(Ordering::SeqCst)
            });
        })
        .detach();
        let run_focus = focus.clone();
        let task = cx.spawn(async move |this, cx| {
            while let Some(event) = receiver.next().await {
                let done = this
                    .update(cx, |this, cx| this.on_review_event(event, cx))
                    .unwrap_or(true);
                if done {
                    break;
                }
            }
        });
        self.review = Some(ReviewRun {
            added: 0,
            focus: run_focus,
            stop,
            changes: ChangeSet::empty(text.len()),
            _task: task,
        });
        cx.emit(PaneEvent::Changed);
        cx.notify();
    }

    fn on_review_event(&mut self, event: ReviewEvent, cx: &mut Context<Self>) -> bool {
        let running = self
            .review
            .as_ref()
            .is_some_and(|run| !run.stop.load(Ordering::SeqCst));
        if !running {
            return true;
        }
        match event {
            ReviewEvent::Edit(edit) => {
                self.place_review(edit, cx);
                false
            }
            ReviewEvent::Dropped(_) => false,
            ReviewEvent::Done { error } => {
                self.end_review(error, cx);
                true
            }
            ReviewEvent::Failed(error) => {
                self.end_review(Some(error), cx);
                true
            }
        }
    }

    fn place_review(
        &mut self,
        edit: desafinado_core::suggestions::ValidatedEdit,
        cx: &mut Context<Self>,
    ) {
        let Some((changes, focus, added)) = self
            .review
            .as_ref()
            .map(|run| (run.changes.clone(), run.focus.clone(), run.added))
        else {
            return;
        };
        if added >= MAX_REVIEW_EDITS {
            return;
        }
        let doc = self.text(cx);
        self.next_review_id += 1;
        let id = format!("r{}", self.next_review_id);
        let placement = place_review_edit(&PlaceReviewEditInput {
            state: &self.suggestions,
            edit: &edit,
            changes: &changes,
            doc: &doc,
            id: &id,
            focus: &focus,
        });
        if let ReviewPlacement::Placed { draft, result } = placement {
            let next =
                resolve_suggestion(&begin_suggestion(&self.suggestions, draft), &id, &result);
            self.set_suggestions(next, cx);
            if let Some(run) = self.review.as_mut() {
                run.added += 1;
            }
            cx.emit(PaneEvent::Changed);
        }
    }

    fn end_review(&mut self, failure: Option<ClientError>, cx: &mut Context<Self>) {
        let Some(run) = self.review.take() else {
            return;
        };
        let actions = match failure {
            Some(_) => vec![PaneToastAction::RetryReview(run.focus.clone())],
            None => Vec::new(),
        };
        cx.emit(PaneEvent::Toast {
            text: review_notice_text(run.added, failure.as_ref()),
            tone: if failure.is_some() {
                ToastTone::Danger
            } else {
                ToastTone::Neutral
            },
            actions,
        });
        self.review_ended(cx);
    }

    fn review_ended(&mut self, cx: &mut Context<Self>) {
        self.reveal_pending = true;
        cx.emit(PaneEvent::Changed);
        self.refresh_decorations(cx);
        cx.notify();
    }

    pub(crate) fn stop_review_quietly(&mut self, _: &mut Context<Self>) {
        if let Some(run) = self.review.take() {
            run.stop.store(true, Ordering::SeqCst);
        }
    }

    pub fn stop_review(&mut self, cx: &mut Context<Self>) {
        if self.stop_pending() {
            cx.emit(PaneEvent::Toast {
                text: "Review stopped".to_owned(),
                tone: ToastTone::Neutral,
                actions: Vec::new(),
            });
            cx.emit(PaneEvent::Changed);
            self.refresh_decorations(cx);
            cx.notify();
            return;
        }
        let Some(run) = self.review.take() else {
            return;
        };
        run.stop.store(true, Ordering::SeqCst);
        cx.emit(PaneEvent::Toast {
            text: review_summary(run.added, true),
            tone: ToastTone::Neutral,
            actions: Vec::new(),
        });
        self.review_ended(cx);
    }

    fn reveal_first_review_suggestion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pane_focused(window, cx) {
            return;
        }
        let Some(first) = first_review_suggestion(&self.suggestions) else {
            return;
        };
        let anchor = if suggestion_at(&self.suggestions, first.from).map(|item| item.id.as_str())
            == Some(first.id.as_str())
        {
            first.from
        } else {
            (first.from + 1).min(first.to)
        };
        self.editor.update(cx, |state, cx| {
            state.set_selected_range(anchor..anchor, cx);
            state.focus(window, cx);
        });
    }

    fn accept_active(&mut self, _: &AcceptActive, window: &mut Window, cx: &mut Context<Self>) {
        let found = suggestion_at(&self.suggestions, self.cursor(cx)).cloned();
        match found {
            None => cx.propagate(),
            Some(Suggestion::Ready(ready)) => {
                self.accept(&ready.id, window, cx);
                self.advance(self.cursor(cx), window, cx);
            }
            Some(_) => {}
        }
    }

    fn escape_at_cursor(
        &mut self,
        _: &EscapeAtCursor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.close_spell_menu(window, cx) {
            return;
        }
        if self.bubble_shown {
            self.dismiss_bubble(window, cx);
            return;
        }
        if self.refine.take().is_some() {
            cx.notify();
            return;
        }
        let found = suggestion_at(&self.suggestions, self.cursor(cx)).cloned();
        match found {
            Some(Suggestion::Ready(ready)) if ready.refine != RefineState::Idle => {
                self.tasks.remove(&ready.id);
                let next = cancel_refine(&self.suggestions, &ready.id);
                self.set_suggestions(next, cx);
            }
            Some(Suggestion::Ready(ready)) => {
                self.reject(&ready.id, cx);
                self.advance(ready.from, window, cx);
            }
            Some(item) => self.reject(&item.id, cx),
            None => cx.propagate(),
        }
    }

    fn pane_undo(&mut self, _: &PaneUndo, _: &mut Window, cx: &mut Context<Self>) {
        let Some(ready) = self.rejected.pop() else {
            cx.propagate();
            return;
        };
        let text = self.text(cx);
        let range_text = text
            .get(ready.from..ready.to)
            .unwrap_or_default()
            .to_string();
        let next = restore_suggestion(&self.suggestions, &ready, &range_text);
        self.restored.push(ready);
        self.set_suggestions(next, cx);
    }

    fn pane_redo(&mut self, _: &PaneRedo, _: &mut Window, cx: &mut Context<Self>) {
        let Some(ready) = self.restored.pop() else {
            cx.propagate();
            return;
        };
        let next = reject_suggestion(&self.suggestions, &ready.id);
        self.rejected.push(ready);
        self.set_suggestions(next, cx);
    }

    fn step_version(&mut self, step: i64, window: &mut Window, cx: &mut Context<Self>) {
        let found = if self.card_focus.is_focused(window) {
            self.active(cx)
        } else {
            self.active_at_cursor(cx)
        };
        let Some(found) = found else {
            cx.propagate();
            return;
        };
        if found.history.len() < 2 {
            return;
        }
        if matches!(found.refine, RefineState::Working { .. }) {
            return;
        }
        let next = show_version(&self.suggestions, &found.id, found.shown as i64 + step);
        self.set_suggestions(next, cx);
    }

    fn previous_version(
        &mut self,
        _: &PreviousVersion,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_version(-1, window, cx);
    }

    fn next_version(&mut self, _: &NextVersion, window: &mut Window, cx: &mut Context<Self>) {
        self.step_version(1, window, cx);
    }

    fn step(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        if !self.move_to_step(direction, window, cx) {
            cx.propagate();
        }
    }

    fn move_to_step(
        &mut self,
        direction: Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let forward = direction == Direction::Forward;
        let Some(position) = self
            .step_from_screen(forward, cx)
            .or_else(|| step_suggestion(&self.suggestions, self.cursor(cx), direction))
        else {
            return false;
        };
        self.place_caret(position, window, cx);
        true
    }

    pub(crate) fn place_caret(
        &mut self,
        position: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor.update(cx, |state, cx| {
            state.set_selected_range(position..position, cx);
            state.focus(window, cx);
        });
        self.reveal_jump(position, cx);
        cx.notify();
    }

    /// After a Suggestion is accepted or rejected, the caret moves on to the next one that starts
    /// at `from` or after it, so a Review can be read through from the keyboard. At the last one
    /// the caret stays.
    fn advance(&mut self, from: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(position) = next_from(&self.suggestions, from) {
            self.place_caret(position, window, cx);
        }
    }

    fn next_suggestion(&mut self, _: &NextSuggestion, window: &mut Window, cx: &mut Context<Self>) {
        self.step(Direction::Forward, window, cx);
    }

    fn previous_suggestion(
        &mut self,
        _: &PreviousSuggestion,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step(Direction::Backward, window, cx);
    }

    fn step_change(&mut self, direction: Direction, cx: &mut Context<Self>) {
        let Some(active) = self.active(cx) else {
            return;
        };
        let ranges = change_ranges(active.from, &active.diff);
        if let Some(target) = step_change(&ranges, self.cursor(cx), direction) {
            self.editor.update(cx, |state, cx| {
                state.set_selected_range(target..target, cx);
            });
            cx.notify();
        }
    }

    fn card_controls(&self, active: &ReadySuggestion) -> Vec<CardControl> {
        let mut controls = Vec::new();
        let working = matches!(active.refine, RefineState::Working { .. });
        if active.history.len() >= 2 && !working {
            if active.shown > 0 {
                controls.push(CardControl::PreviousVersion);
            }
            if active.shown + 1 < active.history.len() {
                controls.push(CardControl::NextVersion);
            }
        }
        if shows_changes(active.from, &active.original, &active.diff) {
            controls.extend([CardControl::PreviousChange, CardControl::NextChange]);
        }
        controls.extend([CardControl::Accept, CardControl::Reject]);
        if !working {
            controls.push(CardControl::Refine);
        }
        if self
            .refine
            .as_ref()
            .is_some_and(|refine| refine.id == active.id)
        {
            controls.push(CardControl::RefineField);
        }
        if matches!(active.refine, RefineState::Failed { .. }) {
            controls.push(CardControl::RetryRefine);
        }
        controls
    }

    fn focus_card_at(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active) = self.active(cx) else {
            return;
        };
        let controls = self.card_controls(&active);
        let Some(control) = controls.get(index).copied() else {
            return;
        };
        self.card_index = index;
        match (control, self.refine.as_ref()) {
            (CardControl::RefineField, Some(refine)) => {
                refine.input.update(cx, |state, cx| state.focus(window, cx));
            }
            _ => window.focus(&self.card_focus, cx),
        }
        cx.notify();
    }

    fn focus_card_edge(&mut self, backward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.bubble_shown {
            self.bubble_input
                .update(cx, |state, cx| state.focus(window, cx));
            cx.notify();
            return;
        }
        let Some(active) = self.active(cx) else {
            cx.propagate();
            return;
        };
        let count = self.card_controls(&active).len();
        match usize::try_from(cycle_index(count, -1, backward)) {
            Ok(index) => self.focus_card_at(index, window, cx),
            Err(_) => cx.propagate(),
        }
    }

    fn focus_card(&mut self, _: &FocusCard, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_card_edge(false, window, cx);
    }

    fn focus_card_back(&mut self, _: &FocusCardBack, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_card_edge(true, window, cx);
    }

    fn cycle_card(&mut self, backward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active) = self.active(cx) else {
            return;
        };
        let count = self.card_controls(&active).len();
        let next = cycle_index(
            count,
            self.card_index.min(count.saturating_sub(1)) as isize,
            backward,
        );
        if let Ok(index) = usize::try_from(next) {
            self.focus_card_at(index, window, cx);
        }
    }

    fn card_next(&mut self, _: &CardNext, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_card(false, window, cx);
    }

    fn card_previous(&mut self, _: &CardPrevious, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_card(true, window, cx);
    }

    fn card_press(&mut self, _: &CardPress, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active) = self.active(cx) else {
            return;
        };
        let controls = self.card_controls(&active);
        if let Some(control) = controls.get(self.card_index).copied() {
            self.press(control, &active, window, cx);
        }
    }

    fn card_leave(&mut self, _: &CardLeave, window: &mut Window, cx: &mut Context<Self>) {
        self.focus(window, cx);
        cx.notify();
    }

    fn press(
        &mut self,
        control: CardControl,
        active: &ReadySuggestion,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = active.id.clone();
        match control {
            CardControl::PreviousVersion | CardControl::NextVersion => {
                let step = if control == CardControl::PreviousVersion {
                    -1
                } else {
                    1
                };
                let next = show_version(&self.suggestions, &id, active.shown as i64 + step);
                self.set_suggestions(next, cx);
                self.keep_card_focus(control, window, cx);
            }
            CardControl::PreviousChange => self.step_change(Direction::Backward, cx),
            CardControl::NextChange => self.step_change(Direction::Forward, cx),
            CardControl::Accept => {
                self.accept(&id, window, cx);
                self.focus(window, cx);
                self.advance(self.cursor(cx), window, cx);
            }
            CardControl::Reject => {
                let from = find_suggestion(&self.suggestions, &id).map(|item| item.from);
                self.reject(&id, cx);
                self.focus(window, cx);
                if let Some(from) = from {
                    self.advance(from, window, cx);
                }
            }
            CardControl::Refine => self.toggle_refine(&id, window, cx),
            CardControl::RefineField => {}
            CardControl::RetryRefine => self.retry_refine_of(&id, window, cx),
        }
    }

    fn keep_card_focus(
        &mut self,
        pressed: CardControl,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.card_focus.is_focused(window) {
            return;
        }
        let Some(active) = self.active(cx) else {
            return;
        };
        let controls = self.card_controls(&active);
        let index = controls
            .iter()
            .position(|control| *control == pressed)
            .or_else(|| {
                controls.iter().position(|control| {
                    matches!(
                        control,
                        CardControl::PreviousVersion | CardControl::NextVersion
                    )
                })
            })
            .unwrap_or(0);
        self.card_index = index;
    }

    fn origin(&self) -> Point<Pixels> {
        self.bounds.get().origin
    }

    pub(crate) fn pane_bounds(&self) -> Bounds<Pixels> {
        self.bounds.get()
    }

    pub(crate) fn content_left(&self) -> Pixels {
        let pane = self.bounds.get();
        let column = crate::ui::COLUMN_WIDTH.min(pane.size.width);
        pane.origin.x + (pane.size.width - column) / 2. + px(COLUMN_PADDING)
    }

    pub(crate) fn content_right(&self) -> Pixels {
        let pane = self.bounds.get();
        let column = crate::ui::COLUMN_WIDTH.min(pane.size.width);
        pane.origin.x + (pane.size.width + column) / 2. - px(COLUMN_PADDING)
    }

    pub(crate) fn card_placement(&self) -> CardPlacement {
        let pane = self.bounds.get();
        placement_for(f64::from(f32::from(self.content_left() - pane.origin.x)))
    }

    fn render_card(
        &self,
        item: &Suggestion,
        kind: CardKind,
        floor: Pixels,
        card_focused: bool,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Option<(AnyElement, Pixels)> {
        let Suggestion::Ready(ready) = item else {
            return None;
        };
        if self.above_view(item.to, cx) {
            return None;
        }
        let (bounds, height) = self.place_card(item, floor, cx)?;
        self.card_rects.borrow_mut().push((bounds, kind));
        self.drawn_cards.borrow_mut().push(item.id.clone());
        let origin = self.origin();
        let frame = self
            .card_frame(&item.id, bounds.size.width, roles, cx)
            .left(bounds.origin.x - origin.x)
            .top(bounds.origin.y - origin.y);
        let card = self.render_ready_card(frame, ready, kind, card_focused, roles, cx);
        let element = ui::enter_after(
            SharedString::from(format!("card-in-{}", item.id)),
            card,
            self.card_enter(item, bounds, cx),
            if kind == CardKind::Hover {
                ui::MOTION_HOVER_DELAY
            } else {
                std::time::Duration::ZERO
            },
        )
        .into_any_element();
        Some((element, bounds.origin.y - origin.y + height + px(8.)))
    }

    fn card_enter(&self, item: &Suggestion, card: Bounds<Pixels>, cx: &App) -> ui::Enter {
        if self.card_placement() == CardPlacement::Margin {
            return ui::Enter::Slide(8.);
        }
        let anchor_at = match item {
            Suggestion::Ready(ready) => {
                desafinado_core::suggestions::first_change_at(ready.from, &ready.diff)
            }
            _ => item.from,
        };
        let range_top = self
            .editor
            .read(cx)
            .range_to_bounds(&(anchor_at..anchor_at))
            .map(|line| line.top());
        match range_top {
            Some(top) if card.bottom() <= top => ui::Enter::Rise(6.),
            _ => ui::Enter::Rise(-6.),
        }
    }

    fn place_card(
        &self,
        item: &Suggestion,
        floor: Pixels,
        cx: &App,
    ) -> Option<(Bounds<Pixels>, Pixels)> {
        let state = self.editor.read(cx);
        let anchor_at = match item {
            Suggestion::Ready(ready) => {
                desafinado_core::suggestions::first_change_at(ready.from, &ready.diff)
            }
            _ => item.from,
        };
        let line = state
            .range_to_bounds(&(anchor_at..anchor_at))
            .or_else(|| state.range_to_bounds(&(item.from..item.from)))?;
        let end = state.range_to_bounds(&(item.to..item.to)).unwrap_or(line);
        let pane = self.bounds.get();
        let origin = pane.origin;
        let width = px(MARGIN_CARD_WIDTH as f32);
        let height = self
            .heights
            .borrow()
            .get(&item.id)
            .copied()
            .unwrap_or(px(96.));
        let placement = self.card_placement();
        let (left, flip, gap, bottom) = match placement {
            CardPlacement::Margin => (
                px(margin_card_left(to_f64(self.content_left() - origin.x)) as f32),
                false,
                0.,
                line.top() - origin.y,
            ),
            CardPlacement::Floating => (
                self.content_right() - width - origin.x,
                true,
                8.,
                self.below_zones(item.to, cx).unwrap_or(end.bottom()) - origin.y,
            ),
        };
        let placed = place_popover(&PopoverInput {
            anchor: Rect {
                left: to_f64(left),
                right: to_f64(left),
                top: to_f64(line.top() - origin.y),
                bottom: to_f64(bottom),
            },
            width: to_f64(width),
            height: to_f64(height),
            area: area_of(pane),
            side: Side::Below,
            flip,
            gap,
        })?;
        let top = match placement {
            CardPlacement::Margin => px(placed.top as f32).max(floor),
            CardPlacement::Floating => px(placed.top as f32),
        };
        let left = px(placed.left as f32);
        Some((
            Bounds::new(point(origin.x + left, origin.y + top), size(width, height)),
            height,
        ))
    }

    fn card_frame(
        &self,
        id: &str,
        width: Pixels,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let heights = Rc::clone(&self.heights);
        let measured_id = id.to_string();
        let entity = cx.entity().downgrade();
        ui::dialog_frame(SharedString::from(format!("card-{id}")), width, roles)
            .absolute()
            .child(ui::measure(move |size, cx| {
                let previous = heights
                    .borrow_mut()
                    .insert(measured_id.clone(), size.height);
                if previous != Some(size.height)
                    && let Some(entity) = entity.upgrade()
                {
                    entity.update(cx, |_, cx| cx.notify());
                }
            }))
            .p(px(12.))
            .gap(px(8.))
    }

    fn render_failed_card(
        &self,
        frame: Stateful<Div>,
        id: &str,
        error: String,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let retry_id = id.to_string();
        let dismiss_id = id.to_string();
        frame
            .bg(roles.danger_fill)
            .child(ui::error_text(error, roles))
            .child(
                ui::row()
                    .gap(px(8.))
                    .child(
                        ui::small_button(
                            SharedString::from(format!("retry-{id}")),
                            "Retry",
                            ButtonKind::Default,
                            roles,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.retry(&retry_id, window, cx);
                            },
                        )),
                    )
                    .child(
                        ui::small_button(
                            SharedString::from(format!("dismiss-{id}")),
                            "Dismiss",
                            ButtonKind::Default,
                            roles,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.reject(&dismiss_id, cx);
                                this.focus(window, cx);
                            },
                        )),
                    ),
            )
    }

    fn render_ready_card(
        &self,
        frame: Stateful<Div>,
        ready: &ReadySuggestion,
        kind: CardKind,
        card_focused: bool,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let active = kind == CardKind::Active;
        let controls = if active {
            self.card_controls(ready)
        } else {
            Vec::new()
        };
        let focused = card_focused
            .then(|| controls.get(self.card_index).copied())
            .flatten();
        let id = ready.id.clone();
        let refine_error = match &ready.refine {
            RefineState::Failed { error, .. } => Some(describe_error(error)),
            _ => None,
        };
        let refine_open = self
            .refine
            .as_ref()
            .filter(|refine| active && refine.id == ready.id)
            .map(|refine| refine.input.clone());
        frame
            .when(!active, |this| this.bg(roles.raised))
            .when(active, |this| {
                this.track_focus(&self.card_focus)
                    .key_context("SuggestionCard")
                    .on_action(cx.listener(Self::card_next))
                    .on_action(cx.listener(Self::card_previous))
                    .on_action(cx.listener(Self::card_press))
                    .on_action(cx.listener(Self::card_leave))
                    .on_action(cx.listener(Self::previous_version))
                    .on_action(cx.listener(Self::next_version))
            })
            .children(
                active
                    .then(|| self.render_versions(ready, focused, roles, cx))
                    .flatten(),
            )
            .child(
                ui::row()
                    .gap(px(8.))
                    .items_start()
                    .child(ui::ai_flag(roles))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(roles.ink)
                            .child(ready.rationale.clone()),
                    ),
            )
            .children(
                active
                    .then(|| self.render_changes(ready, focused, roles, cx))
                    .flatten(),
            )
            .when(active, |this| {
                this.child(self.render_card_actions(ready, focused, roles, cx))
            })
            .when_some(refine_open, |this, input| {
                this.child(
                    div()
                        .key_context("RefineInput")
                        .on_action(cx.listener(Self::refine_confirm))
                        .on_action(cx.listener(Self::refine_cancel))
                        .on_action(cx.listener(Self::card_next))
                        .on_action(cx.listener(Self::card_previous))
                        .child(ui::field(
                            Input::new(&input).appearance(false),
                            true,
                            false,
                            roles,
                        )),
                )
            })
            .when_some(refine_error, |this, error| {
                this.child(self.render_refine_error(&id, error, active, focused, roles, cx))
            })
    }

    fn render_versions(
        &self,
        ready: &ReadySuggestion,
        focused: Option<CardControl>,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let count = ready.history.len();
        if count < 2 {
            return None;
        }
        let shown = ready.shown.min(count - 1);
        let instruction = instruction_label(&ready.history[shown].instruction).to_string();
        let working = matches!(ready.refine, RefineState::Working { .. });
        let id = &ready.id;
        let step =
            |control: CardControl, label: &'static str, disabled: bool, tip: &'static str| {
                ui::sized_button(
                    SharedString::from(format!(
                        "{}-{id}",
                        if control == CardControl::NextVersion {
                            "next"
                        } else {
                            "prev"
                        }
                    )),
                    label,
                    ButtonKind::Quiet,
                    ButtonSize::Small,
                    ui::Control {
                        disabled,
                        focused: focused == Some(control),
                    },
                    roles,
                )
                .tooltip(move |window, cx| component::tooltip::Tooltip::new(tip).build(window, cx))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if let Some(active) = this.active(cx) {
                        this.press(control, &active, window, cx);
                    }
                }))
            };
        Some(
            ui::row()
                .gap(px(4.))
                .type_small()
                .child(step(
                    CardControl::PreviousVersion,
                    "‹",
                    working || shown == 0,
                    "Previous version (Ctrl+H)",
                ))
                .child(format!("{} / {}", shown + 1, count))
                .child(step(
                    CardControl::NextVersion,
                    "›",
                    working || shown + 1 == count,
                    "Next version (Ctrl+L)",
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .pl(px(4.))
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_color(roles.muted)
                        .child(instruction),
                ),
        )
    }

    fn render_changes(
        &self,
        ready: &ReadySuggestion,
        focused: Option<CardControl>,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if !shows_changes(ready.from, &ready.original, &ready.diff) {
            return None;
        }
        let label = change_label(&change_ranges(ready.from, &ready.diff), self.cursor(cx));
        let id = &ready.id;
        let step = |control: CardControl, name: &str, label: &'static str, tip: &'static str| {
            let direction = if control == CardControl::NextChange {
                Direction::Forward
            } else {
                Direction::Backward
            };
            ui::sized_button(
                SharedString::from(format!("{name}-{id}")),
                label,
                ButtonKind::Quiet,
                ButtonSize::Small,
                ui::Control::focused(focused == Some(control)),
                roles,
            )
            .tooltip(move |window, cx| component::tooltip::Tooltip::new(tip).build(window, cx))
            .on_click(cx.listener(move |this, _, _, cx| this.step_change(direction, cx)))
        };
        Some(
            ui::row()
                .gap(px(4.))
                .type_small()
                .child(div().flex_1().text_color(roles.muted).child(label))
                .child(step(
                    CardControl::PreviousChange,
                    "prev-change",
                    "↑",
                    "Previous change",
                ))
                .child(step(
                    CardControl::NextChange,
                    "next-change",
                    "↓",
                    "Next change",
                )),
        )
    }

    fn render_card_actions(
        &self,
        ready: &ReadySuggestion,
        focused: Option<CardControl>,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Div {
        let working = matches!(ready.refine, RefineState::Working { .. });
        let id = ready.id.clone();
        let accept_id = id.clone();
        let reject_id = id.clone();
        let refine_id = id.clone();
        let is_focused = |control: CardControl| focused == Some(control);
        ui::row()
            .gap(px(8.))
            .flex_wrap()
            .child(
                ui::sized_button(
                    SharedString::from(format!("accept-{id}")),
                    "Accept",
                    ButtonKind::Primary,
                    ButtonSize::Small,
                    ui::Control::focused(is_focused(CardControl::Accept)),
                    roles,
                )
                .child(ui::shortcut_hint(shortcut_hint("↵"), true, roles))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.accept(&accept_id, window, cx);
                    this.focus(window, cx);
                })),
            )
            .child(
                ui::sized_button(
                    SharedString::from(format!("reject-{id}")),
                    "Reject",
                    ButtonKind::Default,
                    ButtonSize::Small,
                    ui::Control::focused(is_focused(CardControl::Reject)),
                    roles,
                )
                .child(ui::shortcut_hint("Esc", false, roles))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.reject(&reject_id, cx);
                    this.focus(window, cx);
                })),
            )
            .child(if working {
                ui::working_button(
                    SharedString::from(format!("refine-{id}")),
                    "Refining",
                    roles,
                    cx.reduce_motion(),
                )
            } else {
                ui::sized_button(
                    SharedString::from(format!("refine-{id}")),
                    "Refine",
                    ButtonKind::Default,
                    ButtonSize::Small,
                    ui::Control::focused(is_focused(CardControl::Refine)),
                    roles,
                )
                .child(ui::shortcut_hint(shortcut_hint("K"), false, roles))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.toggle_refine(&refine_id, window, cx);
                }))
            })
    }

    fn render_refine_error(
        &self,
        id: &str,
        error: String,
        active: bool,
        focused: Option<CardControl>,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Div {
        let retry_id = id.to_string();
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(ui::error_text(error, roles))
            .when(active, |this| {
                this.child(
                    ui::row().child(
                        ui::sized_button(
                            SharedString::from(format!("retry-refine-{id}")),
                            "Retry",
                            ButtonKind::Default,
                            ButtonSize::Small,
                            ui::Control::focused(focused == Some(CardControl::RetryRefine)),
                            roles,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| this.retry_refine_of(&retry_id, window, cx),
                        )),
                    ),
                )
            })
    }

    /// The block-level Replacements and the errors of failed Suggestions, each in the rows
    /// the editor reserves below the line its range ends on.
    /// Zones on one line stack in the order the editor reserves their rows. A zone whose
    /// line is above the view is skipped, since the editor has no bounds for it.
    fn render_zones(&self, roles: Roles, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let line_height = self.line_height(cx);
        let mut zones = Vec::new();
        let layout: Vec<(&Suggestion, usize, usize)> = self
            .zone_layout(cx)
            .into_iter()
            .map(|(item, rows)| (item, self.line_of(item.to, cx), rows))
            .collect();
        let lines_and_rows: Vec<(usize, usize)> =
            layout.iter().map(|&(_, line, rows)| (line, rows)).collect();
        for ((item, line, _), before) in layout.into_iter().zip(rows_before(&lines_and_rows)) {
            if self.above_view(item.to, cx) {
                continue;
            }
            let Some((left, top, width)) = self.zone_place(line, cx) else {
                continue;
            };
            let top = top + self.rows_height(before, cx);
            let key = zone_key(&item.id);
            let measure = self.measure_zone(key.clone(), cx);
            let element = match item {
                Suggestion::Ready(ready) => {
                    let Some(text) = block_replacement(&ready.diff) else {
                        continue;
                    };
                    ui::replacement_block(
                        SharedString::from(key),
                        text.to_string(),
                        width,
                        line_height,
                        roles,
                    )
                    .child(measure)
                    .into_any_element()
                }
                Suggestion::Failed(failed) => {
                    let frame = ui::dialog_frame(SharedString::from(key), width, roles)
                        .p(px(12.))
                        .gap(px(8.))
                        .child(measure);
                    self.render_failed_card(
                        frame,
                        &item.id,
                        describe_error(&failed.error),
                        roles,
                        cx,
                    )
                    .into_any_element()
                }
                Suggestion::Pending(_) => continue,
            };
            zones.push(
                div()
                    .absolute()
                    .left(left)
                    .top(top)
                    .child(element)
                    .into_any_element(),
            );
        }
        zones
    }

    /// Where the first zone under a buffer line goes, relative to the pane: under its last
    /// row, from the text's left edge to the column's right edge less the same inset, so an
    /// indented line's zone stays centered on its text.
    fn zone_place(&self, line: usize, cx: &App) -> Option<(Pixels, Pixels, Pixels)> {
        let (line_end, line_start) = self.line_bounds(line, cx)?;
        let origin = self.origin();
        let left = line_start.left();
        let pane = self.bounds.get();
        let column_left = pane.origin.x
            + (pane.size.width - crate::ui::COLUMN_WIDTH.min(pane.size.width)) / 2.
            + px(COLUMN_PADDING);
        let inset = (left - column_left).max(px(0.));
        let width = (self.content_right() - left - inset).max(px(0.));
        Some((
            left - origin.x,
            line_end.bottom() + px(ZONE_TOP) - origin.y,
            width,
        ))
    }

    fn measure_zone(&self, key: String, cx: &Context<Self>) -> impl IntoElement + use<> {
        let heights = Rc::clone(&self.heights);
        let entity = cx.entity().downgrade();
        ui::measure(move |size, cx| {
            let previous = heights.borrow_mut().insert(key.clone(), size.height);
            if previous != Some(size.height)
                && let Some(entity) = entity.upgrade()
            {
                entity.update(cx, |this, cx| {
                    this.sync_zones(cx);
                    cx.notify();
                });
            }
        })
    }

    /// Where the line holding `offset` sits against the view, or `None` before
    /// the editor's first layout.
    pub(crate) fn line_at(&self, offset: usize, cx: &App) -> Option<LineAt> {
        let state = self.editor.read(cx);
        let rows = state.visible_row_range()?;
        let first_shown = component::RopeExt::line_start_offset(state.text(), rows.start);
        let origin = self.bounds.get().origin;
        let bounds = state.range_to_bounds(&(offset..offset)).map(|line| Rect {
            left: to_f64(line.left() - origin.x),
            right: to_f64(line.left() - origin.x),
            top: to_f64(line.top() - origin.y),
            bottom: to_f64(line.bottom() - origin.y),
        });
        Some(LineAt::locate(offset, first_shown, bounds))
    }

    fn above_view(&self, offset: usize, cx: &App) -> bool {
        let state = self.editor.read(cx);
        state.visible_row_range().is_some_and(|rows| {
            rows.start > 0
                && offset < component::RopeExt::line_start_offset(state.text(), rows.start)
        })
    }

    /// Whether the pointer is over a Card that takes clicks, which hides the text under it.
    pub(crate) fn over_card(&self, position: Point<Pixels>) -> bool {
        self.card_rects
            .borrow()
            .iter()
            .any(|(rect, kind)| *kind != CardKind::Hover && rect.contains(&position))
    }

    fn hovered_at(&self, position: Point<Pixels>, cx: &App) -> Option<String> {
        if self.over_card(position) {
            return None;
        }
        let state = self.editor.read(cx);
        let column_left = self.content_left();
        let column_right = self.content_right();
        self.suggestions
            .items
            .iter()
            .filter(|item| matches!(item, Suggestion::Ready(_)))
            .filter(|item| !self.above_view(item.to, cx))
            .filter(|item| {
                let (Some(from), Some(to)) = (
                    state.range_to_bounds(&(item.from..item.from)),
                    state.range_to_bounds(&(item.to..item.to)),
                ) else {
                    return false;
                };
                hits_text_range(position, from, to, column_left..column_right)
            })
            .max_by_key(|item| item.sequence)
            .map(|item| item.id.clone())
    }

    fn track_hover(&mut self, position: Option<Point<Pixels>>, cx: &mut Context<Self>) {
        let next = position
            .filter(|position| self.bounds.get().contains(position))
            .and_then(|position| self.hovered_at(position, cx));
        let spelling = self.hover_spelling(
            position.filter(|position| self.bounds.get().contains(position)),
            cx,
        );
        if next != self.hovered || spelling {
            self.hovered = next;
            cx.notify();
        }
    }

    /// Where the AI caret belongs this frame, for the view that draws it.
    pub(crate) fn ai_caret_place(&self, cx: &App) -> AiCaretPlace {
        let Some(caret) = ai_caret_of(&self.suggestions, self.busy().is_some()) else {
            return AiCaretPlace::Gone;
        };
        if self.above_view(caret.pos, cx) {
            return AiCaretPlace::Hidden;
        }
        let editor = self.editor.read(cx);
        let Some(bounds) = editor.range_to_bounds(&(caret.pos..caret.pos)) else {
            return AiCaretPlace::Hidden;
        };
        let pane = self.bounds.get();
        let spot = |bounds: Bounds<Pixels>| {
            let place = bounds.origin - pane.origin;
            (f32::from(place.x), f32::from(place.y))
        };
        let head = editor.cursor();
        let flag_width =
            px(40. + 7. * caret.label.chars().count() as f32).min(ui::AI_CARET_FLAG_MAX_WIDTH);
        AiCaretPlace::At(AiCaretAt {
            target: caret.target,
            spot: spot(bounds),
            // Above the view, `range_to_bounds` gives the corner of the first
            // line on screen instead of nothing: no takeoff, the caret shows in place.
            takeoff: (!self.above_view(head, cx))
                .then(|| editor.range_to_bounds(&(head..head)))
                .flatten()
                .map(spot),
            height: bounds.size.height,
            flipped: bounds.left() + flag_width > pane.origin.x + pane.size.width,
            label: caret.label,
            area: pane.size,
        })
    }

    fn bubble_view(&self, window: &Window, cx: &App) -> Option<BubbleAnchor> {
        let selection = self.editor.read(cx).selected_range();
        let focused = self.pane_focused(window, cx);
        let empty = selection.is_empty();
        if !focused || (!self.bubble.ask_open && empty) {
            return None;
        }
        let target = self.current_target(cx)?;
        let view = BubbleView {
            focused,
            pointer_down: self.pointer_down.get(),
            selection: BubbleRange {
                from: selection.start,
                to: selection.end,
            },
            has_target: true,
        };
        if !bubble_visible(self.bubble, view) {
            return None;
        }
        let reversed = self.editor.read(cx).is_selection_reversed();
        Some(if empty {
            BubbleAnchor {
                above: true,
                head: target.from,
                span: target.from..target.to,
            }
        } else {
            BubbleAnchor {
                above: reversed,
                head: if reversed {
                    selection.start
                } else {
                    selection.end
                },
                span: selection,
            }
        })
    }

    fn render_bubble(
        &self,
        anchor: BubbleAnchor,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.last_bubble.replace(None);
        let ends = BubbleEnds {
            head: self.line_at(anchor.head, cx)?,
            start: self.line_at(anchor.span.start, cx)?,
            end: self.line_at(anchor.span.end, cx)?,
        };
        let pane = self.bounds.get();
        let area = area_of(pane);
        let column_left = to_f64(self.content_left() - pane.origin.x);
        let height = self
            .heights
            .borrow()
            .get(BUBBLE_KEY)
            .copied()
            .unwrap_or(px(120.));
        let placed = place_popover(&PopoverInput {
            anchor: bubble_anchor(ends, column_left, &area),
            width: f64::from(BUBBLE_WIDTH),
            height: to_f64(height),
            area,
            side: if anchor.above {
                Side::Above
            } else {
                Side::Below
            },
            flip: true,
            gap: 8.,
        })?;
        let heights = Rc::clone(&self.heights);
        let entity = cx.entity().downgrade();
        let actions = self.visible_actions();
        let rows = ask_menu::menu_rows(&actions, &self.menu);
        let hint = ask_menu::hint_visible(&self.menu)
            .then(|| ask_menu::ask_hint(&actions))
            .flatten();
        let hint_text = hint.clone().map(SharedString::from);
        let bubble = ui::popover_frame("bubble", roles)
            .key_context("AskBubble")
            .on_action(cx.listener(Self::bubble_up))
            .on_action(cx.listener(Self::bubble_down))
            .on_action(cx.listener(Self::bubble_confirm))
            .on_action(cx.listener(Self::bubble_cancel))
            .on_action(cx.listener(|_, _: &Stay, _, _| {}))
            .absolute()
            .left(px(placed.left as f32))
            .top(px(placed.top as f32))
            .w(px(BUBBLE_WIDTH))
            .child(ui::measure(move |size, cx| {
                let previous = heights
                    .borrow_mut()
                    .insert(BUBBLE_KEY.to_string(), size.height);
                if previous != Some(size.height)
                    && let Some(entity) = entity.upgrade()
                {
                    entity.update(cx, |_, cx| cx.notify());
                }
            }));
        let live_rows = self.bubble_rows(&rows, roles, cx);
        let bubble = bubble_body(
            bubble,
            Input::new(&self.bubble_input)
                .appearance(false)
                .into_any_element(),
            hint.map(SharedString::from),
            live_rows,
            roles,
        );
        self.remember_bubble(&placed, &rows, hint_text, height, cx);
        Some(bubble_in(bubble, placed.side == Side::Above))
    }

    fn bubble_rows(
        &self,
        rows: &[AskRow],
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        rows.iter()
            .cloned()
            .enumerate()
            .map(|(index, row)| {
                ui::enter_in_turn(
                    ("ask-row-in", index),
                    self.render_ask_row(index, row, roles, cx),
                    ui::Enter::Lift(-4.),
                    index,
                )
                .into_any_element()
            })
            .collect()
    }

    fn remember_bubble(
        &self,
        placed: &Placed,
        rows: &[AskRow],
        hint: Option<SharedString>,
        height: Pixels,
        cx: &App,
    ) {
        self.last_bubble.replace(Some(BubbleGhost {
            left: px(placed.left as f32),
            top: px(placed.top as f32),
            height,
            above: placed.side == Side::Above,
            query: self.bubble_input.read(cx).value(),
            hint,
            rows: rows
                .iter()
                .map(|row| GhostRow {
                    label: ask_row_label(row).into(),
                    skill: matches!(row, AskRow::Action { action, .. } if action.skill_only),
                })
                .collect(),
            active: self.menu.active,
        }));
    }

    fn start_bubble_leave(&mut self, cx: &App) {
        let Some(ghost) = self.last_bubble.take() else {
            return;
        };
        self.bubble_leaving = ui::Presence::present(ghost);
        self.bubble_leaving.hide(ui::bubble_out(), cx);
    }

    fn render_ask_row(
        &self,
        index: usize,
        row: AskRow,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let label = ask_row_label(&row);
        let (skill, span) = match &row {
            AskRow::Action { action, match_span } => (action.skill_only, *match_span),
            AskRow::Free { .. } => (false, None),
        };
        let text = match span {
            Some((from, to)) if to <= label.len() => StyledText::new(label.clone())
                .with_highlights(vec![(
                    from..to,
                    HighlightStyle {
                        underline: Some(UnderlineStyle {
                            thickness: px(1.),
                            color: None,
                            wavy: false,
                        }),
                        ..Default::default()
                    },
                )])
                .into_any_element(),
            _ => StyledText::new(label.clone()).into_any_element(),
        };
        ui::ask_option_row(("ask-row", index), index == self.menu.active, roles)
            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                let next = ask_menu::hover_ask_menu(&this.menu, index);
                if next != this.menu {
                    this.menu = next;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.menu = ask_menu::hover_ask_menu(&this.menu, index);
                this.bubble_confirm(&BubbleConfirm, window, cx);
            }))
            .child(ui::ask_option_label(text))
            .when(skill, |this| this.child(ui::meta("Skill", roles)))
    }
}

fn accepted_change(ready: &ReadySuggestion) -> AcceptedChange {
    AcceptedChange {
        origin: ready.origin,
        instruction: instruction_label(&ready.instruction).to_string(),
        rationale: ready.rationale.clone(),
    }
}

impl EditorPane {
    fn render_cards(
        &mut self,
        window: &Window,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let active_id = self.active(cx).map(|item| item.id.clone());
        let card_focused = self.card_focus.is_focused(window);
        let mut items = self.suggestions.items.clone();
        items.sort_by_key(|item| item.from);
        let shown: HashMap<String, CardKind> = shown_cards(
            items
                .iter()
                .map(|item| (item.id.as_str(), card_status(item))),
            active_id.as_deref(),
            self.hovered.as_deref(),
        )
        .into_iter()
        .map(|(id, kind)| (id.to_string(), kind))
        .collect();
        self.card_rects.borrow_mut().clear();
        self.drawn_cards.borrow_mut().clear();
        let mut cards: Vec<AnyElement> = Vec::new();
        let mut floor = px(0.);
        let is_hover = |item: &&Suggestion| shown.get(&item.id) == Some(&CardKind::Hover);
        let hover_last = items
            .iter()
            .filter(|item| !is_hover(item))
            .chain(items.iter().filter(is_hover));
        for item in hover_last {
            let Some(kind) = shown.get(&item.id).copied() else {
                continue;
            };
            if let Some((card, next)) = self.render_card(item, kind, floor, card_focused, roles, cx)
            {
                cards.push(card);
                floor = next;
            }
        }
        cards
    }

    fn track_bounds(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let bounds = Rc::clone(&self.bounds);
        let entity = cx.entity().downgrade();
        let release = Rc::clone(&self.pointer_down);
        let release_entity = entity.clone();
        canvas(
            move |next, _, cx| {
                if bounds.get() != next {
                    bounds.set(next);
                    if let Some(entity) = entity.upgrade() {
                        entity.update(cx, |_, cx| cx.notify());
                    }
                }
            },
            move |_, (), window, _| {
                let release = Rc::clone(&release);
                let entity = release_entity.clone();
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture
                        && release.replace(false)
                        && let Some(entity) = entity.upgrade()
                    {
                        entity.update(cx, |_, cx| cx.notify());
                    }
                });
            },
        )
        .absolute()
        .size_full()
    }
}

impl EditorPane {
    /// Where the bubble shows this frame; starts its exit when it has just closed.
    fn settle_bubble(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<BubbleAnchor> {
        let bubble = self.bubble_view(window, cx);
        let shown = bubble.is_some();
        if self.bubble_shown && !shown {
            self.clear_bubble(window, cx);
            self.start_bubble_leave(cx);
        }
        if shown {
            self.bubble_leaving = ui::Presence::absent();
        }
        self.bubble_shown = shown;
        bubble
    }
}

impl Render for EditorPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        perf_span!("pane.render");
        let roles = theme::roles(cx);
        if std::mem::take(&mut self.reveal_pending) {
            self.reveal_first_review_suggestion(window, cx);
        }
        let bubble = self.settle_bubble(window, cx);
        let zones = self.render_zones(roles, cx);
        let cards = self.render_cards(window, roles, cx);
        let bubble = bubble
            .and_then(|anchor| self.render_bubble(anchor, roles, cx))
            .or_else(|| {
                self.bubble_leaving
                    .frame(window, cx)
                    .map(|shown| render_bubble_ghost(shown.content, shown.progress, roles))
            });
        let press = Rc::clone(&self.pointer_down);
        let marks = self.render_note_marks(roles, cx);
        let note_card = self.render_note_card(roles, cx);
        let panel = self.render_notes_panel(roles, cx);
        let map = self.render_map_and_chips(roles, cx);
        let tray = self.render_jobs_tray(roles, window, cx);
        let (spell_layer, spell_menu) = self.render_spelling(window, roles, cx);
        let text = div()
            .id("editor-pane")
            .key_context("EditorPane")
            .on_action(cx.listener(Self::ask))
            .on_action(cx.listener(Self::accept_active))
            .on_action(cx.listener(Self::escape_at_cursor))
            .on_action(cx.listener(Self::previous_version))
            .on_action(cx.listener(Self::next_version))
            .on_action(cx.listener(Self::next_suggestion))
            .on_action(cx.listener(Self::previous_suggestion))
            .on_action(cx.listener(Self::focus_card))
            .on_action(cx.listener(Self::focus_card_back))
            .on_action(cx.listener(Self::pane_undo))
            .on_action(cx.listener(Self::pane_redo))
            .capture_key_down(cx.listener(Self::spell_key_down))
            .on_action(cx.listener(Self::spell_menu_ctrl_period))
            .on_action(cx.listener(Self::spell_menu_alt_enter))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::spell_mouse_up))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                this.drag_map(event, cx);
                this.track_hover(Some(event.position), cx);
            }))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, _| {
                this.end_map_drag();
            }))
            .on_hover(cx.listener(|this, _: &bool, window, cx| {
                this.track_hover(Some(window.mouse_position()), cx);
            }))
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(self.track_bounds(cx))
            .child(
                div().size_full().flex().justify_center().child(
                    div()
                        .id("editor-column")
                        .w(crate::ui::COLUMN_WIDTH)
                        .max_w_full()
                        .h_full()
                        .px(px(COLUMN_PADDING))
                        .capture_any_mouse_down(move |_, _, _| press.set(true))
                        .child(Editor::new(&self.editor).h_full().bordered(false)),
                ),
            )
            .when(!zones.is_empty(), |this| {
                this.child(div().absolute().inset_0().overflow_hidden().children(zones))
            })
            .child(spell_layer)
            .child(ui::caret_layer(
                self.editor.clone(),
                caret::settings(cx),
                (
                    self.caret.shown(),
                    suggestion_at(&self.suggestions, self.cursor(cx)).is_some(),
                ),
                roles,
            ))
            .children(map)
            .children(marks)
            .children(note_card)
            .children(tray)
            .children(cards)
            .children(bubble)
            .children(spell_menu);
        div()
            .size_full()
            .flex()
            .flex_row()
            .child(text)
            .children(panel)
    }
}

/// Where the AI caret belongs in the pane this frame.
pub(crate) enum AiCaretPlace {
    /// There is no AI caret.
    Gone,
    /// There is one, out of view.
    Hidden,
    At(AiCaretAt),
}

pub(crate) struct AiCaretAt {
    /// The Suggestion the caret marks, if it marks one yet.
    pub target: Option<String>,
    pub spot: Spot,
    /// The writer's caret, where a new AI caret takes off from.
    pub takeoff: Option<Spot>,
    pub height: Pixels,
    pub label: String,
    /// The flag opens to the left, at the pane's right edge.
    pub flipped: bool,
    /// The pane's size: the caret is clipped to it.
    pub area: Size<Pixels>,
}

pub(crate) fn to_f64(value: Pixels) -> f64 {
    f64::from(f32::from(value))
}

pub(crate) fn area_of(pane: Bounds<Pixels>) -> Viewport {
    Viewport {
        width: to_f64(pane.size.width),
        height: to_f64(pane.size.height),
    }
}

fn ask_row_label(row: &AskRow) -> String {
    match row {
        AskRow::Action { action, .. } => action.label.clone(),
        AskRow::Free { text } => format!("Ask AI: “{text}”"),
    }
}

fn bubble_body<E: ParentElement + Styled>(
    frame: E,
    query: AnyElement,
    hint: Option<SharedString>,
    rows: Vec<AnyElement>,
    roles: Roles,
) -> E {
    let frame = frame.p(px(8.)).gap(px(8.)).child(
        ui::row()
            .gap(px(8.))
            .child(ui::ai_flag(roles))
            .child(div().flex_1().child(query)),
    );
    let frame = match hint {
        Some(hint) => frame.child(ui::meta(hint, roles)),
        None => frame,
    };
    frame.children(rows)
}

fn render_bubble_ghost(ghost: &BubbleGhost, progress: f32, roles: Roles) -> AnyElement {
    let rows = ghost
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            ui::leaving_ask_option_row(("ghost-row", index), index == ghost.active, roles)
                .child(ui::ask_option_label(row.label.clone()))
                .when(row.skill, |this| this.child(ui::meta("Skill", roles)))
                .into_any_element()
        })
        .collect();
    let frame = bubble_body(
        ui::popover_surface(roles)
            .absolute()
            .left(ghost.left)
            .top(ghost.top)
            .w(px(BUBBLE_WIDTH))
            .h(ghost.height)
            .overflow_hidden(),
        div()
            .min_h(px(20.))
            .child(ghost.query.clone())
            .into_any_element(),
        ghost.hint.clone(),
        rows,
        roles,
    );
    ui::risen(frame, if ghost.above { 6. } else { -6. }, progress).into_any_element()
}

fn bubble_in(bubble: Stateful<Div>, above: bool) -> AnyElement {
    ui::enter(
        "bubble-in",
        bubble,
        ui::Enter::Rise(if above { 6. } else { -6. }),
        ui::MOTION_ENTER,
    )
    .into_any_element()
}

fn card_status(item: &Suggestion) -> CardStatus {
    match item {
        Suggestion::Pending(_) => CardStatus::Pending,
        Suggestion::Ready(_) => CardStatus::Ready,
        Suggestion::Failed(_) => CardStatus::Failed,
    }
}

pub(crate) fn hits_text_range(
    position: Point<Pixels>,
    from: Bounds<Pixels>,
    to: Bounds<Pixels>,
    column: Range<Pixels>,
) -> bool {
    let (x, y) = (position.x, position.y);
    if y < from.top() || y >= to.bottom() {
        return false;
    }
    let on_first = y < from.bottom();
    let on_last = y >= to.top();
    let start = if on_first { from.left() } else { column.start };
    let end = if on_last { to.left() } else { column.end };
    x >= start && x <= end
}

#[cfg(test)]
mod tests {
    use super::key_bindings;
    use gpui_kit::{KeyContext, Keymap, Keystroke};

    fn stack(names: &[&str]) -> Vec<KeyContext> {
        names
            .iter()
            .map(|name| KeyContext::parse(name).unwrap())
            .collect()
    }

    fn bound(key: &'static str, names: &[&str]) -> Vec<&'static str> {
        let keymap = Keymap::new(key_bindings());
        let (bindings, _) =
            keymap.bindings_for_input(&[Keystroke::parse(key).unwrap()], &stack(names));
        bindings
            .iter()
            .map(|binding| binding.action().name())
            .collect()
    }

    const CARD: &[&str] = &["EditorPane", "SuggestionCard"];
    const REFINE_FIELD: &[&str] = &["EditorPane", "SuggestionCard", "RefineInput", "Input"];

    #[test]
    fn space_and_enter_press_the_focused_card_control() {
        assert_eq!(bound("space", CARD), vec!["editor_pane::CardPress"]);
        assert_eq!(bound("enter", CARD), vec!["editor_pane::CardPress"]);
    }

    #[test]
    fn editor_shortcuts_still_work_in_the_text() {
        let text = &["EditorPane", "Input"];
        assert_eq!(
            bound("ctrl-enter", text).first(),
            Some(&"editor_pane::AcceptActive")
        );
        assert_eq!(bound("ctrl-k", text).first(), Some(&"editor_pane::AskAi"));
        assert_eq!(
            bound("ctrl-h", CARD).first(),
            Some(&"editor_pane::PreviousVersion")
        );
    }

    #[test]
    fn arrows_move_between_card_controls_like_tab() {
        for key in ["tab", "right", "down"] {
            assert_eq!(bound(key, CARD), vec!["editor_pane::CardNext"], "{key}");
        }
        for key in ["shift-tab", "left", "up"] {
            assert_eq!(bound(key, CARD), vec!["editor_pane::CardPrevious"], "{key}");
        }
    }

    #[test]
    fn arrows_move_the_caret_in_the_refine_field() {
        for key in ["left", "right", "up", "down"] {
            assert!(
                bound(key, REFINE_FIELD)
                    .iter()
                    .all(|name| !name.starts_with("editor_pane::")),
                "{key}"
            );
        }
    }

    #[test]
    fn space_types_in_the_refine_field() {
        assert!(bound("space", REFINE_FIELD).is_empty());
    }

    #[test]
    fn refine_field_keys_stay_in_the_field() {
        assert_eq!(
            bound("enter", REFINE_FIELD).first(),
            Some(&"editor_pane::RefineConfirm")
        );
        assert_eq!(
            bound("escape", REFINE_FIELD).first(),
            Some(&"editor_pane::RefineCancel")
        );
        assert!(!bound("enter", REFINE_FIELD).contains(&"editor_pane::CardPress"));
        for key in ["ctrl-enter", "ctrl-k", "ctrl-h", "ctrl-l", "alt-[", "alt-]"] {
            let actions = bound(key, REFINE_FIELD);
            assert!(
                actions
                    .iter()
                    .all(|name| !name.starts_with("editor_pane::")),
                "{key} reaches {actions:?}"
            );
        }
        for key in ["a", "r", "shift-a"] {
            assert!(bound(key, REFINE_FIELD).is_empty(), "{key} is bound");
        }
    }
}
