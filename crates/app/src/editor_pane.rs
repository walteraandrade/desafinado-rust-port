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
            BubbleChange, BubbleRange, BubbleState, BubbleView, bubble_visible, next_bubble,
        },
        ask_menu::{self, AskMenu, AskRow},
        card_placement::{CardPlacement, MARGIN_CARD_WIDTH, placement_for},
        focus_cycle::cycle_index,
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
        Block, BlockKind, Selection, Target, extract_blocks, find_frontmatter, get_neighbors,
        resolve_target, target_from_range,
    },
    change_set::{ChangeSet, TextChange},
    diff::DiffHunk,
    suggestions::{
        ClientError, Direction, Instruction, PlaceReviewEditInput, ReadySuggestion, RefineState,
        ReviewPlacement, Suggestion, SuggestionOrigin, SuggestionsState, accept_all_plan,
        accept_suggestions, active_suggestion, begin_refine, begin_suggestion, cancel_refine,
        change_label, change_ranges, describe_error, empty_suggestions, fail_refine,
        fail_suggestion, find_suggestion, first_review_suggestion, instruction_history,
        instruction_label, map_suggestions, newly_failed, notice_message, occupied_block_ids,
        open_review_count, place_review_edit, refine_task_label, reject_review_suggestions,
        reject_suggestion, resolve_refine, resolve_suggestion, restore_suggestion, retry_refine,
        retry_suggestion, review_notice_text, review_summary, review_task_label, review_too_large,
        review_too_large_text, show_version, shows_changes, step_change, step_suggestion,
        suggestion_at, task_label,
    },
};
use futures::{StreamExt as _, channel::mpsc};
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
    theme::{self, Roles},
    ui::{self, ButtonKind, ButtonSize},
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

const COLUMN_WIDTH: f32 = 768.;
const COLUMN_PADDING: f32 = 24.;
const MARGIN_GAP: f32 = 24.;

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-shift-z", Redo, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-y", Redo, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-z", Undo, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-shift-z", PaneRedo, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-y", PaneRedo, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-z", PaneUndo, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-k", AskAi, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-enter", AcceptActive, Some("EditorPane > Input")),
        KeyBinding::new("escape", EscapeAtCursor, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-h", PreviousVersion, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-l", NextVersion, Some("EditorPane > Input")),
        KeyBinding::new("alt-]", NextSuggestion, Some("EditorPane > Input")),
        KeyBinding::new("alt-[", PreviousSuggestion, Some("EditorPane > Input")),
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
        KeyBinding::new("enter", CardPress, Some("SuggestionCard")),
        KeyBinding::new("space", CardPress, Some("SuggestionCard")),
        KeyBinding::new("escape", CardLeave, Some("SuggestionCard")),
        KeyBinding::new("ctrl-h", PreviousVersion, Some("SuggestionCard")),
        KeyBinding::new("ctrl-l", NextVersion, Some("SuggestionCard")),
    ]);
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

pub struct EditorPane {
    pub editor: Entity<EditorState>,
    pub suggestions: SuggestionsState,
    strike: TextDecorationCollection,
    fills: RangeDecorationCollection,
    bubble_input: Entity<InputState>,
    bubble: BubbleState,
    menu: AskMenu,
    bubble_shown: bool,
    pointer_down: Rc<Cell<bool>>,
    last_selection: Range<usize>,
    refine: Option<RefineInput>,
    card_focus: FocusHandle,
    card_index: usize,
    applying: Option<Applying>,
    tasks: HashMap<String, Task<()>>,
    next_id: usize,
    next_review_id: usize,
    rejected: Vec<ReadySuggestion>,
    restored: Vec<ReadySuggestion>,
    pub quick_actions: Vec<QuickAction>,
    pub is_skill: bool,
    ai: Arc<dyn AiBackend>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    heights: Rc<RefCell<HashMap<String, Pixels>>>,
    pub review: Option<ReviewRun>,
    reveal_pending: bool,
    _subscriptions: Vec<Subscription>,
}

pub struct ReviewRun {
    pub added: usize,
    focus: String,
    stop: Arc<AtomicBool>,
    changes: ChangeSet,
    _task: Task<()>,
}

