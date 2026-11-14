use std::collections::BTreeSet;

use super::error_message::describe_error;
use super::types::{
    ClientError, Instruction, ReadySuggestion, RefineState, Suggestion, SuggestionDraft,
    SuggestionId, SuggestionOrigin, SuggestionResult, SuggestionsState, TargetSource,
    ValidatedEdit,
};
use crate::blocks::{Block, Range, find_frontmatter, touches_frontmatter};
use crate::change_set::{Assoc, ChangeDesc, TextChange};
use crate::text::js_trim;

pub const DEFAULT_REVIEW_FOCUS: &str =
    "Review for clarity, concision, grammar and tone consistency.";

pub const MAX_REVIEW_CHARS: usize = 120_000;

pub fn review_instruction(focus: &str) -> Instruction {
    let trimmed = js_trim(focus);
    Instruction::FreeText {
        text: if trimmed.is_empty() {
            DEFAULT_REVIEW_FOCUS.to_string()
        } else {
            trimmed.to_string()
        },
    }
}

fn overlaps(a: Range, b: Range) -> bool {
    a.from < b.to && b.from < a.to
}

fn range_of(from: usize, to: usize) -> Range {
    Range { from, to }
}

pub fn occupied_block_ids(state: &SuggestionsState, blocks: &[Block]) -> BTreeSet<String> {
    blocks
        .iter()
        .filter(|block| {
            state
                .items
                .iter()
                .any(|item| overlaps(range_of(item.from, item.to), range_of(block.from, block.to)))
        })
        .map(|block| block.id.clone())
        .collect()
}

fn touches_interior(range: Range, changes: &ChangeDesc) -> bool {
    changes
        .changed_ranges()
        .iter()
        .any(|changed| changed.from_a < range.to && changed.to_a > range.from)
}

pub fn map_block_range(block: Range, changes: &ChangeDesc) -> Option<Range> {
    if touches_interior(block, changes) {
        None
    } else {
        Some(Range {
            from: changes.map_pos(block.from, Assoc::After),
            to: changes.map_pos(block.to, Assoc::Before),
        })
    }
}

pub struct PlaceReviewEditInput<'a> {
    pub state: &'a SuggestionsState,
    pub edit: &'a ValidatedEdit,
    pub changes: &'a ChangeDesc,
    pub doc: &'a str,
    pub id: &'a str,
    pub focus: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    TextChanged,
    Frontmatter,
    Occupied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewPlacement {
    Placed {
        draft: SuggestionDraft,
        result: SuggestionResult,
    },
    Dropped {
        reason: DropReason,
    },
}

pub fn place_review_edit(input: &PlaceReviewEditInput) -> ReviewPlacement {
    let PlaceReviewEditInput {
        state,
        edit,
        changes,
        doc,
        id,
        focus,
    } = input;
    let block_range = range_of(edit.block.from, edit.block.to);
    let Some(range) = map_block_range(block_range, changes) else {
        return ReviewPlacement::Dropped {
            reason: DropReason::TextChanged,
        };
    };
    if doc.get(range.from..range.to) != Some(edit.block.text.as_str()) {
        return ReviewPlacement::Dropped {
            reason: DropReason::TextChanged,
        };
    }
    if touches_frontmatter(find_frontmatter(doc).as_ref(), range) {
        return ReviewPlacement::Dropped {
            reason: DropReason::Frontmatter,
        };
    }
    if state
        .items
        .iter()
        .any(|item| overlaps(range_of(item.from, item.to), range))
    {
        return ReviewPlacement::Dropped {
            reason: DropReason::Occupied,
        };
    }
    ReviewPlacement::Placed {
        draft: SuggestionDraft {
            id: (*id).to_string(),
            from: range.from,
            to: range.to,
            source: TargetSource::Block,
            original: edit.block.text.clone(),
            instruction: review_instruction(focus),
            origin: Some(SuggestionOrigin::Review),
        },
        result: SuggestionResult {
            replacement: edit.replacement.clone(),
            rationale: edit.rationale.clone(),
            unchanged: false,
        },
    }
}

fn ready_review_suggestions(state: &SuggestionsState) -> Vec<&ReadySuggestion> {
    state
        .items
        .iter()
        .filter_map(Suggestion::as_ready)
        .filter(|item| {
            item.origin == SuggestionOrigin::Review
                && !matches!(item.refine, RefineState::Working { .. })
        })
        .collect()
}

