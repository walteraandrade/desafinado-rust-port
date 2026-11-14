use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use desafinado_ai::{
    edits::ReviewValidationContext,
    prompts::{ReviewPromptInput, build_review_prompt},
    review_stream::{ReviewStream, ReviewStreamEvent},
};
use desafinado_core::suggestions::{
    PlaceReviewEditInput, ReviewPlacement, occupied_block_ids, place_review_edit,
    review_notice_text, review_task_label, review_too_large, review_too_large_text,
};
use futures::{StreamExt as _, channel::mpsc};

use desafinado_ai::{
    editor::ask_menu::{self, AskMenu, AskRow},
    prompts::{RefinePromptInput, TargetPromptInput, build_refine_prompt, build_target_prompt},
    quick_actions::{QuickAction, default_quick_actions},
};
use desafinado_core::{
    blocks::{Selection, Target, extract_blocks, find_frontmatter, get_neighbors, resolve_target},
    change_set::{ChangeSet, TextChange},
    diff::DiffHunk,
    suggestions::{
        ClientError, Direction, Instruction, NoticeKind, RefineState, Suggestion, SuggestionDraft,
        SuggestionOrigin, SuggestionResult, SuggestionsState, accept_suggestion, active_suggestion,
        begin_refine, begin_suggestion, change_ranges, describe_error, discard_suggestion,
        empty_suggestions, fail_refine, fail_suggestion, instruction_history, map_suggestions,
        refine_task_label, reject_suggestion, resolve_refine, resolve_suggestion, retry_suggestion,
        show_version, step_suggestion, task_label,
    },
};
use gpui_kit::{
    component::input::{
        Editor, EditorState, Input, InputEditsEvent, InputEvent, InputState, RangeDecoration,
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
        RejectActive,
        PreviousVersion,
        NextVersion,
        NextSuggestion,
        PreviousSuggestion,
        BubbleUp,
        BubbleDown,
        BubbleConfirm,
        BubbleCancel,
    ]
);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-k", AskAi, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-shift-z", Redo, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-y", Redo, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-z", Undo, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-enter", AcceptActive, Some("EditorPane > Input")),
        KeyBinding::new("escape", RejectActive, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-h", PreviousVersion, Some("EditorPane > Input")),
        KeyBinding::new("ctrl-l", NextVersion, Some("EditorPane > Input")),
        KeyBinding::new("alt-]", NextSuggestion, Some("EditorPane > Input")),
        KeyBinding::new("alt-[", PreviousSuggestion, Some("EditorPane > Input")),
        KeyBinding::new("up", BubbleUp, Some("AskBubble > Input")),
        KeyBinding::new("down", BubbleDown, Some("AskBubble > Input")),
        KeyBinding::new("enter", BubbleConfirm, Some("AskBubble > Input")),
        KeyBinding::new("escape", BubbleCancel, Some("AskBubble > Input")),
    ]);
}

pub struct AcceptedChange {
    pub origin: SuggestionOrigin,
    pub instruction: String,
    pub rationale: String,
}

pub enum PaneEvent {
    Changed,
    Accepted {
        before: String,
        text: String,
        changes: Vec<AcceptedChange>,
    },
}

impl EventEmitter<PaneEvent> for EditorPane {}

enum BubbleMode {
    Ask,
    Refine(String),
}

struct Bubble {
    input: Entity<InputState>,
    menu: AskMenu,
    mode: BubbleMode,
    selection: Selection,
    _subscription: Subscription,
}

pub struct EditorPane {
    pub editor: Entity<EditorState>,
    pub suggestions: SuggestionsState,
    strike: TextDecorationCollection,
    fills: RangeDecorationCollection,
    bubble: Option<Bubble>,
    applying: Option<String>,
    tasks: HashMap<String, Task<()>>,
    next_id: usize,
    pub quick_actions: Vec<QuickAction>,
    pub is_skill: bool,
    ai: Arc<dyn AiBackend>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    heights: Rc<RefCell<HashMap<String, Pixels>>>,
    notice: Option<SharedString>,
    pub review: Option<ReviewRun>,
    _subscriptions: Vec<Subscription>,
}