fn edit_change(edit_start: usize, old_end: usize, new_end: usize, after: &str) -> ChangeSet {
    let inserted = after
        .get(edit_start..new_end)
        .unwrap_or_default()
        .to_string();
    let before_len = after.len() + (old_end - edit_start) - (new_end - edit_start);
    ChangeSet::of(
        &[TextChange::new(edit_start, old_end, inserted)],
        before_len,
    )
}

fn has_room_for_review(blocks: &[Block], occupied: &std::collections::BTreeSet<String>) -> bool {
    blocks
        .iter()
        .any(|block| block.kind != BlockKind::Code && !occupied.contains(&block.id))
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
        let edits = cx.subscribe(&editor, |this, editor, event: &InputEditsEvent, cx| {
            let frontmatter = find_frontmatter(&editor.read(cx).value());
            let applying = this.applying.take();
            let mut next = this.suggestions.clone();
            for (edit, rope) in event.edits() {
                let after = rope.to_string();
                let changes = edit_change(
                    edit.start_byte,
                    edit.old_end_byte,
                    edit.new_end_byte,
                    &after,
                );
                if let Some(review) = this.review.as_mut() {
                    review.changes = review.changes.compose(&changes);
                }
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
            this.rejected.clear();
            this.restored.clear();
            this.set_suggestions(next, cx);
            cx.emit(PaneEvent::Changed);
        });
        let moves = cx.observe(&editor, |this, editor, cx| {
            let selection = editor.read(cx).selected_range();
            if selection != this.last_selection {
                this.last_selection = selection.clone();
                this.bubble = next_bubble(
                    this.bubble,
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
                this.refine = None;
                this.refresh_decorations(cx);
                cx.notify();
            }
        });
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
        Self {
            editor,
            suggestions: empty_suggestions(),
            strike,
            fills,
            bubble_input,
            bubble: BubbleState::default(),
            menu: AskMenu::default(),
            bubble_shown: false,
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
            review: None,
            reveal_pending: false,
            _subscriptions: vec![edits, moves, typing],
        }
    }

    pub fn text(&self, cx: &App) -> String {
        self.editor.read(cx).value().to_string()
    }

    fn set_suggestions(&mut self, next: SuggestionsState, cx: &mut Context<Self>) {
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
                actions: Vec::new(),
            });
        }
        for (id, text) in failed {
            cx.emit(PaneEvent::Toast {
                text,
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
        self.tasks.clear();
        self.suggestions = empty_suggestions();
        self.bubble = BubbleState::default();
        self.refine = None;
        self.rejected.clear();
        self.restored.clear();
    }

    pub fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.reset(cx);
        self.editor.update(cx, |state, cx| {
            state.set_value(text.to_string(), window, cx)
        });
        self.suggestions = empty_suggestions();
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
            state.replace_all(text.to_string(), window, cx)
        });
        self.suggestions = empty_suggestions();
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

    pub fn refresh_decorations(&mut self, cx: &mut Context<Self>) {
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
                            color: Some(roles.faint),
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
            .filter(|item| item.to > item.from)
            .map(|item| {
                let active = Some(&item.id) == active_id.as_ref();
                let decoration = RangeDecoration::new(item.from..item.to);
                match item {
                    Suggestion::Ready(ready)
                        if matches!(ready.refine, RefineState::Failed { .. }) =>
                    {
                        decoration
                            .with_style(RangeDecorationStyle::Frame)
                            .with_color(roles.danger)
                    }
                    Suggestion::Ready(_) if active => decoration
                        .with_style(RangeDecorationStyle::Fill)
                        .with_color(roles.ai_fill),
                    Suggestion::Ready(_) => decoration
                        .with_style(RangeDecorationStyle::Fill)
                        .with_color(roles.fill),
                    Suggestion::Pending(_) => decoration
                        .with_style(RangeDecorationStyle::Frame)
                        .with_color(roles.ai),
                    Suggestion::Failed(_) => decoration
                        .with_style(RangeDecorationStyle::Frame)
                        .with_color(roles.danger),
                }
            })
            .collect();
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
        self.strike.set(strikes, cx);
        self.fills.set(fills, cx);
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
        self.clear_bubble(window, cx);
        self.ask_with(instruction, window, cx);
        self.focus(window, cx);
    }

    fn ask_with(&mut self, instruction: Instruction, window: &mut Window, cx: &mut Context<Self>) {
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
        let _ = window;
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
        let ai = self.ai.clone();
        let task_id = id.clone();
        let work = cx.background_spawn(async move { ai.target(call) });
        let task = cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |this, cx| {
                this.finish(&task_id, result, refine, cx);
            });
        });
        self.tasks.insert(id, task);
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

    pub fn start_review(&mut self, focus: String, cx: &mut Context<Self>) {
        if self.review.is_some() {
            return;
        }
        let text = self.text(cx);
        if review_too_large(text.len()) {
            cx.emit(PaneEvent::Toast {
                text: review_too_large_text(text.len()),
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
        let ai = self.ai.clone();
        let flag = stop.clone();
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

    fn stop_review_quietly(&mut self, _: &mut Context<Self>) {
        if let Some(run) = self.review.take() {
            run.stop.store(true, Ordering::SeqCst);
        }
    }

    pub fn stop_review(&mut self, cx: &mut Context<Self>) {
        let Some(run) = self.review.take() else {
            return;
        };
        run.stop.store(true, Ordering::SeqCst);
        cx.emit(PaneEvent::Toast {
            text: review_summary(run.added, true),
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
            Some(Suggestion::Ready(ready)) => self.accept(&ready.id, window, cx),
            Some(_) => {}
        }
    }

    fn escape_at_cursor(
        &mut self,
        _: &EscapeAtCursor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        let cursor = self.cursor(cx);
        match step_suggestion(&self.suggestions, cursor, direction) {
            Some(position) => {
                self.editor.update(cx, |state, cx| {
                    state.set_selected_range(position..position, cx);
                    state.focus(window, cx);
                });
                cx.notify();
            }
            None => cx.propagate(),
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

    fn step_change(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active) = self.active(cx) else {
            return;
        };
        let ranges = change_ranges(active.from, &active.diff);
        if let Some(target) = step_change(&ranges, self.cursor(cx), direction) {
            self.editor.update(cx, |state, cx| {
                state.set_selected_range(target..target, cx);
            });
            let _ = window;
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
                refine.input.update(cx, |state, cx| state.focus(window, cx))
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
            CardControl::PreviousChange => self.step_change(Direction::Backward, window, cx),
            CardControl::NextChange => self.step_change(Direction::Forward, window, cx),
            CardControl::Accept => {
                self.accept(&id, window, cx);
                self.focus(window, cx);
            }
            CardControl::Reject => {
                self.reject(&id, cx);
                self.focus(window, cx);
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

    fn content_right(&self) -> Pixels {
        let pane = self.bounds.get();
        let column = px(COLUMN_WIDTH).min(pane.size.width);
        pane.origin.x + (pane.size.width + column) / 2. - px(COLUMN_PADDING)
    }

    fn card_placement(&self) -> CardPlacement {
        let pane = self.bounds.get();
        let right = f32::from(self.content_right() - pane.origin.x) as f64;
        placement_for(right, f32::from(pane.size.width) as f64)
    }

    #[allow(clippy::too_many_arguments)]
    fn card_button(
        &self,
        id: SharedString,
        label: &str,
        hint: Option<String>,
        kind: ButtonKind,
        disabled: bool,
        focused: bool,
        roles: Roles,
    ) -> Stateful<Div> {
        let primary = kind == ButtonKind::Primary;
        ui::sized_button(
            id,
            label.to_string(),
            kind,
            ButtonSize::Small,
            disabled,
            roles,
        )
        .when_some(hint, |this, hint| {
            this.child(
                div()
                    .ml(px(6.))
                    .text_size(px(11.))
                    .font_weight(FontWeight::NORMAL)
                    .text_color(if primary { roles.canvas } else { roles.faint })
                    .child(hint),
            )
        })
        .when(focused, |this| this.border_2().border_color(roles.accent))
    }

    fn render_card(
        &self,
        item: &Suggestion,
        active: bool,
        floor: Pixels,
        card_focused: bool,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Option<(AnyElement, Pixels)> {
        if item.is_pending() {
            return None;
        }
        let state = self.editor.read(cx);
        let anchor_at = match item {
            Suggestion::Ready(ready) => {
                desafinado_core::suggestions::first_change_at(ready.from, &ready.diff)
            }
            _ => item.from,
        };
        let line = state.range_to_bounds(&(anchor_at..anchor_at))?;
        let pane = self.bounds.get();
        let width = px(MARGIN_CARD_WIDTH as f32);
        let (left, top) = match self.card_placement() {
            CardPlacement::Margin => (
                self.content_right() + px(MARGIN_GAP) - pane.origin.x,
                line.top() - pane.origin.y,
            ),
            CardPlacement::Floating => {
                let range = state.range_to_bounds(&(item.to..item.to)).unwrap_or(line);
                (
                    (self.content_right() - width - pane.origin.x).max(px(8.)),
                    range.bottom() + px(8.) - pane.origin.y,
                )
            }
        };
        let top = top.max(floor);
        let id = item.id.clone();
        let height = self.heights.borrow().get(&id).copied().unwrap_or(px(96.));
        let heights = self.heights.clone();
        let measured_id = id.clone();
        let entity = cx.entity().downgrade();
        let card = ui::dialog_frame(SharedString::from(format!("card-{id}")), width, roles)
            .absolute()
            .left(left)
            .top(top)
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let previous = heights
                            .borrow_mut()
                            .insert(measured_id.clone(), bounds.size.height);
                        if previous != Some(bounds.size.height)
                            && let Some(entity) = entity.upgrade()
                        {
                            entity.update(cx, |_, cx| cx.notify());
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .p(px(10.))
            .gap(px(8.))
            .text_size(px(13.));
        let element = match item {
            Suggestion::Pending(_) => return None,
            Suggestion::Failed(failed) => {
                let retry_id = id.clone();
                let dismiss_id = id.clone();
                card.border_color(roles.danger)
                    .bg(roles.danger_fill)
                    .child(
                        div()
                            .text_color(roles.danger)
                            .child(describe_error(&failed.error)),
                    )
                    .child(
                        ui::row()
                            .gap(px(6.))
                            .child(
                                ui::sized_button(
                                    SharedString::from(format!("retry-{id}")),
                                    "Retry",
                                    ButtonKind::Default,
                                    ButtonSize::Small,
                                    false,
                                    roles,
                                )
                                .on_click(cx.listener(
                                    move |this, _, window, cx| this.retry(&retry_id, window, cx),
                                )),
                            )
                            .child(
                                ui::sized_button(
                                    SharedString::from(format!("dismiss-{id}")),
                                    "Dismiss",
                                    ButtonKind::Default,
                                    ButtonSize::Small,
                                    false,
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
            Suggestion::Ready(ready) => {
                let controls = if active {
                    self.card_controls(ready)
                } else {
                    Vec::new()
                };
                let focused_control = card_focused
                    .then(|| controls.get(self.card_index).copied())
                    .flatten();
                let is_focused = |control: CardControl| focused_control == Some(control);
                let replacement = styled_replacement(&ready.diff.hunks, roles);
                let working = matches!(ready.refine, RefineState::Working { .. });
                let refine_error = match &ready.refine {
                    RefineState::Failed { error, .. } => Some(describe_error(error)),
                    _ => None,
                };
                let version = (active && ready.history.len() >= 2).then(|| {
                    let step = &ready.history[ready.shown.min(ready.history.len() - 1)];
                    (
                        ready.shown,
                        ready.history.len(),
                        instruction_label(&step.instruction).to_string(),
                    )
                });
                let changes = (active && shows_changes(ready.from, &ready.original, &ready.diff))
                    .then(|| {
                        change_label(&change_ranges(ready.from, &ready.diff), self.cursor(cx))
                    });
                let refine_open = self
                    .refine
                    .as_ref()
                    .filter(|refine| active && refine.id == ready.id)
                    .map(|refine| refine.input.clone());
                card.when(active, |this| {
                    this.border_color(roles.ai)
                        .track_focus(&self.card_focus)
                        .key_context("SuggestionCard")
                        .on_action(cx.listener(Self::card_next))
                        .on_action(cx.listener(Self::card_previous))
                        .on_action(cx.listener(Self::card_press))
                        .on_action(cx.listener(Self::card_leave))
                        .on_action(cx.listener(Self::previous_version))
                        .on_action(cx.listener(Self::next_version))
                })
                .when_some(version, |this, (shown, count, instruction)| {
                    this.child(
                        ui::row()
                            .gap(px(4.))
                            .child(
                                self.card_button(
                                    SharedString::from(format!("prev-{id}")),
                                    "‹",
                                    None,
                                    ButtonKind::Quiet,
                                    working || shown == 0,
                                    is_focused(CardControl::PreviousVersion),
                                    roles,
                                )
                                .tooltip(|window, cx| {
                                    component::tooltip::Tooltip::new("Previous version (Ctrl+H)")
                                        .build(window, cx)
                                })
                                .on_click(cx.listener(
                                    |this, _, window, cx| {
                                        if let Some(active) = this.active(cx) {
                                            this.press(
                                                CardControl::PreviousVersion,
                                                &active,
                                                window,
                                                cx,
                                            );
                                        }
                                    },
                                )),
                            )
                            .child(format!("{} / {}", shown + 1, count))
                            .child(
                                self.card_button(
                                    SharedString::from(format!("next-{id}")),
                                    "›",
                                    None,
                                    ButtonKind::Quiet,
                                    working || shown + 1 == count,
                                    is_focused(CardControl::NextVersion),
                                    roles,
                                )
                                .tooltip(|window, cx| {
                                    component::tooltip::Tooltip::new("Next version (Ctrl+L)")
                                        .build(window, cx)
                                })
                                .on_click(cx.listener(
                                    |this, _, window, cx| {
                                        if let Some(active) = this.active(cx) {
                                            this.press(
                                                CardControl::NextVersion,
                                                &active,
                                                window,
                                                cx,
                                            );
                                        }
                                    },
                                )),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .pl(px(3.))
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .text_color(roles.faint)
                                    .child(instruction),
                            ),
                    )
                })
                .child(
                    ui::row()
                        .gap(px(6.))
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
                .child(div().text_size(px(14.)).child(replacement))
                .when_some(changes, |this, label| {
                    this.child(
                        ui::row()
                            .gap(px(4.))
                            .child(div().flex_1().text_color(roles.faint).child(label))
                            .child(
                                self.card_button(
                                    SharedString::from(format!("prev-change-{id}")),
                                    "↑",
                                    None,
                                    ButtonKind::Quiet,
                                    false,
                                    is_focused(CardControl::PreviousChange),
                                    roles,
                                )
                                .tooltip(|window, cx| {
                                    component::tooltip::Tooltip::new("Previous change")
                                        .build(window, cx)
                                })
                                .on_click(cx.listener(
                                    |this, _, window, cx| {
                                        this.step_change(Direction::Backward, window, cx)
                                    },
                                )),
                            )
                            .child(
                                self.card_button(
                                    SharedString::from(format!("next-change-{id}")),
                                    "↓",
                                    None,
                                    ButtonKind::Quiet,
                                    false,
                                    is_focused(CardControl::NextChange),
                                    roles,
                                )
                                .tooltip(|window, cx| {
                                    component::tooltip::Tooltip::new("Next change")
                                        .build(window, cx)
                                })
                                .on_click(cx.listener(
                                    |this, _, window, cx| {
                                        this.step_change(Direction::Forward, window, cx)
                                    },
                                )),
                            ),
                    )
                })
                .when(active, |this| {
                    let accept_id = id.clone();
                    let reject_id = id.clone();
                    let refine_id = id.clone();
                    this.child(
                        ui::row()
                            .gap(px(6.))
                            .flex_wrap()
                            .child(
                                self.card_button(
                                    SharedString::from(format!("accept-{id}")),
                                    "Accept",
                                    Some(shortcut_hint("↵")),
                                    ButtonKind::Primary,
                                    false,
                                    is_focused(CardControl::Accept),
                                    roles,
                                )
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.accept(&accept_id, window, cx);
                                        this.focus(window, cx);
                                    },
                                )),
                            )
                            .child(
                                self.card_button(
                                    SharedString::from(format!("reject-{id}")),
                                    "Reject",
                                    Some("Esc".into()),
                                    ButtonKind::Default,
                                    false,
                                    is_focused(CardControl::Reject),
                                    roles,
                                )
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.reject(&reject_id, cx);
                                        this.focus(window, cx);
                                    },
                                )),
                            )
                            .child(if working {
                                self.card_button(
                                    SharedString::from(format!("refine-{id}")),
                                    "Refining…",
                                    None,
                                    ButtonKind::Default,
                                    true,
                                    false,
                                    roles,
                                )
                            } else {
                                self.card_button(
                                    SharedString::from(format!("refine-{id}")),
                                    "Refine",
                                    Some(shortcut_hint("K")),
                                    ButtonKind::Default,
                                    false,
                                    is_focused(CardControl::Refine),
                                    roles,
                                )
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.toggle_refine(&refine_id, window, cx)
                                    },
                                ))
                            }),
                    )
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
                    let retry_id = id.clone();
                    this.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(div().text_color(roles.danger).child(error))
                            .when(active, |this| {
                                this.child(
                                    ui::row().child(
                                        self.card_button(
                                            SharedString::from(format!("retry-refine-{id}")),
                                            "Retry",
                                            None,
                                            ButtonKind::Default,
                                            false,
                                            is_focused(CardControl::RetryRefine),
                                            roles,
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.retry_refine_of(&retry_id, window, cx)
                                            }),
                                        ),
                                    ),
                                )
                            }),
                    )
                })
            }
        };
        Some((element.into_any_element(), top + height + px(8.)))
    }

    fn render_caret(&self, roles: Roles, cx: &App) -> Option<AnyElement> {
        let caret = ai_caret_of(&self.suggestions, self.review.is_some())?;
        let bounds = self
            .editor
            .read(cx)
            .range_to_bounds(&(caret.pos..caret.pos))?;
        let origin = self.origin();
        let pane = self.bounds.get();
        let label = format!("{} ···", caret.label);
        let flag_width = px(14. + 7. * label.chars().count() as f32);
        let flipped = bounds.left() + flag_width > pane.origin.x + pane.size.width;
        let flag = div()
            .absolute()
            .top(px(-18.))
            .when(flipped, |this| this.right(px(0.)))
            .when(!flipped, |this| this.left(px(0.)))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4.))
            .px(px(4.))
            .h(px(16.))
            .bg(roles.ai)
            .text_color(roles.canvas)
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .whitespace_nowrap()
            .child("AI")
            .child(div().font_weight(FontWeight::NORMAL).child(label));
        Some(
            div()
                .absolute()
                .left(bounds.left() - origin.x)
                .top(bounds.top() - origin.y)
                .w(px(2.))
                .h(bounds.size.height)
                .bg(roles.ai)
                .child(flag)
                .into_any_element(),
        )
    }

    fn bubble_view(&self, window: &Window, cx: &App) -> Option<(bool, Range<usize>, bool)> {
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
        let pos = if empty {
            target.from
        } else if reversed {
            selection.start
        } else {
            selection.end
        };
        let above = empty || reversed;
        Some((above, pos..pos, empty))
    }

    fn render_bubble(
        &self,
        above: bool,
        anchor: Range<usize>,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let line = self.editor.read(cx).range_to_bounds(&anchor)?;
        let pane = self.bounds.get();
        let origin = pane.origin;
        let actions = self.visible_actions();
        let rows = ask_menu::menu_rows(&actions, &self.menu);
        let hint = ask_menu::hint_visible(&self.menu)
            .then(|| ask_menu::ask_hint(&actions))
            .flatten();
        let active = self.menu.active;
        let left = (line.left() - origin.x - px(12.))
            .max(px(8.))
            .min((pane.size.width - px(368.)).max(px(8.)));
        Some(
            div()
                .id("bubble")
                .key_context("AskBubble")
                .on_action(cx.listener(Self::bubble_up))
                .on_action(cx.listener(Self::bubble_down))
                .on_action(cx.listener(Self::bubble_confirm))
                .on_action(cx.listener(Self::bubble_cancel))
                .on_action(cx.listener(|_, _: &Stay, _, _| {}))
                .occlude()
                .absolute()
                .left(left)
                .when(above, |this| {
                    this.bottom(pane.size.height - (line.top() - origin.y) + px(6.))
                })
                .when(!above, |this| this.top(line.bottom() - origin.y + px(6.)))
                .w(px(360.))
                .bg(roles.overlay)
                .border_1()
                .border_color(roles.overlay)
                .p(px(8.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    ui::row().gap(px(6.)).child(ui::ai_flag(roles)).child(
                        div()
                            .flex_1()
                            .child(Input::new(&self.bubble_input).appearance(false)),
                    ),
                )
                .when_some(hint, |this, hint| {
                    this.child(
                        div()
                            .pl(px(2.))
                            .text_size(px(12.))
                            .text_color(roles.faint)
                            .child(hint),
                    )
                })
                .children(rows.into_iter().enumerate().map(|(index, row)| {
                    let (label, skill, span) = match &row {
                        AskRow::Action { action, match_span } => {
                            (action.label.clone(), action.skill_only, *match_span)
                        }
                        AskRow::Free { text } => (format!("Ask AI: “{text}”"), false, None),
                    };
                    let highlighted = index == active;
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
                    div()
                        .id(("ask-row", index))
                        .h(px(28.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_1()
                        .border_color(if highlighted {
                            roles.ai_fill
                        } else {
                            roles.fill
                        })
                        .bg(if highlighted {
                            roles.ai_fill
                        } else {
                            roles.fill
                        })
                        .text_color(if highlighted { roles.ai } else { roles.ink })
                        .text_size(px(13.))
                        .cursor_pointer()
                        .child(text)
                        .when(skill, |this| {
                            this.child(
                                div()
                                    .text_color(roles.faint)
                                    .text_size(px(11.))
                                    .child("Skill"),
                            )
                        })
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
                }))
                .into_any_element(),
        )
    }
}

fn accepted_change(ready: &ReadySuggestion) -> AcceptedChange {
    AcceptedChange {
        origin: ready.origin,
        instruction: instruction_label(&ready.instruction).to_string(),
        rationale: ready.rationale.clone(),
    }
}

fn styled_replacement(hunks: &[DiffHunk], roles: Roles) -> StyledText {
    let mut text = String::new();
    let mut highlights = Vec::new();
    for hunk in hunks {
        match hunk {
            DiffHunk::Equal { text: part, .. } => text.push_str(part),
            DiffHunk::Deleted { text: part, .. } => {
                let start = text.len();
                text.push_str(part);
                highlights.push((
                    start..text.len(),
                    HighlightStyle {
                        color: Some(roles.faint),
                        strikethrough: Some(StrikethroughStyle {
                            thickness: px(1.),
                            color: Some(roles.faint),
                        }),
                        ..Default::default()
                    },
                ));
            }
            DiffHunk::Inserted { text: part, .. } => {
                let start = text.len();
                text.push_str(part);
                highlights.push((
                    start..text.len(),
                    HighlightStyle {
                        color: Some(roles.ai),
                        background_color: Some(roles.ai_fill),
                        ..Default::default()
                    },
                ));
            }
        }
    }
    StyledText::new(text).with_highlights(highlights)
}

impl Render for EditorPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let roles = theme::roles(cx);
        if std::mem::take(&mut self.reveal_pending) {
            self.reveal_first_review_suggestion(window, cx);
        }
        let bubble = self.bubble_view(window, cx);
        let shown = bubble.is_some();
        if self.bubble_shown && !shown {
            self.clear_bubble(window, cx);
        }
        self.bubble_shown = shown;
        let active_id = self.active(cx).map(|item| item.id.clone());
        let card_focused = self.card_focus.is_focused(window);
        let mut items = self.suggestions.items.clone();
        items.sort_by_key(|item| item.from);
        let mut cards: Vec<AnyElement> = Vec::new();
        let mut floor = px(0.);
        for item in &items {
            let active = Some(&item.id) == active_id.as_ref();
            if let Some((card, next)) =
                self.render_card(item, active, floor, card_focused, roles, cx)
            {
                cards.push(card);
                floor = next;
            }
        }
        let caret = self.render_caret(roles, cx);
        let bubble =
            bubble.and_then(|(above, anchor, _)| self.render_bubble(above, anchor, roles, cx));
        let bounds = self.bounds.clone();
        let entity = cx.entity().downgrade();
        let release = self.pointer_down.clone();
        let release_entity = entity.clone();
        let press = self.pointer_down.clone();
        div()
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
            .relative()
            .size_full()
            .child(
                canvas(
                    move |next, _, cx| {
                        if bounds.get() != next {
                            bounds.set(next);
                            if let Some(entity) = entity.upgrade() {
                                entity.update(cx, |_, cx| cx.notify());
                            }
                        }
                    },
                    move |_, _, window, _| {
                        let release = release.clone();
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
                .size_full(),
            )
            .child(
                div().size_full().flex().justify_center().child(
                    div()
                        .id("editor-column")
                        .w(px(COLUMN_WIDTH))
                        .max_w_full()
                        .h_full()
                        .px(px(COLUMN_PADDING))
                        .capture_any_mouse_down(move |_, _, _| press.set(true))
                        .child(Editor::new(&self.editor).h_full().bordered(false)),
                ),
            )
            .children(caret)
            .children(cards)
            .children(bubble)
    }
}