pub fn open_review_count(state: &SuggestionsState) -> usize {
    ready_review_suggestions(state).len()
}

fn in_document_order(state: &SuggestionsState) -> Vec<&ReadySuggestion> {
    let mut ordered = ready_review_suggestions(state);
    ordered.sort_by_key(|item| item.from);
    ordered
}

pub fn first_review_suggestion(state: &SuggestionsState) -> Option<&ReadySuggestion> {
    in_document_order(state).into_iter().next()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptAllPlan {
    pub ids: Vec<SuggestionId>,
    pub changes: Vec<TextChange>,
}

pub fn accept_all_plan(state: &SuggestionsState) -> AcceptAllPlan {
    let ordered = in_document_order(state);
    AcceptAllPlan {
        ids: ordered.iter().map(|item| item.id.clone()).collect(),
        changes: ordered
            .iter()
            .map(|item| TextChange::new(item.from, item.to, item.replacement.clone()))
            .collect(),
    }
}

fn pluralize(count: usize) -> String {
    match count {
        0 => "no suggestions".to_string(),
        1 => "1 suggestion".to_string(),
        _ => format!("{count} suggestions"),
    }
}

pub fn review_summary(added: usize, stopped: bool) -> String {
    format!(
        "Review{}: {}",
        if stopped { " stopped" } else { "" },
        pluralize(added)
    )
}

pub fn review_task_label(focus: &str) -> String {
    let trimmed = js_trim(focus);
    if trimmed.is_empty() {
        "review document → default criteria".to_string()
    } else {
        format!("review document → focus: {trimmed}")
    }
}

pub fn review_notice_text(added: usize, error: Option<&ClientError>) -> String {
    match error {
        None => review_summary(added, false),
        Some(error) if added == 0 => describe_error(error),
        Some(error) => format!(
            "{}. {}",
            review_summary(added, false),
            describe_error(error)
        ),
    }
}

pub fn review_too_large(document_length: usize) -> bool {
    document_length > MAX_REVIEW_CHARS
}

pub fn review_too_large_text(document_length: usize) -> String {
    format!(
        "Review stopped: the Document is {} KB and a pass takes up to {} KB. Select a part and ask instead.",
        (document_length as f64 / 1000.0).round(),
        MAX_REVIEW_CHARS / 1000
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::extract_blocks;
    use crate::change_set::ChangeSet;
    use crate::suggestions::state::{
        begin_refine, begin_suggestion, empty_suggestions, fail_suggestion, find_suggestion,
        resolve_suggestion,
    };

    fn doc() -> String {
        [
            "# Title",
            "",
            "First paragraph here.",
            "",
            "Second paragraph here.",
            "",
            "```js",
            "const a = 1",
            "```",
            "",
            "Third paragraph here.",
            "",
        ]
        .join("\n")
    }

    struct Fixture {
        doc: String,
        blocks: Vec<Block>,
    }

    impl Fixture {
        fn new() -> Self {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            Self { doc, blocks }
        }

        fn named(&self, text: &str) -> Block {
            self.blocks
                .iter()
                .find(|block| block.text == text)
                .unwrap_or_else(|| panic!("no block {text}"))
                .clone()
        }

        fn first(&self) -> Block {
            self.named("First paragraph here.")
        }

        fn second(&self) -> Block {
            self.named("Second paragraph here.")
        }

        fn third(&self) -> Block {
            self.named("Third paragraph here.")
        }

        fn change(&self, from: usize, to: usize, insert: &str) -> ChangeSet {
            ChangeSet::of(&[TextChange::new(from, to, insert)], self.doc.len())
        }

        fn unchanged(&self) -> ChangeSet {
            ChangeSet::empty(self.doc.len())
        }

        fn draft(&self, id: &str, from: usize, to: usize) -> SuggestionDraft {
            SuggestionDraft {
                id: id.into(),
                from,
                to,
                source: TargetSource::Selection,
                original: self.doc[from..to].to_string(),
                instruction: Instruction::FreeText { text: "x".into() },
                origin: None,
            }
        }

        fn review_draft(&self, id: &str, from: usize, to: usize) -> SuggestionDraft {
            SuggestionDraft {
                origin: Some(SuggestionOrigin::Review),
                ..self.draft(id, from, to)
            }
        }
    }

    fn edit_on(block: &Block, replacement: Option<&str>) -> ValidatedEdit {
        ValidatedEdit {
            block_id: block.id.clone(),
            replacement: replacement
                .map_or_else(|| format!("{} Better.", block.text), str::to_string),
            rationale: "Clearer.".into(),
            block: block.clone(),
        }
    }

    fn result(replacement: &str) -> SuggestionResult {
        SuggestionResult {
            replacement: replacement.into(),
            rationale: "Because.".into(),
            unchanged: false,
        }
    }

    fn state_of(drafts: Vec<SuggestionDraft>) -> SuggestionsState {
        drafts
            .into_iter()
            .fold(empty_suggestions(), |state, draft| {
                begin_suggestion(&state, draft)
            })
    }

    fn ready_of(state: &SuggestionsState, id: &str, replacement: &str) -> SuggestionsState {
        resolve_suggestion(state, id, &result(replacement))
    }

    fn rng(from: usize, to: usize) -> Range {
        Range { from, to }
    }

    mod review_instruction_tests {
        use super::*;

        #[test]
        fn uses_the_focus_as_a_free_text_instruction() {
            assert_eq!(
                review_instruction("  tone only "),
                Instruction::FreeText {
                    text: "tone only".into()
                }
            );
        }

        #[test]
        fn falls_back_to_the_default_criteria_without_a_focus() {
            assert_eq!(
                review_instruction(""),
                Instruction::FreeText {
                    text: DEFAULT_REVIEW_FOCUS.into()
                }
            );
            assert_eq!(review_instruction("   ").text(), DEFAULT_REVIEW_FOCUS);
        }
    }

    mod occupied_block_ids_tests {
        use super::*;

        fn set(ids: &[&str]) -> BTreeSet<String> {
            ids.iter().map(|id| (*id).to_string()).collect()
        }

        #[test]
        fn lists_blocks_that_overlap_a_pending_ready_or_failed_suggestion() {
            let f = Fixture::new();
            let base = state_of(vec![
                f.draft("a", f.first().from, f.first().to),
                f.draft("b", f.second().from + 3, f.second().from + 8),
                f.draft("c", f.third().from, f.third().to),
            ]);
            let state = fail_suggestion(
                &ready_of(&base, "b", "x"),
                "c",
                ClientError::Network {
                    message: "offline".into(),
                },
            );
            assert_eq!(
                occupied_block_ids(&state, &f.blocks),
                set(&[&f.first().id, &f.second().id, &f.third().id])
            );
        }

        #[test]
        fn leaves_free_blocks_out_including_a_block_that_only_touches_a_suggestion() {
            let f = Fixture::new();
            let state = state_of(vec![f.draft("a", f.first().from, f.first().to)]);
            assert_eq!(occupied_block_ids(&state, &f.blocks), set(&[&f.first().id]));
        }

        #[test]
        fn is_empty_without_suggestions() {
            let f = Fixture::new();
            assert_eq!(
                occupied_block_ids(&empty_suggestions(), &f.blocks),
                set(&[])
            );
        }
    }

    mod map_block_range_tests {
        use super::*;

        fn block_range(block: &Block) -> Range {
            rng(block.from, block.to)
        }

        #[test]
        fn keeps_the_range_when_nothing_changed() {
            let f = Fixture::new();
            let second = f.second();
            assert_eq!(
                map_block_range(block_range(&second), &f.unchanged()),
                Some(rng(second.from, second.to))
            );
        }

        #[test]
        fn shifts_the_range_after_an_insertion_before_it() {
            let f = Fixture::new();
            let second = f.second();
            assert_eq!(
                map_block_range(block_range(&second), &f.change(0, 0, "Intro.\n\n")),
                Some(rng(second.from + 8, second.to + 8))
            );
        }

        #[test]
        fn shifts_the_range_back_after_a_deletion_before_it() {
            let f = Fixture::new();
            let second = f.second();
            let first = f.first();
            assert_eq!(
                map_block_range(
                    block_range(&second),
                    &f.change(first.from, first.from + 6, "")
                ),
                Some(rng(second.from - 6, second.to - 6))
            );
        }

        #[test]
        fn keeps_the_range_for_an_edit_after_it() {
            let f = Fixture::new();
            let first = f.first();
            let third = f.third();
            assert_eq!(
                map_block_range(block_range(&first), &f.change(third.from, third.from, "x")),
                Some(rng(first.from, first.to))
            );
        }

        #[test]
        fn treats_typing_at_the_very_start_or_end_of_the_block_as_outside() {
            let f = Fixture::new();
            let second = f.second();
            assert_eq!(
                map_block_range(
                    block_range(&second),
                    &f.change(second.from, second.from, "x")
                ),
                Some(rng(second.from + 1, second.to + 1))
            );
            assert_eq!(
                map_block_range(block_range(&second), &f.change(second.to, second.to, "x")),
                Some(rng(second.from, second.to))
            );
        }

        #[test]
        fn returns_none_when_text_is_inserted_inside_the_block() {
            let f = Fixture::new();
            let second = f.second();
            assert_eq!(
                map_block_range(
                    block_range(&second),
                    &f.change(second.from + 4, second.from + 4, "x")
                ),
                None
            );
        }

        #[test]
        fn returns_none_when_text_inside_the_block_is_deleted_or_replaced() {
            let f = Fixture::new();
            let second = f.second();
            assert_eq!(
                map_block_range(
                    block_range(&second),
                    &f.change(second.from + 2, second.from + 5, "")
                ),
                None
            );
            assert_eq!(
                map_block_range(
                    block_range(&second),
                    &f.change(second.from, second.to, "Gone.")
                ),
                None
            );
        }

        #[test]
        fn returns_none_when_an_edit_straddles_the_block_boundary() {
            let f = Fixture::new();
            let second = f.second();
            let first = f.first();
            assert_eq!(
                map_block_range(
                    block_range(&second),
                    &f.change(first.to - 2, second.from + 3, "")
                ),
                None
            );
        }

        #[test]
        fn uses_every_change_since_the_request_started() {
            let f = Fixture::new();
            let second = f.second();
            let third = f.third();
            let first = f.change(0, 0, "Intro.\n\n");
            let later = ChangeSet::of(&[TextChange::insert(third.from + 8, "x")], f.doc.len() + 8);
            assert_eq!(
                map_block_range(block_range(&second), &first.compose(&later)),
                Some(rng(second.from + 8, second.to + 8))
            );
            let inside = ChangeSet::of(
                &[TextChange::insert(second.from + 8 + 2, "z")],
                f.doc.len() + 8,
            );
            assert_eq!(
                map_block_range(block_range(&second), &first.compose(&inside)),
                None
            );
        }
    }

    mod place_review_edit_tests {
        use super::*;

        struct Overrides {
            state: Option<SuggestionsState>,
            changes: Option<ChangeSet>,
            doc: Option<String>,
        }

        fn none() -> Overrides {
            Overrides {
                state: None,
                changes: None,
                doc: None,
            }
        }

        fn place(f: &Fixture, edit: &ValidatedEdit, overrides: Overrides) -> ReviewPlacement {
            let state = overrides.state.unwrap_or_else(empty_suggestions);
            let changes = overrides.changes.unwrap_or_else(|| f.unchanged());
            let doc = overrides.doc.unwrap_or_else(|| f.doc.clone());
            place_review_edit(&PlaceReviewEditInput {
                state: &state,
                edit,
                changes: &changes,
                doc: &doc,
                id: "s1",
                focus: "tone only",
            })
        }

        fn placed_range(placement: &ReviewPlacement) -> (usize, usize) {
            match placement {
                ReviewPlacement::Placed { draft, .. } => (draft.from, draft.to),
                other => panic!("expected a placement, got {other:?}"),
            }
        }

        fn dropped(reason: DropReason) -> ReviewPlacement {
            ReviewPlacement::Dropped { reason }
        }

        #[test]
        fn places_the_edit_on_its_block_as_a_review_suggestion() {
            let f = Fixture::new();
            let second = f.second();
            let placed = place(&f, &edit_on(&second, Some("Second one.")), none());
            assert_eq!(
                placed,
                ReviewPlacement::Placed {
                    draft: SuggestionDraft {
                        id: "s1".into(),
                        from: second.from,
                        to: second.to,
                        source: TargetSource::Block,
                        original: second.text.clone(),
                        instruction: Instruction::FreeText {
                            text: "tone only".into()
                        },
                        origin: Some(SuggestionOrigin::Review),
                    },
                    result: SuggestionResult {
                        replacement: "Second one.".into(),
                        rationale: "Clearer.".into(),
                        unchanged: false,
                    },
                }
            );
        }

        #[test]
        fn maps_the_block_range_through_edits_made_elsewhere_during_the_stream() {
            let f = Fixture::new();
            let second = f.second();
            let changes = f.change(0, 0, "Intro.\n\n");
            let doc = changes.apply(&f.doc);
            let placed = place(
                &f,
                &edit_on(&second, None),
                Overrides {
                    changes: Some(changes),
                    doc: Some(doc),
                    ..none()
                },
            );
            assert_eq!(placed_range(&placed), (second.from + 8, second.to + 8));
        }

        #[test]
        fn maps_through_an_edit_after_the_block_too() {
            let f = Fixture::new();
            let second = f.second();
            let changes = f.change(f.third().to, f.third().to, " More.");
            let doc = changes.apply(&f.doc);
            let placed = place(
                &f,
                &edit_on(&second, None),
                Overrides {
                    changes: Some(changes),
                    doc: Some(doc),
                    ..none()
                },
            );
            assert_eq!(placed_range(&placed), (second.from, second.to));
        }

        #[test]
        fn drops_the_edit_when_the_user_edited_inside_its_block() {
            let f = Fixture::new();
            let second = f.second();
            let changes = f.change(second.from + 3, second.from + 3, "zz");
            let doc = changes.apply(&f.doc);
            assert_eq!(
                place(
                    &f,
                    &edit_on(&second, None),
                    Overrides {
                        changes: Some(changes),
                        doc: Some(doc),
                        ..none()
                    }
                ),
                dropped(DropReason::TextChanged)
            );
        }

        #[test]
        fn drops_the_edit_when_the_mapped_text_no_longer_matches_the_block() {
            let f = Fixture::new();
            assert_eq!(
                place(
                    &f,
                    &edit_on(&f.second(), None),
                    Overrides {
                        doc: Some(f.doc.replace("Second", "Sekond")),
                        ..none()
                    }
                ),
                dropped(DropReason::TextChanged)
            );
        }

        #[test]
        fn drops_the_edit_when_the_user_turned_its_block_into_a_frontmatter() {
            let f = Fixture::new();
            let first = f.first();
            let changes = ChangeSet::of(
                &[
                    TextChange::insert(0, "---\nname: demo\n"),
                    TextChange::insert(first.to, "\n---"),
                ],
                f.doc.len(),
            );
            let doc = changes.apply(&f.doc);
            assert!(find_frontmatter(&doc).unwrap().to > first.to);
            assert_eq!(
                place(
                    &f,
                    &edit_on(&first, None),
                    Overrides {
                        changes: Some(changes),
                        doc: Some(doc),
                        ..none()
                    }
                ),
                dropped(DropReason::Frontmatter)
            );
        }

        #[test]
        fn places_the_edit_after_a_frontmatter_the_user_typed_above_it() {
            let f = Fixture::new();
            let second = f.second();
            let changes = f.change(0, 0, "---\nname: demo\n---\n\n");
            let doc = changes.apply(&f.doc);
            let placed = place(
                &f,
                &edit_on(&second, None),
                Overrides {
                    changes: Some(changes),
                    doc: Some(doc),
                    ..none()
                },
            );
            assert_eq!(placed_range(&placed), (second.from + 20, second.to + 20));
        }

        #[test]
        fn drops_the_edit_when_a_suggestion_now_overlaps_the_mapped_block() {
            let f = Fixture::new();
            let second = f.second();
            let state = state_of(vec![f.draft("other", second.from + 2, second.from + 6)]);
            assert_eq!(
                place(
                    &f,
                    &edit_on(&second, None),
                    Overrides {
                        state: Some(state),
                        ..none()
                    }
                ),
                dropped(DropReason::Occupied)
            );
        }

        #[test]
        fn drops_the_edit_when_a_pending_suggestion_covers_the_block() {
            let f = Fixture::new();
            let second = f.second();
            let state = state_of(vec![f.draft("other", second.from, second.to)]);
            assert_eq!(
                place(
                    &f,
                    &edit_on(&second, None),
                    Overrides {
                        state: Some(state),
                        ..none()
                    }
                ),
                dropped(DropReason::Occupied)
            );
        }

        #[test]
        fn places_the_edit_next_to_suggestions_on_other_blocks() {
            let f = Fixture::new();
            let state = state_of(vec![
                f.draft("a", f.first().from, f.first().to),
                f.draft("c", f.third().from, f.third().to),
            ]);
            assert!(matches!(
                place(
                    &f,
                    &edit_on(&f.second(), None),
                    Overrides {
                        state: Some(state),
                        ..none()
                    }
                ),
                ReviewPlacement::Placed { .. }
            ));
        }

        #[test]
        fn checks_the_user_edit_before_the_overlap() {
            let f = Fixture::new();
            let second = f.second();
            let changes = f.change(second.from + 3, second.from + 3, "zz");
            let doc = changes.apply(&f.doc);
            let state = state_of(vec![f.draft("other", second.from, second.to)]);
            assert_eq!(
                place(
                    &f,
                    &edit_on(&second, None),
                    Overrides {
                        state: Some(state),
                        changes: Some(changes),
                        doc: Some(doc),
                    }
                ),
                dropped(DropReason::TextChanged)
            );
        }

        #[test]
        fn yields_a_suggestion_state_with_the_review_origin_once_applied() {
            let f = Fixture::new();
            let placed = place(&f, &edit_on(&f.first(), Some("First.")), none());
            let ReviewPlacement::Placed { draft, result } = placed else {
                panic!("expected a placement");
            };
            let state = resolve_suggestion(
                &begin_suggestion(&empty_suggestions(), draft.clone()),
                &draft.id,
                &result,
            );
            let found = find_suggestion(&state, "s1").unwrap();
            let ready = found.as_ready().expect("ready");
            assert_eq!(ready.origin, SuggestionOrigin::Review);
            assert_eq!(ready.replacement, "First.");
        }
    }

    mod accept_all_and_reject_all_scope {
        use super::*;

        fn review_state(f: &Fixture) -> SuggestionsState {
            let state = state_of(vec![
                f.draft("ask", f.first().from, f.first().to),
                f.review_draft("r2", f.third().from, f.third().to),
                f.review_draft("r1", f.second().from, f.second().to),
            ]);
            ready_of(
                &ready_of(&ready_of(&state, "ask", "Asked."), "r2", "Three."),
                "r1",
                "Two.",
            )
        }

        fn tighter() -> Instruction {
            Instruction::FreeText {
                text: "tighter".into(),
            }
        }

        #[test]
        fn counts_the_open_review_suggestions() {
            let f = Fixture::new();
            assert_eq!(open_review_count(&review_state(&f)), 2);
            assert_eq!(open_review_count(&empty_suggestions()), 0);
        }

        #[test]
        fn ignores_review_suggestions_that_are_not_ready() {
            let f = Fixture::new();
            let state = state_of(vec![f.review_draft("r", f.second().from, f.second().to)]);
            assert_eq!(open_review_count(&state), 0);
            assert_eq!(
                accept_all_plan(&state),
                AcceptAllPlan {
                    ids: vec![],
                    changes: vec![]
                }
            );
        }

        #[test]
        fn leaves_out_a_review_suggestion_whose_refine_is_in_flight_from_the_plan_and_the_count() {
            let f = Fixture::new();
            let refining = begin_refine(&review_state(&f), "r1", tighter());
            assert_eq!(open_review_count(&refining), 1);
            assert_eq!(
                accept_all_plan(&refining),
                AcceptAllPlan {
                    ids: vec!["r2".into()],
                    changes: vec![TextChange::new(f.third().from, f.third().to, "Three.")],
                }
            );
        }

        #[test]
        fn has_nothing_to_accept_when_every_review_suggestion_is_refining() {
            let f = Fixture::new();
            let state = ["r1", "r2"].iter().fold(review_state(&f), |current, id| {
                begin_refine(&current, id, tighter())
            });
            assert_eq!(open_review_count(&state), 0);
            assert_eq!(
                accept_all_plan(&state),
                AcceptAllPlan {
                    ids: vec![],
                    changes: vec![]
                }
            );
        }

        #[test]
        fn plans_one_change_per_ready_review_suggestion_in_document_order() {
            let f = Fixture::new();
            assert_eq!(
                accept_all_plan(&review_state(&f)),
                AcceptAllPlan {
                    ids: vec!["r1".into(), "r2".into()],
                    changes: vec![
                        TextChange::new(f.second().from, f.second().to, "Two."),
                        TextChange::new(f.third().from, f.third().to, "Three."),
                    ],
                }
            );
        }

        #[test]
        fn finds_the_open_review_suggestion_closest_to_the_top_whatever_order_it_arrived_in() {
            let f = Fixture::new();
            assert_eq!(
                first_review_suggestion(&review_state(&f)).map(|found| found.id.as_str()),
                Some("r1")
            );
        }

        #[test]
        fn finds_no_first_review_suggestion_when_none_is_open() {
            let f = Fixture::new();
            assert!(first_review_suggestion(&empty_suggestions()).is_none());
            let pending = state_of(vec![f.review_draft("r", f.second().from, f.second().to)]);
            assert!(first_review_suggestion(&pending).is_none());
        }

        #[test]
        fn plans_changes_that_apply_together_to_the_original_document() {
            let f = Fixture::new();
            let plan = accept_all_plan(&review_state(&f));
            let applied = ChangeSet::of(&plan.changes, f.doc.len()).apply(&f.doc);
            assert!(applied.contains("Two."));
            assert!(applied.contains("Three."));
            assert!(applied.contains("First paragraph here."));
            assert!(!applied.contains("Second paragraph"));
        }
    }

    mod review_summary_tests {
        use super::*;

        #[test]
        fn says_how_many_suggestions_the_pass_produced() {
            assert_eq!(review_summary(4, false), "Review: 4 suggestions");
            assert_eq!(review_summary(1, false), "Review: 1 suggestion");
            assert_eq!(review_summary(0, false), "Review: no suggestions");
        }

        #[test]
        fn says_when_the_user_stopped_the_pass() {
            assert_eq!(review_summary(2, true), "Review stopped: 2 suggestions");
            assert_eq!(review_summary(0, true), "Review stopped: no suggestions");
        }
    }

    mod review_task_label_tests {
        use super::*;

        #[test]
        fn names_the_focus_or_the_default_criteria() {
            assert_eq!(
                review_task_label("tone only"),
                "review document → focus: tone only"
            );
            assert_eq!(
                review_task_label("  "),
                "review document → default criteria"
            );
        }
    }

    mod review_notice_text_tests {
        use super::*;

        #[test]
        fn summarises_a_clean_pass() {
            assert_eq!(review_notice_text(4, None), "Review: 4 suggestions");
            assert_eq!(review_notice_text(0, None), "Review: no suggestions");
        }

        #[test]
        fn gives_the_human_message_when_the_error_came_before_any_suggestion() {
            let text = review_notice_text(
                0,
                Some(&ClientError::OutOfCredit {
                    message: "No credit".into(),
                }),
            );
            assert!(text.to_lowercase().contains("out of credit"));
        }

        #[test]
        fn keeps_the_count_and_adds_the_message_when_the_error_came_after_some() {
            let text = review_notice_text(
                2,
                Some(&ClientError::Timeout {
                    message: "slow".into(),
                }),
            );
            assert!(text.contains("Review: 2 suggestions"));
            assert!(text.to_lowercase().contains("took too long"));
        }
    }

    mod review_size_limit {
        use super::*;

        #[test]
        fn accepts_a_document_up_to_the_limit_and_refuses_one_over_it() {
            assert!(!review_too_large(MAX_REVIEW_CHARS));
            assert!(review_too_large(MAX_REVIEW_CHARS + 1));
        }

        #[test]
        fn covers_the_largest_skill_file_seen_on_the_maintainer_machine_with_room_to_spare() {
            assert!(!review_too_large(60_000));
        }

        #[test]
        fn says_how_big_the_document_is_and_what_to_do() {
            assert_eq!(
                review_too_large_text(130_400),
                "Review stopped: the Document is 130 KB and a pass takes up to 120 KB. Select a part and ask instead."
            );
        }
    }
}