pub struct ReviewRun {
    pub added: usize,
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
                this.suggestions = match this.applying.clone() {
                    Some(id) => accept_suggestion(&this.suggestions, &id, &changes),
                    None => map_suggestions(&this.suggestions, &changes, frontmatter.as_ref()),
                };
            }
            this.applying = None;
            this.drop_dead_tasks();
            this.refresh_decorations(cx);
            cx.emit(PaneEvent::Changed);
            cx.notify();
        });
        let changes = cx.subscribe(&editor, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        Self {
            editor,
            suggestions: empty_suggestions(),
            strike,
            fills,
            bubble: None,
            applying: None,
            tasks: HashMap::new(),
            next_id: 0,
            quick_actions: default_quick_actions(),
            is_skill: false,
            ai,
            bounds: Rc::new(Cell::new(Bounds::default())),
            heights: Rc::new(RefCell::new(HashMap::new())),
            notice: None,
            review: None,
            _subscriptions: vec![edits, changes],
        }
    }

    pub fn text(&self, cx: &App) -> String {
        self.editor.read(cx).value().to_string()
    }

    pub fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.tasks.clear();
        self.suggestions = empty_suggestions();
        self.bubble = None;
        self.editor.update(cx, |state, cx| {
            state.set_value(text.to_string(), window, cx)
        });
        self.refresh_decorations(cx);
        cx.notify();
    }

    fn clear_suggestions(&mut self, cx: &mut Context<Self>) {
        self.stop_review(cx);
        self.tasks.clear();
        self.suggestions = empty_suggestions();
        self.bubble = None;
    }

    pub fn replace_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_suggestions(cx);
        self.set_text(text, window, cx);
    }

    pub fn refresh_text(
        &mut self,
        change: &crate::document::MinimalChange,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear_suggestions(cx);
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
        self.refresh_decorations(cx);
        cx.notify();
    }

    pub fn restore_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_suggestions(cx);
        self.editor.update(cx, |state, cx| {
            state.replace_all(text.to_string(), window, cx)
        });
        self.refresh_decorations(cx);
        cx.notify();
    }

    pub fn open_count(&self) -> usize {
        self.suggestions
            .items
            .iter()
            .filter(|item| item.is_ready())
            .count()
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |state, cx| state.focus(window, cx));
    }

    fn drop_dead_tasks(&mut self) {
        let live: Vec<String> = self
            .suggestions
            .items
            .iter()
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

    pub fn refresh_decorations(&mut self, cx: &mut Context<Self>) {
        let roles = theme::roles(cx);
        let cursor = self.cursor(cx);
        let active_id =
            active_suggestion(&self.suggestions, Some(cursor)).map(|item| item.id.clone());
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
                    } => Some(TextDecoration::new(
                        from + start..from + end,
                        HighlightStyle {
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
        let fills: Vec<RangeDecoration> = self
            .suggestions
            .items
            .iter()
            .filter(|item| item.to > item.from)
            .map(|item| {
                let active = Some(&item.id) == active_id.as_ref();
                let decoration = RangeDecoration::new(item.from..item.to);
                match item {
                    Suggestion::Ready(_) if active => decoration
                        .with_style(RangeDecorationStyle::Fill)
                        .with_color(roles.raised),
                    Suggestion::Ready(_) => decoration
                        .with_style(RangeDecorationStyle::Frame)
                        .with_color(roles.hairline),
                    Suggestion::Pending(_) => decoration
                        .with_style(RangeDecorationStyle::Frame)
                        .with_color(roles.ai),
                    Suggestion::Failed(_) => decoration
                        .with_style(RangeDecorationStyle::Frame)
                        .with_color(roles.danger),
                }
            })
            .collect();
        self.strike.set(strikes, cx);
        self.fills.set(fills, cx);
    }

    fn ask(&mut self, _: &AskAi, window: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.cursor(cx);
        if let Some(active) = active_suggestion(&self.suggestions, Some(cursor))
            && active.from <= cursor
            && cursor <= active.to
        {
            let id = active.id.clone();
            self.open_bubble(BubbleMode::Refine(id), window, cx);
            return;
        }
        self.open_bubble(BubbleMode::Ask, window, cx);
    }

    fn open_bubble(&mut self, mode: BubbleMode, window: &mut Window, cx: &mut Context<Self>) {
        let placeholder = match mode {
            BubbleMode::Ask => {
                ask_menu::ask_hint(&self.visible_actions()).unwrap_or_else(|| "Ask AI…".into())
            }
            BubbleMode::Refine(_) => "Refine: tell the AI what to change".into(),
        };
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let subscription =
            cx.subscribe_in(&input, window, |this, input, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = input.read(cx).value().to_string();
                    if let Some(bubble) = this.bubble.as_mut() {
                        bubble.menu = ask_menu::type_ask_menu(&query);
                    }
                    cx.notify();
                }
            });
        input.update(cx, |state, cx| state.focus(window, cx));
        self.bubble = Some(Bubble {
            input,
            menu: AskMenu::default(),
            mode,
            selection: self.selection(cx),
            _subscription: subscription,
        });
        cx.notify();
    }

    fn visible_actions(&self) -> Vec<QuickAction> {
        desafinado_ai::quick_actions::quick_actions_for(self.is_skill, &self.quick_actions)
    }

    fn close_bubble(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.bubble = None;
        self.focus(window, cx);
        cx.notify();
    }

    fn bubble_move(&mut self, step: i8, cx: &mut Context<Self>) {
        let actions = self.visible_actions();
        if let Some(bubble) = self.bubble.as_mut() {
            bubble.menu = ask_menu::move_ask_menu(&actions, &bubble.menu, step);
        }
        cx.notify();
    }

    fn bubble_up(&mut self, _: &BubbleUp, _: &mut Window, cx: &mut Context<Self>) {
        self.bubble_move(-1, cx);
    }

    fn bubble_down(&mut self, _: &BubbleDown, _: &mut Window, cx: &mut Context<Self>) {
        self.bubble_move(1, cx);
    }

    fn bubble_cancel(&mut self, _: &BubbleCancel, window: &mut Window, cx: &mut Context<Self>) {
        self.close_bubble(window, cx);
    }

    fn bubble_confirm(&mut self, _: &BubbleConfirm, window: &mut Window, cx: &mut Context<Self>) {
        let Some(bubble) = self.bubble.as_ref() else {
            return;
        };
        let query = bubble.input.read(cx).value().to_string();
        let selection = bubble.selection;
        let instruction = match &bubble.mode {
            BubbleMode::Refine(_) => {
                let text = query.trim().to_string();
                (!text.is_empty()).then_some(Instruction::FreeText { text })
            }
            BubbleMode::Ask => match ask_menu::chosen_row(&self.visible_actions(), &bubble.menu) {
                Some(AskRow::Action { action, .. }) => Some(Instruction::QuickAction {
                    id: action.id.clone(),
                    label: action.label.clone(),
                    text: action.instruction.clone(),
                }),
                Some(AskRow::Free { text }) => Some(Instruction::FreeText { text }),
                None => None,
            },
        };
        let refine = match &bubble.mode {
            BubbleMode::Refine(id) => Some(id.clone()),
            BubbleMode::Ask => None,
        };
        let Some(instruction) = instruction else {
            return;
        };
        self.close_bubble(window, cx);
        match refine {
            Some(id) => self.refine(id, instruction, cx),
            None => self.request(selection, instruction, cx),
        }
    }

    fn request(&mut self, selection: Selection, instruction: Instruction, cx: &mut Context<Self>) {
        let text = self.text(cx);
        let blocks = extract_blocks(&text);
        let Some(target) = resolve_target(&blocks, &text, selection) else {
            self.notice = Some("Put the cursor in a paragraph or select text first.".into());
            cx.notify();
            return;
        };
        if target.text.trim().is_empty() {
            self.notice = Some("Put the cursor in a paragraph or select text first.".into());
            cx.notify();
            return;
        }
        self.notice = None;
        let neighbors = get_neighbors(&blocks, &target);
        let ids: Vec<&str> = target
            .blocks
            .iter()
            .map(|block| block.id.as_str())
            .collect();
        let task = task_label(target.source, &ids, &instruction);
        let parts = build_target_prompt(&TargetPromptInput {
            target: target.clone(),
            neighbors,
            instruction: instruction.text().to_string(),
        });
        self.next_id += 1;
        let id = format!("s{}", self.next_id);
        self.suggestions = begin_suggestion(
            &self.suggestions,
            SuggestionDraft {
                id: id.clone(),
                from: target.from,
                to: target.to,
                source: target.source,
                original: target.text.clone(),
                instruction,
                origin: None,
            },
        );
        let call = TargetCall {
            task,
            parts,
            original: target.text.clone(),
        };
        self.run(id, call, false, cx);
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
        result: Result<SuggestionResult, ClientError>,
        refine: bool,
        cx: &mut Context<Self>,
    ) {
        self.tasks.remove(id);
        self.suggestions = match (result, refine) {
            (Ok(result), false) => resolve_suggestion(&self.suggestions, id, &result),
            (Ok(result), true) => resolve_refine(&self.suggestions, id, &result),
            (Err(error), false) => fail_suggestion(&self.suggestions, id, error),
            (Err(error), true) => fail_refine(&self.suggestions, id, error),
        };
        self.refresh_decorations(cx);
        cx.emit(PaneEvent::Changed);
        cx.notify();
    }

    fn refine(&mut self, id: String, instruction: Instruction, cx: &mut Context<Self>) {
        let Some(ready) = self
            .suggestions
            .items
            .iter()
            .filter_map(Suggestion::as_ready)
            .find(|item| item.id == id)
            .cloned()
        else {
            return;
        };
        let text = self.text(cx);
        let blocks = extract_blocks(&text);
        let target = resolve_target(
            &blocks,
            &text,
            Selection {
                from: ready.from,
                to: ready.to,
            },
        )
        .unwrap_or(Target {
            source: ready.source,
            from: ready.from,
            to: ready.to,
            text: ready.original.clone(),
            blocks: vec![],
        });
        let neighbors = get_neighbors(&blocks, &target);
        let ids: Vec<&str> = target
            .blocks
            .iter()
            .map(|block| block.id.as_str())
            .collect();
        let task = refine_task_label(&ids, &instruction);
        let parts = build_refine_prompt(&RefinePromptInput {
            target: Target {
                text: ready.original.clone(),
                ..target.clone()
            },
            neighbors,
            instruction: instruction.text().to_string(),
            history: instruction_history(&ready),
            latest_replacement: ready.replacement.clone(),
        });
        self.suggestions = begin_refine(&self.suggestions, &id, instruction);
        self.run(
            id,
            TargetCall {
                task,
                parts,
                original: ready.original.clone(),
            },
            true,
            cx,
        );
    }

    pub fn accept(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let before = self.text(cx);
        let changes: Vec<AcceptedChange> = self.accept_one(id, window, cx).into_iter().collect();
        self.emit_accepted(before, changes, cx);
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

    fn accept_one(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AcceptedChange> {
        let ready = self
            .suggestions
            .items
            .iter()
            .filter_map(Suggestion::as_ready)
            .find(|item| item.id == id)
            .cloned()?;
        self.applying = Some(ready.id.clone());
        let replacement = ready.replacement.clone();
        self.editor.update(cx, |state, cx| {
            state.set_selected_range(ready.from..ready.to, cx);
            state.replace(replacement, window, cx);
        });
        if self.applying.is_some() {
            self.applying = None;
            self.suggestions = reject_suggestion(&self.suggestions, &ready.id);
        }
        Some(AcceptedChange {
            origin: ready.origin,
            instruction: instruction_label(&ready.instruction),
            rationale: ready.rationale.clone(),
        })
    }

    pub fn reject(&mut self, id: &str, cx: &mut Context<Self>) {
        self.tasks.remove(id);
        self.suggestions = reject_suggestion(&self.suggestions, id);
        self.refresh_decorations(cx);
        cx.notify();
    }

    pub fn accept_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut ids: Vec<(usize, String)> = self
            .suggestions
            .items
            .iter()
            .filter_map(Suggestion::as_ready)
            .map(|item| (item.from, item.id.clone()))
            .collect();
        ids.sort_by(|a, b| b.0.cmp(&a.0));
        let before = self.text(cx);
        let mut changes: Vec<AcceptedChange> = ids
            .into_iter()
            .filter_map(|(_, id)| self.accept_one(&id, window, cx))
            .collect();
        changes.reverse();
        self.emit_accepted(before, changes, cx);
    }

    pub fn reject_all(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<String> = self
            .suggestions
            .items
            .iter()
            .map(|item| item.id.clone())
            .collect();
        for id in ids {
            self.reject(&id, cx);
        }
    }

    pub fn add_review_suggestion(
        &mut self,
        draft: SuggestionDraft,
        result: &SuggestionResult,
        cx: &mut Context<Self>,
    ) {
        let id = draft.id.clone();
        self.suggestions = begin_suggestion(&self.suggestions, draft);
        self.suggestions = resolve_suggestion(&self.suggestions, &id, result);
        self.refresh_decorations(cx);
        cx.notify();
    }

    pub fn start_review(&mut self, focus: String, cx: &mut Context<Self>) {
        if self.review.is_some() {
            return;
        }
        let text = self.text(cx);
        if review_too_large(text.len()) {
            self.notice = Some(review_too_large_text(text.len()).into());
            cx.notify();
            return;
        }
        let blocks = extract_blocks(&text);
        let occupied = occupied_block_ids(&self.suggestions, &blocks);
        let parts = build_review_prompt(&ReviewPromptInput {
            blocks: blocks.clone(),
            focus: Some(focus.clone()).filter(|focus| !focus.trim().is_empty()),
        });
        let call = ReviewCall {
            task: review_task_label(&focus),
            parts,
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
        let mut stream = ReviewStream::new(ReviewValidationContext { blocks, occupied });
        let task = cx.spawn(async move |this, cx| {
            let mut failure: Option<ClientError> = None;
            while let Some(event) = receiver.next().await {
                match event {
                    ReviewEvent::Chunk(chunk) => {
                        let events = stream.feed(&chunk);
                        let alive = this
                            .update(cx, |this, cx| {
                                for event in events {
                                    if let ReviewStreamEvent::Edit(edit) = event {
                                        this.place_review(edit, &focus, cx);
                                    }
                                }
                                this.review
                                    .as_ref()
                                    .is_some_and(|run| !run.stop.load(Ordering::SeqCst))
                            })
                            .unwrap_or(false);
                        if !alive {
                            break;
                        }
                    }
                    ReviewEvent::Done => {
                        failure = stream.finish().err();
                        break;
                    }
                    ReviewEvent::Failed(error) => {
                        failure = Some(error);
                        break;
                    }
                }
            }
            let _ = this.update(cx, |this, cx| this.end_review(failure, cx));
        });
        self.review = Some(ReviewRun {
            added: 0,
            stop,
            changes: ChangeSet::empty(text.len()),
            _task: task,
        });
        self.notice = None;
        cx.emit(PaneEvent::Changed);
        cx.notify();
    }

    fn place_review(
        &mut self,
        edit: desafinado_core::suggestions::ValidatedEdit,
        focus: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(changes) = self.review.as_ref().map(|run| run.changes.clone()) else {
            return;
        };
        let doc = self.text(cx);
        let id = self.next_suggestion_id();
        let placement = place_review_edit(&PlaceReviewEditInput {
            state: &self.suggestions,
            edit: &edit,
            changes: &changes,
            doc: &doc,
            id: &id,
            focus,
        });
        if let ReviewPlacement::Placed { draft, result } = placement {
            self.add_review_suggestion(draft, &result, cx);
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
        let stopped = run.stop.load(Ordering::SeqCst);
        self.notice = Some(
            if stopped {
                desafinado_core::suggestions::review_summary(run.added, true)
            } else {
                review_notice_text(run.added, failure.as_ref())
            }
            .into(),
        );
        cx.emit(PaneEvent::Changed);
        cx.notify();
    }

    pub fn stop_review(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = self.review.as_ref() {
            run.stop.store(true, Ordering::SeqCst);
        }
        self.end_review(None, cx);
    }

    pub fn next_suggestion_id(&mut self) -> String {
        self.next_id += 1;
        format!("s{}", self.next_id)
    }

    fn active_id(&self, cx: &App) -> Option<String> {
        active_suggestion(&self.suggestions, Some(self.cursor(cx))).map(|item| item.id.clone())
    }

    fn accept_active(&mut self, _: &AcceptActive, window: &mut Window, cx: &mut Context<Self>) {
        match self.active_id(cx) {
            Some(id) => self.accept(&id, window, cx),
            None => cx.propagate(),
        }
    }

    fn reject_active(&mut self, _: &RejectActive, _: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.cursor(cx);
        let inside = self
            .suggestions
            .items
            .iter()
            .find(|item| item.from <= cursor && cursor <= item.to)
            .map(|item| item.id.clone());
        match inside {
            Some(id) => self.reject(&id, cx),
            None => cx.propagate(),
        }
    }

    fn step_version(&mut self, step: i64, cx: &mut Context<Self>) {
        let Some(id) = self.active_id(cx) else {
            cx.propagate();
            return;
        };
        let shown = self
            .suggestions
            .items
            .iter()
            .filter_map(Suggestion::as_ready)
            .find(|item| item.id == id)
            .map(|item| item.shown as i64)
            .unwrap_or(0);
        self.suggestions = show_version(&self.suggestions, &id, shown + step);
        self.refresh_decorations(cx);
        cx.notify();
    }

    fn previous_version(&mut self, _: &PreviousVersion, _: &mut Window, cx: &mut Context<Self>) {
        self.step_version(-1, cx);
    }

    fn next_version(&mut self, _: &NextVersion, _: &mut Window, cx: &mut Context<Self>) {
        self.step_version(1, cx);
    }

    fn step(&mut self, direction: Direction, window: &mut Window, cx: &mut Context<Self>) {
        let cursor = self.cursor(cx);
        if let Some(position) = step_suggestion(&self.suggestions, cursor, direction) {
            self.editor.update(cx, |state, cx| {
                state.set_selected_range(position..position, cx);
                state.focus(window, cx);
            });
            self.refresh_decorations(cx);
            cx.notify();
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

    fn retry(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(item) = self
            .suggestions
            .items
            .iter()
            .find(|item| item.id == id)
            .cloned()
        else {
            return;
        };
        self.suggestions = retry_suggestion(&self.suggestions, id);
        self.suggestions = discard_suggestion(&self.suggestions, id, NoticeKind::TextChanged);
        self.request(
            Selection {
                from: item.from,
                to: item.to,
            },
            item.instruction.clone(),
            cx,
        );
    }

    fn origin(&self) -> Point<Pixels> {
        self.bounds.get().origin
    }

    fn render_card(
        &self,
        item: &Suggestion,
        active: bool,
        floor: Pixels,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Option<(AnyElement, Pixels)> {
        let state = self.editor.read(cx);
        let line = state.range_to_bounds(&(item.from..item.from))?;
        let pane = self.bounds.get();
        let column_right = pane.origin.x + (pane.size.width + px(768.).min(pane.size.width)) / 2.;
        let margin = pane.origin.x + pane.size.width - column_right;
        let width = px(232.);
        let (left, top) = if margin >= width + px(16.) {
            (
                column_right + px(8.) - pane.origin.x,
                line.top() - pane.origin.y,
            )
        } else {
            let range = state.range_to_bounds(&(item.to..item.to)).unwrap_or(line);
            (
                (column_right - width - px(24.) - pane.origin.x).max(px(8.)),
                range.bottom() + px(8.) - pane.origin.y,
            )
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
            Suggestion::Pending(base) => {
                card.child(ui::row().gap(px(6.)).child(ui::ai_flag(roles)).child(
                    div().text_color(roles.ai).child(format!(
                        "{} ···",
                        base.instruction.text().chars().take(40).collect::<String>()
                    )),
                ))
            }
            Suggestion::Failed(failed) => {
                let retry_id = id.clone();
                let dismiss_id = id.clone();
                card.border_color(roles.danger)
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
                                    "Try again",
                                    ButtonKind::Default,
                                    ButtonSize::Small,
                                    false,
                                    roles,
                                )
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.retry(&retry_id, cx)),
                                ),
                            )
                            .child(
                                ui::sized_button(
                                    SharedString::from(format!("dismiss-{id}")),
                                    "Dismiss",
                                    ButtonKind::Quiet,
                                    ButtonSize::Small,
                                    false,
                                    roles,
                                )
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.reject(&dismiss_id, cx)),
                                ),
                            ),
                    )
            }
            Suggestion::Ready(ready) => {
                let replacement = styled_replacement(&ready.diff.hunks, roles);
                let accept_id = id.clone();
                let reject_id = id.clone();
                let refine_id = id.clone();
                let steps = ready.history.len();
                let changes = change_ranges(ready.from, &ready.diff).len();
                let working = matches!(ready.refine, RefineState::Working { .. });
                let refine_error = match &ready.refine {
                    RefineState::Failed { error, .. } => Some(describe_error(error)),
                    _ => None,
                };
                card.when(active, |this| this.border_color(roles.ai))
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
                    .when(changes > 1, |this| {
                        this.child(
                            div()
                                .text_color(roles.faint)
                                .child(format!("{changes} changes")),
                        )
                    })
                    .when(working, |this| {
                        this.child(div().text_color(roles.ai).child("Refining ···"))
                    })
                    .when_some(refine_error, |this, error| {
                        this.child(div().text_color(roles.danger).child(error))
                    })
                    .when(active, |this| {
                        this.child(
                            ui::row()
                                .gap(px(6.))
                                .flex_wrap()
                                .child(
                                    ui::sized_button(
                                        SharedString::from(format!("accept-{id}")),
                                        "Accept",
                                        ButtonKind::Primary,
                                        ButtonSize::Small,
                                        false,
                                        roles,
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.accept(&accept_id, window, cx)
                                        },
                                    )),
                                )
                                .child(
                                    ui::sized_button(
                                        SharedString::from(format!("reject-{id}")),
                                        "Reject",
                                        ButtonKind::Default,
                                        ButtonSize::Small,
                                        false,
                                        roles,
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| this.reject(&reject_id, cx),
                                    )),
                                )
                                .child(
                                    ui::sized_button(
                                        SharedString::from(format!("refine-{id}")),
                                        "Refine",
                                        ButtonKind::Default,
                                        ButtonSize::Small,
                                        false,
                                        roles,
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            this.open_bubble(
                                                BubbleMode::Refine(refine_id.clone()),
                                                window,
                                                cx,
                                            )
                                        },
                                    )),
                                )
                                .when(steps > 1, |this| {
                                    let back = id.clone();
                                    let forward = id.clone();
                                    let shown = ready.shown as i64;
                                    this.child(
                                        ui::row()
                                            .gap(px(2.))
                                            .child(
                                                ui::sized_button(
                                                    SharedString::from(format!("prev-{id}")),
                                                    "‹",
                                                    ButtonKind::Quiet,
                                                    ButtonSize::Small,
                                                    false,
                                                    roles,
                                                )
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.suggestions = show_version(
                                                        &this.suggestions,
                                                        &back,
                                                        shown - 1,
                                                    );
                                                    this.refresh_decorations(cx);
                                                    cx.notify();
                                                })),
                                            )
                                            .child(format!("{} / {}", ready.shown + 1, steps))
                                            .child(
                                                ui::sized_button(
                                                    SharedString::from(format!("next-{id}")),
                                                    "›",
                                                    ButtonKind::Quiet,
                                                    ButtonSize::Small,
                                                    false,
                                                    roles,
                                                )
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.suggestions = show_version(
                                                        &this.suggestions,
                                                        &forward,
                                                        shown + 1,
                                                    );
                                                    this.refresh_decorations(cx);
                                                    cx.notify();
                                                })),
                                            ),
                                    )
                                }),
                        )
                    })
            }
        };
        Some((element.into_any_element(), top + height + px(8.)))
    }

    fn render_caret(&self, item: &Suggestion, roles: Roles, cx: &App) -> Option<AnyElement> {
        let working = match item {
            Suggestion::Pending(base) => Some(base.instruction.text().to_string()),
            Suggestion::Ready(ready) => match &ready.refine {
                RefineState::Working { instruction } => Some(instruction.text().to_string()),
                _ => None,
            },
            Suggestion::Failed(_) => None,
        }?;
        let bounds = self.editor.read(cx).range_to_bounds(&(item.to..item.to))?;
        let origin = self.origin();
        let label: String = working.chars().take(32).collect();
        Some(
            div()
                .absolute()
                .left(bounds.left() - origin.x)
                .top(bounds.top() - origin.y - px(18.))
                .child(
                    div()
                        .px(px(4.))
                        .h(px(16.))
                        .bg(roles.ai)
                        .text_color(roles.canvas)
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("AI · {label} ···")),
                )
                .child(div().w(px(2.)).h(bounds.size.height).bg(roles.ai))
                .into_any_element(),
        )
    }

    fn render_bubble(
        &self,
        bubble: &Bubble,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.editor.read(cx);
        let anchor = state
            .range_to_bounds(&(bubble.selection.to..bubble.selection.to))
            .or_else(|| state.range_to_bounds(&(state.cursor()..state.cursor())))
            .unwrap_or_default();
        let origin = self.origin();
        let actions = self.visible_actions();
        let rows = match bubble.mode {
            BubbleMode::Ask => ask_menu::menu_rows(&actions, &bubble.menu),
            BubbleMode::Refine(_) => vec![],
        };
        let active = bubble.menu.active;
        Some(
            div()
                .id("bubble")
                .key_context("AskBubble")
                .on_action(cx.listener(Self::bubble_up))
                .on_action(cx.listener(Self::bubble_down))
                .on_action(cx.listener(Self::bubble_confirm))
                .on_action(cx.listener(Self::bubble_cancel))
                .occlude()
                .absolute()
                .left((anchor.left() - origin.x - px(12.)).max(px(8.)))
                .top(anchor.bottom() - origin.y + px(8.))
                .w(px(360.))
                .bg(roles.raised)
                .border_1()
                .border_color(roles.hairline)
                .p(px(8.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    ui::row().gap(px(6.)).child(ui::ai_flag(roles)).child(
                        div()
                            .flex_1()
                            .child(Input::new(&bubble.input).appearance(false)),
                    ),
                )
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
                    let chosen = row.clone();
                    div()
                        .id(("ask-row", index))
                        .h(px(28.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_1()
                        .border_color(if highlighted { roles.ai } else { roles.raised })
                        .bg(if highlighted {
                            roles.raised
                        } else {
                            roles.fill
                        })
                        .text_color(if highlighted { roles.ai } else { roles.ink })
                        .text_size(px(13.))
                        .child(text)
                        .when(skill, |this| {
                            this.child(
                                div()
                                    .text_color(roles.faint)
                                    .text_size(px(11.))
                                    .child("Skill"),
                            )
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(bubble) = this.bubble.as_mut() {
                                bubble.menu = ask_menu::hover_ask_menu(&bubble.menu, index);
                            }
                            let _ = &chosen;
                            this.bubble_confirm(&BubbleConfirm, window, cx);
                        }))
                }))
                .into_any_element(),
        )
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
                        background_color: Some(roles.raised),
                        ..Default::default()
                    },
                ));
            }
        }
    }
    StyledText::new(text).with_highlights(highlights)
}

