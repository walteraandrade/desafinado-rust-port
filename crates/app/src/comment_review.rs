//! A Review of the whole Document in the editor. A decision model reads the Focus and picks
//! Comment or Revise. Revise is the Review pass; Comment leaves Notes: General notes in a panel
//! beside the text, and a numbered mark in the margin for each Note on a Block. The text is not
//! touched until the writer asks for a rewrite of one Note.

use desafinado_core::log_err::LogErr;
use std::sync::Arc;

use desafinado_ai::{
    editor::{
        card_placement::{CardPlacement, MARGIN_CARD_WIDTH, margin_card_left},
        geometry::{Rect, Viewport},
        popover_place::{PopoverInput, Side, place_popover},
    },
    notes::{
        CommentPromptInput, Note, NoteBoard, build_comment_prompt, comment_task_label,
        rewrite_instruction,
    },
    review_mode::{ModeChoice, ModeSource, ReviewMode, mode_task_label, mode_without_decision},
};
use desafinado_core::{
    blocks::extract_blocks,
    change_set::ChangeSet,
    suggestions::{
        ClientError, Instruction, describe_error, review_too_large, review_too_large_text,
    },
};
use desafinado_runtime::ai_backend::{CommentCall, ModeCall};
use gpui_kit::{prelude::FluentBuilder as _, *};

use crate::{
    editor_pane::{EditorPane, PaneEvent, PaneToastAction},
    theme::Roles,
    ui::{self, ButtonKind},
};

pub const NOTES_PANEL_WIDTH: f32 = 320.;
const MARK_SIZE: f32 = 24.;
const MARK_GAP: f32 = 8.;
const NOTE_CARD_HEIGHT: f32 = 160.;

/// A Review that has not answered yet: the decision, then the Comment call.
pub struct PendingReview {
    focus: String,
    changes: ChangeSet,
    _task: Task<()>,
}

#[derive(Clone, Debug)]
pub struct ReviewModeState {
    pub choice: ModeChoice,
    pub focus: String,
}