impl Render for EditorPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let roles = theme::roles(cx);
        let cursor = self.cursor(cx);
        let active_id =
            active_suggestion(&self.suggestions, Some(cursor)).map(|item| item.id.clone());
        let mut items = self.suggestions.items.clone();
        items.sort_by_key(|item| item.from);
        let mut cards: Vec<AnyElement> = Vec::new();
        let mut carets: Vec<AnyElement> = Vec::new();
        let mut floor = px(0.);
        for item in &items {
            let active = Some(&item.id) == active_id.as_ref();
            if let Some((card, next)) = self.render_card(item, active, floor, roles, cx) {
                cards.push(card);
                floor = next;
            }
            if let Some(caret) = self.render_caret(item, roles, cx) {
                carets.push(caret);
            }
        }
        let bubble = self
            .bubble
            .as_ref()
            .and_then(|bubble| self.render_bubble(bubble, roles, cx));
        let bounds = self.bounds.clone();
        let entity = cx.entity().downgrade();
        div()
            .id("editor-pane")
            .key_context("EditorPane")
            .on_action(cx.listener(Self::ask))
            .on_action(cx.listener(Self::accept_active))
            .on_action(cx.listener(Self::reject_active))
            .on_action(cx.listener(Self::previous_version))
            .on_action(cx.listener(Self::next_version))
            .on_action(cx.listener(Self::next_suggestion))
            .on_action(cx.listener(Self::previous_suggestion))
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
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                div().size_full().flex().justify_center().child(
                    div()
                        .w(px(768.))
                        .max_w_full()
                        .h_full()
                        .px(px(24.))
                        .child(Editor::new(&self.editor).h_full().bordered(false)),
                ),
            )
            .children(self.notice.clone().map(|notice| {
                div()
                    .absolute()
                    .bottom(px(16.))
                    .left(px(16.))
                    .p(px(8.))
                    .bg(roles.raised)
                    .border_1()
                    .border_color(roles.hairline)
                    .text_size(px(13.))
                    .child(notice)
            }))
            .children(carets)
            .children(cards)
            .children(bubble)
    }
}

fn instruction_label(instruction: &Instruction) -> String {
    match instruction {
        Instruction::QuickAction { label, .. } => label.clone(),
        Instruction::FreeText { text } => text.clone(),
    }
}