impl EditorPane {
    pub fn start_review(&mut self, focus: String, cx: &mut Context<Self>) {
        if self.busy().is_some() {
            return;
        }
        let focus = focus.trim().to_string();
        self.mode = None;
        self.jobs.review_started = Some(std::time::Instant::now());
        if let Some(choice) = mode_without_decision(&focus) {
            self.run_mode(choice, focus, cx);
            return;
        }
        let ai = Arc::clone(&self.ai);
        let call = ModeCall {
            task: mode_task_label(&focus),
            focus: focus.clone(),
        };
        let work = cx.background_spawn(async move { ai.review_mode(call) });
        let decided_focus = focus.clone();
        let task = cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| this.on_mode(decided_focus, result, cx))
                .log_warn();
        });
        self.pending = Some(PendingReview {
            focus,
            changes: ChangeSet::empty(self.text(cx).len()),
            _task: task,
        });
        cx.emit(PaneEvent::Changed);
        cx.notify();
    }

    fn on_mode(
        &mut self,
        focus: String,
        result: Result<ModeChoice, ClientError>,
        cx: &mut Context<Self>,
    ) {
        if self.pending.take().is_none() {
            return;
        }
        let choice = result.unwrap_or_else(|error| {
            log::warn!("The review mode decision failed, commenting instead: {error:?}");
            ModeChoice {
                mode: ReviewMode::Comment,
                source: ModeSource::Default,
            }
        });
        self.run_mode(choice, focus, cx);
    }

    fn run_mode(&mut self, choice: ModeChoice, focus: String, cx: &mut Context<Self>) {
        self.mode = Some(ReviewModeState {
            choice,
            focus: focus.clone(),
        });
        self.set_notes(None, cx);
        match choice.mode {
            ReviewMode::Revise => self.start_revise(focus, cx),
            ReviewMode::Comment => self.start_comment(focus, cx),
        }
        cx.emit(PaneEvent::Changed);
        cx.notify();
    }

    fn start_comment(&mut self, focus: String, cx: &mut Context<Self>) {
        let text = self.text(cx);
        if review_too_large(text.len()) {
            cx.emit(PaneEvent::Toast {
                text: review_too_large_text(text.len()),
                tone: ui::ToastTone::Neutral,
                actions: Vec::new(),
            });
            return;
        }
        let blocks = extract_blocks(&text);
        let call = CommentCall {
            task: comment_task_label(&focus),
            parts: build_comment_prompt(&CommentPromptInput {
                blocks: blocks.clone(),
                focus: Some(focus.clone()),
            }),
            blocks,
        };
        let ai = Arc::clone(&self.ai);
        let work = cx.background_spawn(async move { ai.comment(call) });
        let task = cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| this.on_notes(result, cx))
                .log_warn();
        });
        self.set_notes(None, cx);
        self.pending = Some(PendingReview {
            focus,
            changes: ChangeSet::empty(text.len()),
            _task: task,
        });
    }

    fn on_notes(&mut self, result: Result<NoteBoard, ClientError>, cx: &mut Context<Self>) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        let toast = match result {
            Ok(board) if board.notes.is_empty() && board.general.is_empty() => Some((
                "Comments: nothing to note".to_owned(),
                ui::ToastTone::Neutral,
                Vec::new(),
            )),
            Ok(board) => {
                self.set_notes(Some(board.map_through(&pending.changes)), cx);
                None
            }
            Err(error) => Some((
                describe_error(&error),
                ui::ToastTone::Danger,
                vec![PaneToastAction::RetryReview(pending.focus)],
            )),
        };
        if let Some((text, tone, actions)) = toast {
            cx.emit(PaneEvent::Toast {
                text,
                tone,
                actions,
            });
        }
        cx.emit(PaneEvent::Changed);
        cx.notify();
    }

    /// Runs a failed Review again in the mode it ran in, which may be the writer's choice,
    /// without asking the decision model again.
    pub fn retry_review(&mut self, focus: String, cx: &mut Context<Self>) {
        let focus = focus.trim().to_string();
        match self.mode.clone() {
            Some(state) if state.focus == focus && self.busy().is_none() => {
                self.jobs.review_started = Some(std::time::Instant::now());
                self.run_mode(state.choice, focus, cx);
            }
            _ => self.start_review(focus, cx),
        }
    }

    /// Takes a Note off the board once the Suggestion asked for from it is accepted.
    pub(crate) fn resolve_rewritten_note(&mut self, suggestion: &str, cx: &mut Context<Self>) {
        let Some(index) = self
            .note_rewrites
            .iter()
            .position(|(id, _)| id == suggestion)
        else {
            return;
        };
        let (_, number) = self.note_rewrites.remove(index);
        let next = self.notes.as_ref().map(|board| board.without(number));
        self.set_notes(next, cx);
    }

    /// `Some` while a Review runs, with the number of Suggestions it added so far.
    pub fn busy(&self) -> Option<usize> {
        self.review
            .as_ref()
            .map(|run| run.added)
            .or(self.pending.as_ref().map(|_| 0))
    }

    /// Stops a Review that has not answered yet; true when there was one.
    pub(crate) fn stop_pending(&mut self) -> bool {
        self.pending.take().is_some()
    }

    /// The header chip's text while a Review or its Notes are on screen.
    pub fn mode_chip(&self) -> Option<String> {
        let state = self.mode.as_ref()?;
        let shown = self.notes.is_some() || self.busy().is_some() || self.open_review_count() > 0;
        shown.then(|| state.choice.chip_label())
    }

    /// Runs the Review again in the other mode, when the decision read the Focus wrong.
    pub fn switch_mode(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.mode.clone() else {
            return;
        };
        self.stop_review_quietly(cx);
        self.pending = None;
        self.jobs.review_started = Some(std::time::Instant::now());
        self.run_mode(
            ModeChoice::by_writer(state.choice.mode.other()),
            state.focus,
            cx,
        );
        self.refresh_decorations(cx);
    }

    pub(crate) fn map_notes(&mut self, changes: &ChangeSet) {
        if let Some(pending) = self.pending.as_mut() {
            pending.changes = pending.changes.compose(changes);
        }
        if let Some(board) = self.notes.as_ref() {
            self.notes = Some(board.map_through(changes));
        }
    }

    fn set_notes(&mut self, board: Option<NoteBoard>, cx: &mut Context<Self>) {
        self.notes = board.filter(|board| !board.is_empty());
        self.refresh_decorations(cx);
        cx.emit(PaneEvent::Changed);
        cx.notify();
    }

    pub fn close_notes(&mut self, cx: &mut Context<Self>) {
        self.set_notes(None, cx);
    }

    fn pick_note(&mut self, number: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(board) = self.notes.as_ref() else {
            return;
        };
        let open = board.active != Some(number);
        let next = board.pick(open.then_some(number));
        let at = next.active_note().map(|note| note.from);
        self.set_notes(Some(next), cx);
        if let Some(at) = at {
            self.editor.update(cx, |state, cx| {
                state.set_selected_range(at..at, cx);
                state.focus(window, cx);
            });
        }
    }

    fn dismiss_note(&mut self, number: usize, window: &mut Window, cx: &mut Context<Self>) {
        let next = self.notes.as_ref().map(|board| board.without(number));
        self.set_notes(next, cx);
        self.focus(window, cx);
    }

    fn show_all_notes(&mut self, cx: &mut Context<Self>) {
        let next = self.notes.as_ref().map(NoteBoard::showing_all);
        self.set_notes(next, cx);
    }

    fn rewrite_note(&mut self, number: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(board) = self.notes.clone() else {
            return;
        };
        let Some(note) = board.note(number).cloned() else {
            return;
        };
        if note.from >= note.to {
            cx.emit(PaneEvent::Toast {
                text: "The text of this Note was deleted".to_owned(),
                tone: ui::ToastTone::Neutral,
                actions: Vec::new(),
            });
            self.dismiss_note(number, window, cx);
            return;
        }
        self.set_notes(Some(board.pick(None)), cx);
        let range = note.from..note.to;
        self.editor
            .update(cx, |state, cx| state.set_selected_range(range, cx));
        let asked = self.next_id;
        self.ask_with(
            Instruction::FreeText {
                text: rewrite_instruction(&note),
            },
            window,
            cx,
        );
        if self.next_id > asked {
            self.note_rewrites
                .push((format!("s{}", self.next_id), number));
        }
        self.focus(window, cx);
    }

    pub(crate) fn active_note_range(&self) -> Option<std::ops::Range<usize>> {
        self.notes
            .as_ref()
            .and_then(NoteBoard::active_note)
            .map(|note| note.from..note.to)
    }

    pub(crate) fn render_note_marks(
        &self,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(board) = self.notes.as_ref() else {
            return Vec::new();
        };
        let pane = self.pane_bounds();
        let left = (self.content_left() - pane.origin.x - px(MARK_SIZE + MARK_GAP)).max(px(0.));
        let state = self.editor.read(cx);
        board
            .shown()
            .iter()
            .filter_map(|note| {
                let line = state.range_to_bounds(&(note.from..note.from))?;
                let top = line.top() - pane.origin.y + (line.size.height - px(MARK_SIZE)) / 2.;
                if top < px(0.) || top > pane.size.height - px(MARK_SIZE) {
                    return None;
                }
                let number = note.number;
                Some(
                    ui::note_mark(
                        SharedString::from(format!("note-mark-{number}")),
                        number,
                        match (board.active == Some(number), note.stale) {
                            (true, _) => ui::MarkState::Open,
                            (false, true) => ui::MarkState::Stale,
                            (false, false) => ui::MarkState::Plain,
                        },
                        roles,
                    )
                    .absolute()
                    .left(left)
                    .top(top)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.pick_note(number, window, cx);
                    }))
                    .into_any_element(),
                )
            })
            .collect()
    }

    fn note_card_origin(&self, note: &Note, cx: &App) -> Option<Point<Pixels>> {
        let state = self.editor.read(cx);
        let line = state.range_to_bounds(&(note.from..note.from))?;
        let end = state.range_to_bounds(&(note.to..note.to)).unwrap_or(line);
        let pane = self.pane_bounds();
        let width = px(MARGIN_CARD_WIDTH as f32);
        let margin = self.card_placement() == CardPlacement::Margin;
        let (left, bottom) = if margin {
            let content_left = f64::from(f32::from(self.content_left()));
            (px(margin_card_left(content_left) as f32), line.top())
        } else {
            (self.content_right() - width, end.bottom())
        };
        let to_f64 = |value: Pixels| f64::from(f32::from(value));
        let placed = place_popover(&PopoverInput {
            anchor: Rect {
                left: to_f64(left - pane.origin.x),
                right: to_f64(left - pane.origin.x),
                top: to_f64(line.top() - pane.origin.y),
                bottom: to_f64(bottom - pane.origin.y),
            },
            width: to_f64(width),
            height: f64::from(NOTE_CARD_HEIGHT),
            area: Viewport {
                width: to_f64(pane.size.width),
                height: to_f64(pane.size.height),
            },
            side: Side::Below,
            flip: !margin,
            gap: if margin { 0. } else { 8. },
        })?;
        Some(point(px(placed.left as f32), px(placed.top as f32)))
    }

    pub(crate) fn render_note_card(
        &self,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let note = self.notes.as_ref()?.active_note()?.clone();
        let origin = self.note_card_origin(&note, cx)?;
        let number = note.number;
        let card = ui::dialog_frame(
            SharedString::from(format!("note-card-{number}")),
            px(MARGIN_CARD_WIDTH as f32),
            roles,
        )
        .absolute()
        .left(origin.x)
        .top(origin.y)
        .p(px(12.))
        .gap(px(8.))
        .child(
            ui::row()
                .gap(px(8.))
                .child(ui::ai_label(note.kind.label(), roles))
                .when(note.stale, |this| {
                    this.child(ui::meta("The text changed since this note", roles))
                }),
        )
        .child(ui::title(note.title.clone(), roles))
        .child(div().child(note.comment.clone()))
        .child(
            ui::row()
                .gap(px(8.))
                .flex_wrap()
                .child(
                    ui::small_button(
                        SharedString::from(format!("note-rewrite-{number}")),
                        "Suggest rewrite",
                        ButtonKind::Primary,
                        roles,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.rewrite_note(number, window, cx);
                    })),
                )
                .child(
                    ui::small_button(
                        SharedString::from(format!("note-dismiss-{number}")),
                        "Dismiss",
                        ButtonKind::Default,
                        roles,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.dismiss_note(number, window, cx);
                    })),
                ),
        );
        Some(
            ui::enter(
                "note-card-in",
                card,
                ui::Enter::Slide(if self.card_placement() == CardPlacement::Margin {
                    8.
                } else {
                    -8.
                }),
                ui::MOTION_ENTER,
            )
            .into_any_element(),
        )
    }

    pub(crate) fn render_notes_panel(
        &self,
        roles: Roles,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let board = self.notes.as_ref()?;
        let general = board.general.iter().map(|note| {
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(ui::ai_label(note.topic.clone(), roles))
                .child(div().child(note.text.clone()))
        });
        let content = div()
            .id("notes-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.notes_scroll)
            .flex()
            .flex_col()
            .gap(px(16.))
            .when(!board.general.is_empty(), |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .child(ui::title("General notes", roles))
                        .children(general),
                )
            })
            .when(!board.notes.is_empty(), |this| {
                this.child(self.render_note_list(board, roles, cx))
            });
        Some(
            ui::side_panel("notes-panel", px(NOTES_PANEL_WIDTH), roles)
                .child(
                    ui::row()
                        .justify_between()
                        .child(ui::title("Editor's notes", roles))
                        .child(
                            ui::small_button("notes-close", "Close", ButtonKind::Quiet, roles)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.close_notes(cx);
                                    this.focus(window, cx);
                                })),
                        ),
                )
                .child(ui::scrolled_rest(content, &self.notes_scroll))
                .into_any_element(),
        )
    }

    fn render_note_list(&self, board: &NoteBoard, roles: Roles, cx: &mut Context<Self>) -> Div {
        let hidden = board.hidden_count();
        let summary = if board.notes.len() > board.shown().len() {
            format!(
                "{} of {}, most important first",
                board.shown().len(),
                board.notes.len()
            )
        } else {
            "Most important first".to_owned()
        };
        let rows = board.shown().iter().map(|note| {
            let number = note.number;
            ui::note_row(
                SharedString::from(format!("note-row-{number}")),
                board.active == Some(number),
                roles,
            )
            .child(if note.stale {
                ui::meta(number.to_string(), roles).w(px(16.))
            } else {
                ui::ai_label(number.to_string(), roles).w(px(16.))
            })
            .child(div().flex_1().min_w_0().child(note.title.clone()))
            .child(ui::meta(note.kind.label(), roles))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.pick_note(number, window, cx);
            }))
        });
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(ui::title("Notes", roles))
            .child(ui::meta(summary, roles).mb(px(4.)))
            .children(rows)
            .when(hidden > 0, |this| {
                this.child(
                    ui::small_button(
                        "notes-show-more",
                        format!("Show {hidden} more"),
                        ButtonKind::Default,
                        roles,
                    )
                    .mt(px(8.))
                    .on_click(cx.listener(|this, _, _, cx| this.show_all_notes(cx))),
                )
            })
    }
}
