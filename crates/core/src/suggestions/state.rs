use super::in_flight::{edits_inside, inserts_at, rebase_replacement};
use super::replacement::match_edge_whitespace;
use super::types::{
    ClientError, FailedSuggestion, Instruction, NoticeKind, PendingSuggestion, ReadySuggestion,
    RefineState, RefineStep, Suggestion, SuggestionBase, SuggestionDraft, SuggestionId,
    SuggestionNotice, SuggestionOrigin, SuggestionResult, SuggestionsState,
};
use crate::blocks::{Frontmatter, Range, touches_frontmatter};
use crate::change_set::{Assoc, ChangeDesc};
use crate::diff::adaptive_diff;

pub fn empty_suggestions() -> SuggestionsState {
    SuggestionsState::default()
}

fn with_notice(state: SuggestionsState, kind: NoticeKind) -> SuggestionsState {
    let serial = state.notice.map_or(0, |notice| notice.serial) + 1;
    SuggestionsState {
        notice: Some(SuggestionNotice { serial, kind }),
        ..state
    }
}

fn without(state: &SuggestionsState, id: &str) -> SuggestionsState {
    SuggestionsState {
        items: state
            .items
            .iter()
            .filter(|item| item.id != id)
            .cloned()
            .collect(),
        ..state.clone()
    }
}

fn replace_item(state: &SuggestionsState, next: Suggestion) -> SuggestionsState {
    SuggestionsState {
        items: state
            .items
            .iter()
            .map(|item| {
                if item.id == next.id {
                    next.clone()
                } else {
                    item.clone()
                }
            })
            .collect(),
        ..state.clone()
    }
}

pub fn find_suggestion<'a>(state: &'a SuggestionsState, id: &str) -> Option<&'a Suggestion> {
    state.items.iter().find(|item| item.id == id)
}

fn overlaps(item: &Suggestion, from: usize, to: usize) -> bool {
    item.from < to && from < item.to
}

pub fn begin_suggestion(state: &SuggestionsState, draft: SuggestionDraft) -> SuggestionsState {
    let mut items: Vec<Suggestion> = state
        .items
        .iter()
        .filter(|item| !overlaps(item, draft.from, draft.to))
        .cloned()
        .collect();
    let replaced = items.len() != state.items.len();
    items.push(Suggestion::Pending(PendingSuggestion::new(
        SuggestionBase {
            id: draft.id,
            from: draft.from,
            to: draft.to,
            source: draft.source,
            original: draft.original,
            instruction: draft.instruction,
            sequence: state.next_sequence,
            origin: draft.origin.unwrap_or(SuggestionOrigin::Ask),
        },
    )));
    let next = SuggestionsState {
        items,
        next_sequence: state.next_sequence + 1,
        notice: state.notice,
    };
    if replaced {
        with_notice(next, NoticeKind::Replaced)
    } else {
        next
    }
}

pub fn resolve_suggestion(
    state: &SuggestionsState,
    id: &str,
    result: &SuggestionResult,
) -> SuggestionsState {
    let Some(Suggestion::Pending(pending)) = find_suggestion(state, id) else {
        return state.clone();
    };
    if result.unchanged {
        return with_notice(without(state, id), NoticeKind::NoChange);
    }
    let answer = match_edge_whitespace(&pending.original, &result.replacement);
    let (base, answer, notice) = match rebased_answer(pending, answer) {
        Ok(rebased) => rebased,
        Err(kind) => return with_notice(without(state, id), kind),
    };
    let replacement = match_edge_whitespace(&base.original, &answer);
    let ready = ReadySuggestion {
        diff: adaptive_diff(&base.original, &replacement),
        history: vec![RefineStep {
            instruction: base.instruction.clone(),
            replacement: replacement.clone(),
            rationale: result.rationale.clone(),
        }],
        base,
        replacement,
        rationale: result.rationale.clone(),
        shown: 0,
        refine: RefineState::Idle,
    };
    let next = replace_item(state, Suggestion::Ready(ready));
    match notice {
        Some(kind) => with_notice(next, kind),
        None => next,
    }
}

type RebasedAnswer = (SuggestionBase, String, Option<NoticeKind>);

/// The answer over the text as it is now, when the writer edited it while the model worked.
fn rebased_answer(
    pending: &PendingSuggestion,
    answer: String,
) -> Result<RebasedAnswer, NoticeKind> {
    let current = pending.current();
    if current == pending.original {
        return Ok((pending.base.clone(), answer, None));
    }
    let rebased = rebase_replacement(&pending.original, &pending.edits, &answer);
    match (rebased.kept, rebased.dropped) {
        (0, 0) => Err(NoticeKind::NoChange),
        (0, _) => Err(NoticeKind::TextChanged),
        (_, dropped) => Ok((
            SuggestionBase {
                original: rebased.original,
                ..pending.base.clone()
            },
            rebased.replacement,
            (dropped > 0).then_some(NoticeKind::PartlyKept),
        )),
    }
}

fn showing_step(
    suggestion: &ReadySuggestion,
    history: Vec<RefineStep>,
    shown: usize,
) -> ReadySuggestion {
    match history.get(shown) {
        None => suggestion.clone(),
        Some(step) => ReadySuggestion {
            diff: adaptive_diff(&suggestion.original, &step.replacement),
            replacement: step.replacement.clone(),
            rationale: step.rationale.clone(),
            history,
            shown,
            ..suggestion.clone()
        },
    }
}

fn find_ready<'a>(state: &'a SuggestionsState, id: &str) -> Option<&'a ReadySuggestion> {
    find_suggestion(state, id).and_then(Suggestion::as_ready)
}

fn with_refine(found: &ReadySuggestion, refine: RefineState) -> Suggestion {
    Suggestion::Ready(ReadySuggestion {
        refine,
        ..found.clone()
    })
}

pub fn begin_refine(
    state: &SuggestionsState,
    id: &str,
    instruction: Instruction,
) -> SuggestionsState {
    match find_ready(state, id) {
        Some(found) if !matches!(found.refine, RefineState::Working { .. }) => replace_item(
            state,
            with_refine(found, RefineState::Working { instruction }),
        ),
        _ => state.clone(),
    }
}

pub fn resolve_refine(
    state: &SuggestionsState,
    id: &str,
    result: &SuggestionResult,
) -> SuggestionsState {
    let Some(found) = find_ready(state, id) else {
        return state.clone();
    };
    let RefineState::Working { instruction } = &found.refine else {
        return state.clone();
    };
    let replacement = match_edge_whitespace(&found.original, &result.replacement);
    if result.unchanged || replacement == found.replacement {
        return with_notice(
            replace_item(state, with_refine(found, RefineState::Idle)),
            NoticeKind::NoChange,
        );
    }
    let mut history: Vec<RefineStep> = found
        .history
        .iter()
        .take(found.shown + 1)
        .cloned()
        .collect();
    history.push(RefineStep {
        instruction: instruction.clone(),
        replacement,
        rationale: result.rationale.clone(),
    });
    let last = history.len() - 1;
    let shown = ReadySuggestion {
        refine: RefineState::Idle,
        ..showing_step(found, history, last)
    };
    replace_item(state, Suggestion::Ready(shown))
}

pub fn show_version(state: &SuggestionsState, id: &str, index: i64) -> SuggestionsState {
    let Some(found) = find_ready(state, id) else {
        return state.clone();
    };
    let Ok(index) = usize::try_from(index) else {
        return state.clone();
    };
    if matches!(found.refine, RefineState::Working { .. })
        || index == found.shown
        || found.history.get(index).is_none()
    {
        return state.clone();
    }
    let shown = ReadySuggestion {
        refine: RefineState::Idle,
        ..showing_step(found, found.history.clone(), index)
    };
    replace_item(state, Suggestion::Ready(shown))
}

pub fn fail_refine(state: &SuggestionsState, id: &str, error: ClientError) -> SuggestionsState {
    match find_ready(state, id) {
        Some(found) => match &found.refine {
            RefineState::Working { instruction } => replace_item(
                state,
                with_refine(
                    found,
                    RefineState::Failed {
                        instruction: instruction.clone(),
                        error,
                    },
                ),
            ),
            _ => state.clone(),
        },
        None => state.clone(),
    }
}

pub fn retry_refine(state: &SuggestionsState, id: &str) -> SuggestionsState {
    match find_ready(state, id) {
        Some(found) => match &found.refine {
            RefineState::Failed { instruction, .. } => replace_item(
                state,
                with_refine(
                    found,
                    RefineState::Working {
                        instruction: instruction.clone(),
                    },
                ),
            ),
            _ => state.clone(),
        },
        None => state.clone(),
    }
}

pub fn cancel_refine(state: &SuggestionsState, id: &str) -> SuggestionsState {
    match find_ready(state, id) {
        Some(found) if found.refine != RefineState::Idle => {
            replace_item(state, with_refine(found, RefineState::Idle))
        }
        _ => state.clone(),
    }
}

pub fn instruction_history(suggestion: &ReadySuggestion) -> Vec<String> {
    suggestion
        .history
        .iter()
        .take(suggestion.shown + 1)
        .map(|step| step.instruction.text().to_string())
        .collect()
}

pub fn fail_suggestion(state: &SuggestionsState, id: &str, error: ClientError) -> SuggestionsState {
    match find_suggestion(state, id) {
        Some(Suggestion::Pending(pending)) => replace_item(
            state,
            Suggestion::Failed(FailedSuggestion {
                base: SuggestionBase {
                    original: pending.current(),
                    ..pending.base.clone()
                },
                error,
            }),
        ),
        _ => state.clone(),
    }
}

pub fn retry_suggestion(state: &SuggestionsState, id: &str) -> SuggestionsState {
    match find_suggestion(state, id) {
        Some(Suggestion::Failed(failed)) => replace_item(
            state,
            Suggestion::Pending(PendingSuggestion::new(failed.base.clone())),
        ),
        _ => state.clone(),
    }
}

pub fn reject_suggestion(state: &SuggestionsState, id: &str) -> SuggestionsState {
    if find_suggestion(state, id).is_none() {
        state.clone()
    } else {
        without(state, id)
    }
}

pub fn restore_suggestion(
    state: &SuggestionsState,
    suggestion: &ReadySuggestion,
    text_at_range: &str,
) -> SuggestionsState {
    if find_suggestion(state, &suggestion.id).is_some()
        || text_at_range != suggestion.original
        || state
            .items
            .iter()
            .any(|item| overlaps(item, suggestion.from, suggestion.to))
    {
        return state.clone();
    }
    let mut items = state.items.clone();
    items.push(Suggestion::Ready(suggestion.clone()));
    SuggestionsState {
        items,
        ..state.clone()
    }
}

pub fn reject_suggestions(state: &SuggestionsState, ids: &[SuggestionId]) -> SuggestionsState {
    let items: Vec<Suggestion> = state
        .items
        .iter()
        .filter(|item| !ids.contains(&item.id))
        .cloned()
        .collect();
    if items.len() == state.items.len() {
        state.clone()
    } else {
        SuggestionsState {
            items,
            ..state.clone()
        }
    }
}

pub fn reject_review_suggestions(state: &SuggestionsState) -> SuggestionsState {
    let items: Vec<Suggestion> = state
        .items
        .iter()
        .filter(|item| item.origin != SuggestionOrigin::Review)
        .cloned()
        .collect();
    if items.len() == state.items.len() {
        state.clone()
    } else {
        SuggestionsState {
            items,
            ..state.clone()
        }
    }
}

fn touches_interior(item: &Suggestion, changes: &ChangeDesc) -> bool {
    changes
        .changed_ranges()
        .iter()
        .any(|range| range.from_a < item.to && range.to_a > item.from)
}

pub fn map_suggestions(
    state: &SuggestionsState,
    changes: &ChangeDesc,
    frontmatter: Option<&Frontmatter>,
) -> SuggestionsState {
    if changes.is_empty() || state.items.is_empty() {
        return state.clone();
    }
    let ends: Vec<usize> = state
        .items
        .iter()
        .filter(|item| item.is_pending())
        .map(|item| item.to)
        .collect();
    let survivors: Vec<Suggestion> = state
        .items
        .iter()
        .filter_map(|item| match item {
            Suggestion::Pending(pending) => follow_edits(pending, changes, &ends),
            _ if touches_interior(item, changes) => None,
            _ => Some(remap(item, changes)),
        })
        .filter(|item| {
            !touches_frontmatter(
                frontmatter,
                Range {
                    from: item.from,
                    to: item.to,
                },
            )
        })
        .collect();
    let dropped = survivors.len() != state.items.len();
    let next = SuggestionsState {
        items: survivors,
        ..state.clone()
    };
    if dropped {
        with_notice(next, NoticeKind::TextChanged)
    } else {
        next
    }
}

/// A request keeps running while the writer edits its text, typing at its edges included; only
/// deleting all of it ends it.
/// `ends` are where the pending requests end: text typed where one ends and another starts
/// joins the one that ends there.
fn follow_edits(
    pending: &PendingSuggestion,
    changes: &ChangeDesc,
    ends: &[usize],
) -> Option<Suggestion> {
    let joins_start = inserts_at(changes, pending.from) && !ends.contains(&pending.from);
    let edits = pending.edits.compose(&edits_inside(
        pending.from,
        pending.to,
        changes,
        joins_start,
    ));
    let from = changes.map_pos(
        pending.from,
        if joins_start {
            Assoc::Before
        } else {
            Assoc::After
        },
    );
    let to_assoc = if inserts_at(changes, pending.to) {
        Assoc::After
    } else {
        Assoc::Before
    };
    let to = changes.map_pos(pending.to, to_assoc).max(from);
    (to > from || pending.from == pending.to).then(|| {
        Suggestion::Pending(PendingSuggestion {
            base: SuggestionBase {
                from,
                to,
                ..pending.base.clone()
            },
            edits,
        })
    })
}

fn remap(item: &Suggestion, changes: &ChangeDesc) -> Suggestion {
    let from = changes.map_pos(item.from, Assoc::After);
    let to = changes.map_pos(item.to, Assoc::Before);
    let moved = |base: &SuggestionBase| SuggestionBase {
        from,
        to,
        ..base.clone()
    };
    match item {
        Suggestion::Pending(pending) => Suggestion::Pending(PendingSuggestion {
            base: moved(&pending.base),
            edits: pending.edits.clone(),
        }),
        Suggestion::Ready(ready) => Suggestion::Ready(ReadySuggestion {
            base: moved(&ready.base),
            ..ready.clone()
        }),
        Suggestion::Failed(failed) => Suggestion::Failed(FailedSuggestion {
            base: moved(&failed.base),
            error: failed.error.clone(),
        }),
    }
}

pub fn accept_suggestions(
    state: &SuggestionsState,
    ids: &[SuggestionId],
    changes: &ChangeDesc,
    frontmatter: Option<&Frontmatter>,
) -> SuggestionsState {
    map_suggestions(&reject_suggestions(state, ids), changes, frontmatter)
}

pub fn newly_failed<'a>(
    before: &SuggestionsState,
    after: &'a SuggestionsState,
) -> Vec<&'a FailedSuggestion> {
    after
        .items
        .iter()
        .filter_map(|item| match item {
            Suggestion::Failed(failed)
                if !find_suggestion(before, &failed.id).is_some_and(Suggestion::is_failed) =>
            {
                Some(failed)
            }
            _ => None,
        })
        .collect()
}

fn contains(item: &Suggestion, position: usize) -> bool {
    item.from <= position && position <= item.to
}

fn most_recent<'a, T>(items: impl Iterator<Item = &'a T>) -> Option<&'a T>
where
    T: std::ops::Deref<Target = SuggestionBase> + 'a,
{
    items.fold(None, |best: Option<&T>, item| match best {
        Some(current) if item.sequence <= current.sequence => Some(current),
        _ => Some(item),
    })
}

fn oldest<'a, T>(items: impl Iterator<Item = &'a T>) -> Option<&'a T>
where
    T: std::ops::Deref<Target = SuggestionBase> + 'a,
{
    items.fold(None, |best: Option<&T>, item| match best {
        Some(current) if item.sequence >= current.sequence => Some(current),
        _ => Some(item),
    })
}

pub fn active_suggestion(
    state: &SuggestionsState,
    cursor: Option<usize>,
) -> Option<&ReadySuggestion> {
    let ready: Vec<&ReadySuggestion> = state
        .items
        .iter()
        .filter_map(Suggestion::as_ready)
        .collect();
    let containing = cursor.map_or_else(Vec::new, |position| {
        ready
            .iter()
            .copied()
            .filter(|item| item.from <= position && position <= item.to)
            .collect()
    });
    most_recent(containing.iter().copied())
        .or_else(|| {
            most_recent(
                ready
                    .iter()
                    .copied()
                    .filter(|item| item.origin != SuggestionOrigin::Review),
            )
        })
        .or_else(|| oldest(ready.iter().copied()))
}

pub fn suggestion_at(state: &SuggestionsState, position: usize) -> Option<&Suggestion> {
    most_recent(state.items.iter().filter(|item| contains(item, position)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::find_frontmatter;
    use crate::change_set::{ChangeSet, TextChange};
    use crate::diff::DiffMode;
    use crate::suggestions::types::TargetSource;

    fn discard_suggestion(
        state: &SuggestionsState,
        id: &str,
        kind: NoticeKind,
    ) -> SuggestionsState {
        if find_suggestion(state, id).is_none() {
            state.clone()
        } else {
            with_notice(without(state, id), kind)
        }
    }

    fn accept_suggestion(
        state: &SuggestionsState,
        id: &str,
        changes: &ChangeDesc,
    ) -> SuggestionsState {
        accept_suggestions(state, &[id.to_string()], changes, None)
    }

    fn is_in_flight(item: &Suggestion) -> bool {
        match item {
            Suggestion::Pending(_) => true,
            Suggestion::Ready(ready) => matches!(ready.refine, RefineState::Working { .. }),
            Suggestion::Failed(_) => false,
        }
    }

    fn in_flight_suggestion_ids(state: &SuggestionsState) -> Vec<SuggestionId> {
        state
            .items
            .iter()
            .filter(|item| is_in_flight(item))
            .map(|item| item.id.clone())
            .collect()
    }

    fn shorter() -> Instruction {
        Instruction::QuickAction {
            id: "shorter".into(),
            label: "Shorter".into(),
            text: "Make it shorter.".into(),
        }
    }

    fn free(text: &str) -> Instruction {
        Instruction::FreeText { text: text.into() }
    }

    fn draft(id: &str, from: usize, to: usize) -> SuggestionDraft {
        SuggestionDraft {
            id: id.into(),
            from,
            to,
            source: TargetSource::Selection,
            original: "x".repeat(to - from),
            instruction: shorter(),
            origin: None,
        }
    }

    fn review_draft(id: &str, from: usize, to: usize) -> SuggestionDraft {
        SuggestionDraft {
            origin: Some(SuggestionOrigin::Review),
            ..draft(id, from, to)
        }
    }

    fn result_with(replacement: &str, rationale: &str) -> SuggestionResult {
        SuggestionResult {
            replacement: replacement.into(),
            rationale: rationale.into(),
            unchanged: false,
        }
    }

    fn result(replacement: &str) -> SuggestionResult {
        result_with(replacement, "Because.")
    }

    fn begun(drafts: Vec<SuggestionDraft>) -> SuggestionsState {
        drafts
            .into_iter()
            .fold(empty_suggestions(), |state, draft| {
                begin_suggestion(&state, draft)
            })
    }

    fn ready(state: &SuggestionsState, id: &str, replacement: &str) -> SuggestionsState {
        resolve_suggestion(state, id, &result(replacement))
    }

    fn ready_default(state: &SuggestionsState, id: &str) -> SuggestionsState {
        ready(state, id, "changed")
    }

    fn ids_of(state: &SuggestionsState) -> Vec<&str> {
        state.items.iter().map(|item| item.id.as_str()).collect()
    }

    fn must_find<'a>(state: &'a SuggestionsState, id: &str) -> &'a Suggestion {
        find_suggestion(state, id).unwrap_or_else(|| panic!("missing {id}"))
    }

    fn ready_of<'a>(state: &'a SuggestionsState, id: &str) -> &'a ReadySuggestion {
        must_find(state, id)
            .as_ready()
            .unwrap_or_else(|| panic!("{id} is not ready"))
    }

    fn spans(state: &SuggestionsState) -> Vec<(usize, usize)> {
        state
            .items
            .iter()
            .map(|item| (item.from, item.to))
            .collect()
    }

    fn network() -> ClientError {
        ClientError::Network {
            message: "offline".into(),
        }
    }

    fn change(from: usize, to: usize, insert: &str) -> ChangeSet {
        ChangeSet::of(&[TextChange::new(from, to, insert)], 100)
    }

    mod begin_suggestion_tests {
        use super::*;

        #[test]
        fn creates_a_pending_suggestion_with_the_request_data() {
            let state = begun(vec![SuggestionDraft {
                source: TargetSource::Block,
                original: "ten chars!".into(),
                ..draft("a", 5, 15)
            }]);
            assert_eq!(
                state.items,
                vec![Suggestion::Pending(PendingSuggestion::new(
                    SuggestionBase {
                        id: "a".into(),
                        from: 5,
                        to: 15,
                        source: TargetSource::Block,
                        original: "ten chars!".into(),
                        instruction: shorter(),
                        sequence: 0,
                        origin: SuggestionOrigin::Ask,
                    }
                ))]
            );
            assert_eq!(state.notice, None);
        }

        #[test]
        fn keeps_the_origin_the_request_came_with() {
            let state = begun(vec![draft("a", 0, 10), review_draft("b", 20, 30)]);
            let origins: Vec<SuggestionOrigin> =
                state.items.iter().map(|item| item.origin).collect();
            assert_eq!(origins, [SuggestionOrigin::Ask, SuggestionOrigin::Review]);
        }

        #[test]
        fn keeps_many_suggestions_open_at_once_in_creation_order() {
            let state = begun(vec![
                draft("a", 0, 10),
                draft("b", 20, 30),
                draft("c", 40, 50),
            ]);
            assert_eq!(ids_of(&state), ["a", "b", "c"]);
            let sequences: Vec<usize> = state.items.iter().map(|item| item.sequence).collect();
            assert_eq!(sequences, [0, 1, 2]);
        }

        #[test]
        fn does_not_treat_touching_ranges_as_an_overlap() {
            let state = begun(vec![draft("a", 0, 10), draft("b", 10, 20)]);
            assert_eq!(ids_of(&state), ["a", "b"]);
            assert_eq!(state.notice, None);
        }

        #[test]
        fn replaces_an_open_suggestion_that_overlaps_the_new_request() {
            let state = begun(vec![draft("a", 0, 20)]);
            let next = begin_suggestion(&ready_default(&state, "a"), draft("b", 10, 30));
            assert_eq!(ids_of(&next), ["b"]);
            assert_eq!(
                next.notice.map(|notice| notice.kind),
                Some(NoticeKind::Replaced)
            );
        }

        #[test]
        fn replaces_a_pending_suggestion_that_overlaps_the_new_request() {
            let next = begin_suggestion(&begun(vec![draft("a", 0, 20)]), draft("b", 5, 8));
            assert_eq!(ids_of(&next), ["b"]);
            assert_eq!(
                next.notice.map(|notice| notice.kind),
                Some(NoticeKind::Replaced)
            );
        }

        #[test]
        fn replaces_every_overlapped_suggestion_and_keeps_the_others() {
            let state = begun(vec![
                draft("a", 0, 10),
                draft("b", 20, 30),
                draft("c", 40, 50),
            ]);
            let next = begin_suggestion(&state, draft("d", 5, 45));
            assert_eq!(ids_of(&next), ["d"]);

            let kept = begin_suggestion(&state, draft("e", 8, 22));
            assert_eq!(ids_of(&kept), ["c", "e"]);
        }

        #[test]
        fn gives_each_new_notice_a_new_serial() {
            let first = begin_suggestion(&begun(vec![draft("a", 0, 10)]), draft("b", 0, 10));
            let second = begin_suggestion(&first, draft("c", 0, 10));
            assert!(second.notice.unwrap().serial > first.notice.unwrap().serial);
        }
    }

    mod resolve_suggestion_tests {
        use super::*;

        #[test]
        fn turns_a_pending_suggestion_into_a_ready_one_with_a_diff() {
            let state = resolve_suggestion(
                &begun(vec![SuggestionDraft {
                    original: "Hello brave world".into(),
                    ..draft("a", 0, 17)
                }]),
                "a",
                &result_with("Hello world", "Cut a word."),
            );
            let found = ready_of(&state, "a");
            assert_eq!(found.replacement, "Hello world");
            assert_eq!(found.rationale, "Cut a word.");
            assert_eq!(found.original, "Hello brave world");
            assert_eq!(found.instruction, shorter());
            assert_eq!(
                found.history,
                vec![RefineStep {
                    instruction: shorter(),
                    replacement: "Hello world".into(),
                    rationale: "Cut a word.".into(),
                }]
            );
            assert_eq!(found.shown, 0);
            assert_eq!(found.refine, RefineState::Idle);
            assert_eq!(found.diff.mode, DiffMode::Word);
            assert_eq!(found.sequence, 0);
        }

        #[test]
        fn keeps_the_original_edge_whitespace_of_the_target() {
            let state = resolve_suggestion(
                &begun(vec![SuggestionDraft {
                    original: "  Hello brave\n".into(),
                    ..draft("a", 0, 13)
                }]),
                "a",
                &result("Hello world"),
            );
            assert_eq!(ready_of(&state, "a").replacement, "  Hello world\n");
        }

        #[test]
        fn drops_the_suggestion_with_a_no_change_notice_when_the_replacement_is_unchanged() {
            let state = resolve_suggestion(
                &begun(vec![draft("a", 0, 10)]),
                "a",
                &SuggestionResult {
                    replacement: "xxxxxxxxxx".into(),
                    rationale: "Nothing to do.".into(),
                    unchanged: true,
                },
            );
            assert_eq!(state.items, vec![]);
            assert_eq!(
                state.notice.map(|notice| notice.kind),
                Some(NoticeKind::NoChange)
            );
        }

        #[test]
        fn ignores_a_result_for_a_suggestion_that_is_gone() {
            let state = begun(vec![draft("a", 0, 10)]);
            assert_eq!(resolve_suggestion(&state, "missing", &result("y")), state);
        }

        #[test]
        fn ignores_a_result_for_a_suggestion_that_is_not_pending() {
            let ready_state = ready_default(&begun(vec![draft("a", 0, 10)]), "a");
            assert_eq!(
                resolve_suggestion(&ready_state, "a", &result("z")),
                ready_state
            );
        }
    }

    mod fail_and_retry_suggestion {
        use super::*;

        #[test]
        fn turns_a_pending_suggestion_into_an_error_that_keeps_the_request() {
            let state = fail_suggestion(&begun(vec![draft("a", 3, 9)]), "a", network());
            let Suggestion::Failed(failed) = must_find(&state, "a") else {
                panic!("expected an error suggestion");
            };
            assert_eq!(failed.error, network());
            assert_eq!(failed.from, 3);
            assert_eq!(failed.to, 9);
            assert_eq!(failed.instruction, shorter());
            assert_eq!(failed.source, TargetSource::Selection);
        }

        #[test]
        fn ignores_a_failure_for_a_suggestion_that_is_gone_or_not_pending() {
            let state = ready_default(&begun(vec![draft("a", 0, 10)]), "a");
            assert_eq!(fail_suggestion(&state, "a", network()), state);
            assert_eq!(fail_suggestion(&state, "missing", network()), state);
        }

        #[test]
        fn retry_puts_an_error_back_to_pending_with_the_same_request() {
            let failed = fail_suggestion(&begun(vec![draft("a", 3, 9)]), "a", network());
            let retried = retry_suggestion(&failed, "a");
            let found = must_find(&retried, "a");
            assert!(found.is_pending());
            assert_eq!(found.from, 3);
            assert_eq!(found.to, 9);
            assert_eq!(found.instruction, shorter());
            assert_eq!(in_flight_suggestion_ids(&retried), ["a"]);
        }

        #[test]
        fn retry_ignores_suggestions_that_are_not_in_error() {
            let state = begun(vec![draft("a", 0, 10)]);
            assert_eq!(retry_suggestion(&state, "a"), state);
            assert_eq!(retry_suggestion(&state, "missing"), state);
        }
    }

    mod newly_failed_tests {
        use super::*;

        fn timeout() -> ClientError {
            ClientError::Timeout {
                message: "slow".into(),
            }
        }

        fn ids(items: Vec<&FailedSuggestion>) -> Vec<String> {
            items.iter().map(|item| item.id.clone()).collect()
        }

        #[test]
        fn lists_the_suggestions_that_failed_in_this_step() {
            let before = begun(vec![draft("a", 0, 5)]);
            let after = fail_suggestion(&before, "a", timeout());
            assert_eq!(ids(newly_failed(&before, &after)), ["a"]);
        }

        #[test]
        fn ignores_suggestions_that_had_already_failed() {
            let failed = fail_suggestion(&begun(vec![draft("a", 0, 5)]), "a", timeout());
            let next = begin_suggestion(&failed, draft("b", 10, 15));
            assert!(newly_failed(&failed, &next).is_empty());
        }

        #[test]
        fn lists_a_suggestion_again_when_its_retry_fails() {
            let retried = retry_suggestion(
                &fail_suggestion(&begun(vec![draft("a", 0, 5)]), "a", timeout()),
                "a",
            );
            let after = fail_suggestion(&retried, "a", timeout());
            assert_eq!(ids(newly_failed(&retried, &after)), ["a"]);
        }
    }

    mod reject_and_discard {
        use super::*;

        #[test]
        fn reject_removes_the_suggestion_without_a_notice() {
            let state =
                reject_suggestion(&ready_default(&begun(vec![draft("a", 0, 10)]), "a"), "a");
            assert_eq!(state.items, vec![]);
            assert_eq!(state.notice, None);
        }

        #[test]
        fn reject_also_drops_pending_and_error_suggestions() {
            let pending = begun(vec![draft("a", 0, 10)]);
            assert_eq!(reject_suggestion(&pending, "a").items, vec![]);
            let failed = fail_suggestion(
                &pending,
                "a",
                ClientError::Network {
                    message: String::new(),
                },
            );
            assert_eq!(reject_suggestion(&failed, "a").items, vec![]);
        }

        #[test]
        fn discard_removes_the_suggestion_with_the_given_notice() {
            let state = discard_suggestion(
                &begun(vec![draft("a", 0, 10)]),
                "a",
                NoticeKind::TextChanged,
            );
            assert_eq!(state.items, vec![]);
            assert_eq!(
                state.notice.map(|notice| notice.kind),
                Some(NoticeKind::TextChanged)
            );
        }

        #[test]
        fn leaves_the_state_alone_for_an_unknown_id() {
            let state = begun(vec![draft("a", 0, 10)]);
            assert_eq!(reject_suggestion(&state, "x"), state);
            assert_eq!(
                discard_suggestion(&state, "x", NoticeKind::TextChanged),
                state
            );
        }
    }

    mod reject_suggestions_tests {
        use super::*;

        #[test]
        fn removes_every_listed_suggestion_without_a_notice() {
            let state = begun(vec![
                draft("a", 0, 10),
                draft("b", 20, 30),
                draft("c", 40, 50),
            ]);
            let next = reject_suggestions(
                &state,
                &["a".to_string(), "c".to_string(), "missing".to_string()],
            );
            assert_eq!(ids_of(&next), ["b"]);
            assert_eq!(next.notice, None);
        }

        #[test]
        fn returns_the_same_state_when_nothing_matches() {
            let state = begun(vec![draft("a", 0, 10)]);
            assert_eq!(reject_suggestions(&state, &["x".to_string()]), state);
        }
    }

    mod reject_review_suggestions_tests {
        use super::*;

        fn mixed() -> SuggestionsState {
            begun(vec![
                draft("a", 0, 10),
                review_draft("r1", 20, 30),
                review_draft("r2", 40, 50),
                draft("b", 60, 70),
            ])
        }

        #[test]
        fn drops_only_suggestions_that_came_from_a_review_pass() {
            let next =
                reject_review_suggestions(&ready_default(&ready_default(&mixed(), "r1"), "a"));
            assert_eq!(ids_of(&next), ["a", "b"]);
            assert_eq!(next.notice, None);
        }

        #[test]
        fn drops_them_whatever_their_status() {
            let refining = begin_refine(&ready_default(&mixed(), "r1"), "r1", shorter());
            assert_eq!(ids_of(&reject_review_suggestions(&refining)), ["a", "b"]);
        }

        #[test]
        fn returns_the_same_state_without_review_suggestions() {
            let state = begun(vec![draft("a", 0, 10)]);
            assert_eq!(reject_review_suggestions(&state), state);
        }
    }

    mod map_suggestions_tests {
        use super::*;

        fn open() -> SuggestionsState {
            ready_default(
                &ready_default(&begun(vec![draft("a", 20, 40), draft("b", 60, 80)]), "a"),
                "b",
            )
        }

        fn notice_kind(state: &SuggestionsState) -> Option<NoticeKind> {
            state.notice.map(|notice| notice.kind)
        }

        #[test]
        fn shifts_suggestions_after_an_insertion_before_them() {
            let next = map_suggestions(&open(), &change(5, 5, "abc"), None);
            assert_eq!(spans(&next), [(23, 43), (63, 83)]);
            assert_eq!(next.notice, None);
        }

        #[test]
        fn shifts_suggestions_back_after_a_deletion_before_them() {
            let next = map_suggestions(&open(), &change(0, 10, ""), None);
            assert_eq!(spans(&next), [(10, 30), (50, 70)]);
        }

        #[test]
        fn keeps_a_suggestion_untouched_by_an_edit_after_it() {
            let next = map_suggestions(&open(), &change(90, 95, "zz"), None);
            assert_eq!(spans(&next), [(20, 40), (60, 80)]);
        }

        #[test]
        fn treats_typing_at_the_very_start_of_a_suggestion_as_outside() {
            let next = map_suggestions(&open(), &change(20, 20, "abc"), None);
            let found = must_find(&next, "a");
            assert_eq!((found.from, found.to), (23, 43));
        }

        #[test]
        fn treats_typing_at_the_very_end_of_a_suggestion_as_outside() {
            let next = map_suggestions(&open(), &change(40, 40, "abc"), None);
            let a = must_find(&next, "a");
            let b = must_find(&next, "b");
            assert_eq!((a.from, a.to), (20, 40));
            assert_eq!((b.from, b.to), (63, 83));
        }

        #[test]
        fn treats_an_edit_that_ends_where_a_suggestion_starts_as_outside() {
            let next = map_suggestions(&open(), &change(10, 20, "abc"), None);
            let found = must_find(&next, "a");
            assert_eq!((found.from, found.to), (13, 33));
        }

        #[test]
        fn discards_a_suggestion_when_text_is_inserted_inside_it() {
            let next = map_suggestions(&open(), &change(30, 30, "abc"), None);
            assert_eq!(ids_of(&next), ["b"]);
            assert_eq!(notice_kind(&next), Some(NoticeKind::TextChanged));
        }

        #[test]
        fn discards_a_suggestion_when_text_inside_it_is_deleted() {
            let next = map_suggestions(&open(), &change(25, 28, ""), None);
            assert_eq!(ids_of(&next), ["b"]);
            assert_eq!(notice_kind(&next), Some(NoticeKind::TextChanged));
        }

        #[test]
        fn discards_a_suggestion_when_an_edit_straddles_its_boundary() {
            assert_eq!(
                ids_of(&map_suggestions(&open(), &change(15, 25, ""), None)),
                ["b"]
            );
            assert_eq!(
                ids_of(&map_suggestions(&open(), &change(35, 45, ""), None)),
                ["b"]
            );
        }

        #[test]
        fn discards_a_suggestion_when_its_whole_range_is_replaced() {
            assert_eq!(
                ids_of(&map_suggestions(&open(), &change(20, 40, "new"), None)),
                ["b"]
            );
        }

        #[test]
        fn discards_a_suggestion_that_ends_up_inside_a_frontmatter_keeps_the_rest() {
            let frontmatter =
                find_frontmatter(&format!("---\nx: {}\n---\n", "y".repeat(37))).unwrap();
            let next = map_suggestions(&open(), &change(95, 95, "z"), Some(&frontmatter));
            assert_eq!(ids_of(&next), ["b"]);
            assert_eq!(notice_kind(&next), Some(NoticeKind::TextChanged));
        }

        #[test]
        fn discards_a_suggestion_that_overlaps_the_start_of_a_frontmatter() {
            let frontmatter =
                find_frontmatter(&format!("---\nx: {}\n---\n", "y".repeat(27))).unwrap();
            let next = map_suggestions(&open(), &change(95, 95, "z"), Some(&frontmatter));
            assert_eq!(ids_of(&next), ["b"]);
        }

        #[test]
        fn keeps_suggestions_after_a_frontmatter() {
            let frontmatter = find_frontmatter("---\na: 1\n---\n").unwrap();
            let next = map_suggestions(&open(), &change(95, 95, "z"), Some(&frontmatter));
            assert_eq!(ids_of(&next), ["a", "b"]);
            assert_eq!(next.notice, None);
        }

        fn two_paragraphs(id: &str) -> SuggestionsState {
            begun(vec![SuggestionDraft {
                original: "First one.\n\nSecond one.\n".into(),
                ..draft(id, 20, 44)
            }])
        }

        #[test]
        fn keeps_a_pending_suggestion_running_while_its_text_is_edited() {
            let next = map_suggestions(&two_paragraphs("a"), &change(26, 29, "two!"), None);
            assert_eq!(spans(&next), [(20, 45)]);
            assert_eq!(next.notice, None);
            assert_eq!(in_flight_suggestion_ids(&next), ["a"]);
            let Some(Suggestion::Pending(pending)) = find_suggestion(&next, "a") else {
                panic!("a is not pending");
            };
            assert_eq!(pending.original, "First one.\n\nSecond one.\n");
            assert_eq!(pending.current(), "First two!.\n\nSecond one.\n");
        }

        #[test]
        fn discards_a_pending_suggestion_when_all_its_text_is_deleted() {
            let next = map_suggestions(&two_paragraphs("a"), &change(15, 50, ""), None);
            assert_eq!(next.items, vec![]);
            assert_eq!(notice_kind(&next), Some(NoticeKind::TextChanged));
        }

        #[test]
        fn lays_the_answer_over_the_edits_and_leaves_out_the_edited_lines() {
            let edited = map_suggestions(&two_paragraphs("a"), &change(26, 29, "two"), None);
            let next = resolve_suggestion(
                &edited,
                "a",
                &result("First, shorter.\n\nSecond, shorter.\n"),
            );
            let found = ready_of(&next, "a");
            assert_eq!(found.original, "First two.\n\nSecond one.\n");
            assert_eq!(found.replacement, "First two.\n\nSecond, shorter.\n");
            assert_eq!((found.from, found.to), (20, 44));
            assert_eq!(notice_kind(&next), Some(NoticeKind::PartlyKept));
        }

        #[test]
        fn keeps_the_whole_answer_without_a_notice_when_the_edits_miss_its_lines() {
            let edited = map_suggestions(&two_paragraphs("a"), &change(26, 29, "two"), None);
            let next = resolve_suggestion(&edited, "a", &result("First one.\n\nSecond!\n"));
            assert_eq!(ready_of(&next, "a").replacement, "First two.\n\nSecond!\n");
            assert_eq!(next.notice, None);
        }

        #[test]
        fn discards_the_answer_when_every_change_was_on_an_edited_line() {
            let edited = map_suggestions(&two_paragraphs("a"), &change(26, 29, "two"), None);
            let next = resolve_suggestion(&edited, "a", &result("First!\n\nSecond one.\n"));
            assert_eq!(next.items, vec![]);
            assert_eq!(notice_kind(&next), Some(NoticeKind::TextChanged));
        }

        fn pending_of<'a>(state: &'a SuggestionsState, id: &str) -> &'a PendingSuggestion {
            match must_find(state, id) {
                Suggestion::Pending(pending) => pending,
                _ => panic!("{id} is not pending"),
            }
        }

        #[test]
        fn follows_several_edits_in_turn_and_across_its_edges() {
            let mut doc = format!(
                "{}First one.\n\nSecond one.\n{}",
                "a".repeat(20),
                "z".repeat(56)
            );
            let mut state = two_paragraphs("a");
            for (from, to, insert) in [
                (26, 29, "two"),
                (32, 32, " ✨ more"),
                (15, 23, "WX"),
                (40, 50, "é"),
            ] {
                let changes = ChangeSet::of(&[TextChange::new(from, to, insert)], doc.len());
                doc = changes.apply(&doc);
                state = map_suggestions(&state, &changes, None);
                let pending = pending_of(&state, "a");
                assert_eq!(doc[pending.from..pending.to], pending.current());
                assert_eq!(pending.edits.length(), pending.original.len());
            }
            assert_eq!(
                pending_of(&state, "a").original,
                "First one.\n\nSecond one.\n"
            );
        }

        #[test]
        fn counts_typing_on_at_the_end_of_its_text_as_an_edit_to_its_last_line() {
            let state = begun(vec![SuggestionDraft {
                original: "The cat sat".into(),
                ..draft("a", 10, 21)
            }]);
            let typed = map_suggestions(&state, &change(21, 21, "isfied."), None);
            assert_eq!(spans(&typed), [(10, 28)]);
            assert_eq!(pending_of(&typed, "a").current(), "The cat satisfied.");
            let next = resolve_suggestion(&typed, "a", &result("The feline rested"));
            assert_eq!(next.items, vec![]);
            assert_eq!(notice_kind(&next), Some(NoticeKind::TextChanged));
        }

        #[test]
        fn gives_text_typed_between_two_requests_to_the_one_that_ends_there() {
            let state = begun(vec![
                SuggestionDraft {
                    original: "l1\nl2\n".into(),
                    ..draft("a", 0, 6)
                },
                SuggestionDraft {
                    original: "l3\nl4\n".into(),
                    ..draft("b", 6, 12)
                },
            ]);
            let typed = map_suggestions(&state, &change(6, 6, "X"), None);
            assert_eq!(spans(&typed), [(0, 7), (7, 13)]);
            assert_eq!(pending_of(&typed, "a").current(), "l1\nl2\nX");
            assert_eq!(pending_of(&typed, "b").current(), "l3\nl4\n");
        }

        #[test]
        fn says_no_change_when_the_answer_matches_the_text_the_model_saw() {
            let edited = map_suggestions(&two_paragraphs("a"), &change(26, 29, "two"), None);
            let next = resolve_suggestion(&edited, "a", &result("First one.\n\nSecond one.\n"));
            assert_eq!(next.items, vec![]);
            assert_eq!(notice_kind(&next), Some(NoticeKind::NoChange));
        }

        #[test]
        fn a_failed_request_keeps_the_text_as_edited_for_a_retry() {
            let edited = map_suggestions(&two_paragraphs("a"), &change(26, 29, "two"), None);
            let failed = fail_suggestion(&edited, "a", network());
            assert_eq!(
                must_find(&failed, "a").original,
                "First two.\n\nSecond one.\n"
            );
            let retried = retry_suggestion(&failed, "a");
            let again = resolve_suggestion(&retried, "a", &result("First two!\n\nSecond one.\n"));
            assert_eq!(again.notice, None);
            assert_eq!(
                ready_of(&again, "a").replacement,
                "First two!\n\nSecond one.\n"
            );
        }

        #[test]
        fn discards_an_error_suggestion_when_its_text_is_edited() {
            let failed = fail_suggestion(
                &begun(vec![draft("a", 20, 40)]),
                "a",
                ClientError::Network {
                    message: String::new(),
                },
            );
            assert_eq!(
                map_suggestions(&failed, &change(30, 31, "q"), None).items,
                vec![]
            );
        }

        #[test]
        fn discards_several_suggestions_hit_by_one_change_with_a_single_notice() {
            let next = map_suggestions(&open(), &change(30, 70, "q"), None);
            assert_eq!(next.items, vec![]);
            assert_eq!(notice_kind(&next), Some(NoticeKind::TextChanged));
        }

        #[test]
        fn returns_the_same_state_for_a_change_that_touches_nothing() {
            let state = open();
            assert_eq!(map_suggestions(&state, &ChangeSet::empty(100), None), state);
        }
    }

    mod accept_suggestion_tests {
        use super::*;

        #[test]
        fn removes_the_suggestion_and_does_not_count_its_own_change_as_an_edit_inside() {
            let state = ready_default(
                &ready_default(&begun(vec![draft("a", 20, 40), draft("b", 60, 80)]), "a"),
                "b",
            );
            let own = change(20, 40, &"y".repeat(30));

            let next = accept_suggestion(&state, "a", &own);

            assert_eq!(ids_of(&next), ["b"]);
            assert_eq!(next.notice, None);
            let found = must_find(&next, "b");
            assert_eq!((found.from, found.to), (70, 90));
        }

        #[test]
        fn still_discards_another_suggestion_if_the_change_reaches_inside_it() {
            let state = ready_default(
                &ready_default(&begun(vec![draft("a", 20, 40), draft("b", 60, 80)]), "a"),
                "b",
            );
            let next = accept_suggestion(&state, "a", &change(30, 70, ""));
            assert_eq!(next.items, vec![]);
        }
    }

    mod accept_suggestions_tests {
        use super::*;

        #[test]
        fn removes_every_accepted_suggestion_and_maps_the_others_through_the_one_change() {
            let state = ready_default(
                &ready_default(
                    &ready_default(
                        &begun(vec![
                            draft("a", 0, 10),
                            draft("b", 20, 30),
                            draft("c", 40, 50),
                        ]),
                        "a",
                    ),
                    "b",
                ),
                "c",
            );
            let changes = ChangeSet::of(
                &[TextChange::new(0, 10, "x"), TextChange::new(20, 30, "yy")],
                60,
            );
            let next =
                accept_suggestions(&state, &["a".to_string(), "b".to_string()], &changes, None);
            assert_eq!(ids_of(&next), ["c"]);
            assert_eq!(spans(&next), [(40 - 9 - 8, 50 - 9 - 8)]);
            assert_eq!(next.notice, None);
        }
    }

    mod active_suggestion_tests {
        use super::*;

        fn open() -> SuggestionsState {
            ready_default(
                &ready_default(
                    &ready_default(
                        &begun(vec![
                            draft("a", 0, 10),
                            draft("b", 20, 30),
                            draft("c", 40, 50),
                        ]),
                        "a",
                    ),
                    "b",
                ),
                "c",
            )
        }

        fn active_id(state: &SuggestionsState, cursor: Option<usize>) -> Option<String> {
            active_suggestion(state, cursor).map(|found| found.id.clone())
        }

        #[expect(
            clippy::unnecessary_wraps,
            reason = "builds the Option the assertions compare against"
        )]
        fn some(id: &str) -> Option<String> {
            Some(id.to_string())
        }

        #[test]
        fn is_none_without_ready_suggestions() {
            assert_eq!(active_id(&empty_suggestions(), Some(5)), None);
            assert_eq!(active_id(&begun(vec![draft("a", 0, 10)]), Some(5)), None);
        }

        #[test]
        fn is_the_most_recently_created_suggestion_when_the_cursor_is_outside() {
            assert_eq!(active_id(&open(), Some(15)), some("c"));
            assert_eq!(active_id(&open(), None), some("c"));
        }

        #[test]
        fn is_the_suggestion_containing_the_cursor() {
            assert_eq!(active_id(&open(), Some(25)), some("b"));
            assert_eq!(active_id(&open(), Some(0)), some("a"));
            assert_eq!(active_id(&open(), Some(30)), some("b"));
        }

        #[test]
        fn prefers_the_most_recent_of_two_suggestions_touching_at_the_cursor() {
            let touching = ready_default(
                &ready_default(&begun(vec![draft("a", 0, 10), draft("b", 10, 20)]), "a"),
                "b",
            );
            assert_eq!(active_id(&touching, Some(10)), some("b"));
        }

        #[test]
        fn ignores_pending_and_error_suggestions() {
            let state = begin_suggestion(
                &ready_default(&begun(vec![draft("a", 0, 10)]), "a"),
                draft("b", 20, 30),
            );
            assert_eq!(active_id(&state, Some(25)), some("a"));
        }

        mod with_review_suggestions {
            use super::*;

            fn streamed() -> SuggestionsState {
                ready_default(
                    &ready_default(
                        &ready_default(
                            &begun(vec![
                                review_draft("r1", 0, 10),
                                review_draft("r2", 20, 30),
                                review_draft("r3", 40, 50),
                            ]),
                            "r1",
                        ),
                        "r2",
                    ),
                    "r3",
                )
            }

            #[test]
            fn falls_back_to_the_first_most_important_one_not_the_newest_arrival() {
                assert_eq!(active_id(&streamed(), None), some("r1"));
                assert_eq!(active_id(&streamed(), Some(15)), some("r1"));
            }

            #[test]
            fn does_not_jump_while_more_arrive() {
                let early = ready_default(&begun(vec![review_draft("r1", 0, 10)]), "r1");
                let later =
                    ready_default(&begin_suggestion(&early, review_draft("r2", 20, 30)), "r2");
                assert_eq!(active_id(&early, None), some("r1"));
                assert_eq!(active_id(&later, None), some("r1"));
            }

            #[test]
            fn still_follows_the_cursor_into_a_review_suggestion() {
                assert_eq!(active_id(&streamed(), Some(45)), some("r3"));
            }

            #[test]
            fn prefers_the_newest_suggestion_the_user_asked_for() {
                let state = ready_default(&begin_suggestion(&streamed(), draft("a", 60, 70)), "a");
                assert_eq!(active_id(&state, None), some("a"));
                assert_eq!(active_id(&state, Some(25)), some("r2"));
            }

            #[test]
            fn uses_the_newest_asked_one_when_it_was_created_before_the_pass() {
                let asked = ready_default(&begun(vec![draft("a", 60, 70)]), "a");
                let state =
                    ready_default(&begin_suggestion(&asked, review_draft("r1", 0, 10)), "r1");
                assert_eq!(active_id(&state, None), some("a"));
            }
        }
    }

    mod suggestion_at_tests {
        use super::*;

        #[test]
        fn finds_the_suggestion_of_any_status_that_contains_a_position() {
            let state = begun(vec![draft("a", 0, 10), draft("b", 20, 30)]);
            assert_eq!(
                suggestion_at(&state, 5).map(|found| found.id.as_str()),
                Some("a")
            );
            assert_eq!(
                suggestion_at(&state, 20).map(|found| found.id.as_str()),
                Some("b")
            );
            assert!(suggestion_at(&state, 15).is_none());
        }
    }

    mod restore_suggestion_tests {
        use super::*;

        fn rejected() -> (SuggestionsState, ReadySuggestion) {
            let state = ready_default(&begun(vec![draft("a", 0, 10)]), "a");
            let suggestion = ready_of(&state, "a").clone();
            (reject_suggestion(&state, "a"), suggestion)
        }

        #[test]
        fn puts_a_rejected_suggestion_back_when_the_text_is_unchanged() {
            let (state, suggestion) = rejected();
            let restored = restore_suggestion(&state, &suggestion, &"x".repeat(10));
            assert_eq!(ids_of(&restored), ["a"]);
        }

        #[test]
        fn ignores_a_restore_when_the_text_changed_or_the_range_is_taken() {
            let (state, suggestion) = rejected();
            assert_eq!(restore_suggestion(&state, &suggestion, "other"), state);
            let taken = begin_suggestion(&state, draft("b", 5, 8));
            assert_eq!(
                restore_suggestion(&taken, &suggestion, &"x".repeat(10)),
                taken
            );
        }
    }

    mod refine {
        use super::*;

        const ORIGINAL: &str = "Hello brave new world";

        fn error() -> ClientError {
            network()
        }

        fn opened() -> SuggestionsState {
            ready(
                &begun(vec![SuggestionDraft {
                    original: ORIGINAL.into(),
                    source: TargetSource::Block,
                    ..draft("a", 10, 31)
                }]),
                "a",
                "Hello new world",
            )
        }

        fn formal() -> Instruction {
            free("Now more formal.")
        }

        fn shortest() -> Instruction {
            free("Make it tiny.")
        }

        fn working() -> SuggestionsState {
            begin_refine(&opened(), "a", formal())
        }

        fn working_refine(instruction: Instruction) -> RefineState {
            RefineState::Working { instruction }
        }

        mod begin_refine_tests {
            use super::*;

            #[test]
            fn marks_a_ready_suggestion_as_working_and_keeps_what_it_shows() {
                let opened = opened();
                let before = ready_of(&opened, "a");
                let started = begin_refine(&opened, "a", formal());
                let after = ready_of(&started, "a");
                assert_eq!(after.refine, working_refine(formal()));
                assert_eq!(
                    *after,
                    ReadySuggestion {
                        refine: after.refine.clone(),
                        ..before.clone()
                    }
                );
                assert_eq!(in_flight_suggestion_ids(&started), ["a"]);
            }

            #[test]
            fn can_start_again_after_a_failure_with_a_new_instruction() {
                let failed = fail_refine(&working(), "a", error());
                let again = begin_refine(&failed, "a", shortest());
                assert_eq!(ready_of(&again, "a").refine, working_refine(shortest()));
            }

            #[test]
            fn ignores_a_suggestion_that_is_already_refining() {
                let state = working();
                assert_eq!(begin_refine(&state, "a", shortest()), state);
            }

            #[test]
            fn ignores_unknown_pending_and_error_suggestions() {
                let pending = begun(vec![draft("a", 0, 10)]);
                assert_eq!(begin_refine(&pending, "a", formal()), pending);
                assert_eq!(begin_refine(&pending, "x", formal()), pending);
                let failed = fail_suggestion(&pending, "a", error());
                assert_eq!(begin_refine(&failed, "a", formal()), failed);
            }
        }

        mod resolve_refine_tests {
            use super::*;

            #[test]
            fn replaces_the_replacement_and_the_rationale_in_place() {
                let before_state = working();
                let before = ready_of(&before_state, "a");
                let state = resolve_refine(
                    &before_state,
                    "a",
                    &result_with("Greetings, world", "More formal."),
                );
                let after = ready_of(&state, "a");
                assert_eq!(after.id, "a");
                assert_eq!(after.from, before.from);
                assert_eq!(after.to, before.to);
                assert_eq!(after.sequence, before.sequence);
                assert_eq!(after.original, ORIGINAL);
                assert_eq!(after.instruction, shorter());
                assert_eq!(after.replacement, "Greetings, world");
                assert_eq!(after.rationale, "More formal.");
                assert_eq!(after.refine, RefineState::Idle);
                assert_eq!(state.items.len(), 1);
                assert_eq!(state.notice, None);
            }

            #[test]
            fn always_diffs_against_the_original_text() {
                let state = resolve_refine(&working(), "a", &result("Greetings, world"));
                let after = ready_of(&state, "a");
                assert_eq!(after.diff, adaptive_diff(ORIGINAL, "Greetings, world"));
                assert_ne!(
                    after.diff,
                    adaptive_diff("Hello new world", &after.replacement)
                );
            }

            #[test]
            fn recomputes_the_diff_mode_from_the_original_and_the_new_replacement() {
                assert_eq!(ready_of(&opened(), "a").diff.mode, DiffMode::Word);
                let state =
                    resolve_refine(&working(), "a", &result("Salutations to everyone present"));
                assert_eq!(ready_of(&state, "a").diff.mode, DiffMode::Block);
            }

            #[test]
            fn appends_a_step_to_the_history_oldest_first() {
                let first = resolve_refine(&working(), "a", &result_with("Hi world", "Short."));
                let second = resolve_refine(
                    &begin_refine(&first, "a", shortest()),
                    "a",
                    &result_with("Hi", "Tiny."),
                );
                assert_eq!(
                    ready_of(&second, "a").history,
                    vec![
                        RefineStep {
                            instruction: shorter(),
                            replacement: "Hello new world".into(),
                            rationale: "Because.".into(),
                        },
                        RefineStep {
                            instruction: formal(),
                            replacement: "Hi world".into(),
                            rationale: "Short.".into(),
                        },
                        RefineStep {
                            instruction: shortest(),
                            replacement: "Hi".into(),
                            rationale: "Tiny.".into(),
                        },
                    ]
                );
                assert_eq!(ready_of(&second, "a").original, ORIGINAL);
                assert_eq!(ready_of(&second, "a").replacement, "Hi");
            }

            #[test]
            fn keeps_the_edge_whitespace_of_the_original() {
                let resolved = resolve_suggestion(
                    &begun(vec![SuggestionDraft {
                        original: "  Hello brave\n".into(),
                        ..draft("a", 0, 14)
                    }]),
                    "a",
                    &result("Hello"),
                );
                let state = resolve_refine(
                    &begin_refine(&resolved, "a", formal()),
                    "a",
                    &result("Greetings"),
                );
                assert_eq!(ready_of(&state, "a").replacement, "  Greetings\n");
            }

            #[test]
            fn keeps_the_current_replacement_and_shows_no_change_when_the_result_equals_it() {
                let working_state = working();
                let before = ready_of(&working_state, "a").clone();
                let state = resolve_refine(&working_state, "a", &result("Hello new world"));
                assert_eq!(
                    *ready_of(&state, "a"),
                    ReadySuggestion {
                        refine: RefineState::Idle,
                        ..before
                    }
                );
                assert_eq!(
                    state.notice.map(|notice| notice.kind),
                    Some(NoticeKind::NoChange)
                );
            }

            #[test]
            fn treats_surrounding_whitespace_as_no_difference_from_the_latest_replacement() {
                let state = resolve_refine(&working(), "a", &result("  Hello new world \n"));
                assert_eq!(ready_of(&state, "a").history.len(), 1);
                assert_eq!(
                    state.notice.map(|notice| notice.kind),
                    Some(NoticeKind::NoChange)
                );
            }

            #[test]
            fn keeps_the_current_replacement_and_shows_no_change_when_the_result_is_the_original() {
                let working_state = working();
                let before = ready_of(&working_state, "a").clone();
                let state = resolve_refine(
                    &working_state,
                    "a",
                    &SuggestionResult {
                        replacement: ORIGINAL.into(),
                        rationale: "Back to the start.".into(),
                        unchanged: true,
                    },
                );
                assert_eq!(
                    *ready_of(&state, "a"),
                    ReadySuggestion {
                        refine: RefineState::Idle,
                        ..before
                    }
                );
                assert_eq!(
                    state.notice.map(|notice| notice.kind),
                    Some(NoticeKind::NoChange)
                );
            }

            #[test]
            fn ignores_a_result_for_a_suggestion_that_is_gone_or_not_refining() {
                let idle = opened();
                assert_eq!(resolve_refine(&idle, "a", &result("x")), idle);
                assert_eq!(resolve_refine(&idle, "missing", &result("x")), idle);
                let failed = fail_refine(&working(), "a", error());
                assert_eq!(resolve_refine(&failed, "a", &result("x")), failed);
            }
        }

        mod fail_and_retry_refine {
            use super::*;

            #[test]
            fn records_the_error_and_keeps_the_current_replacement_rationale_and_history() {
                let working_state = working();
                let before = ready_of(&working_state, "a").clone();
                let failed_state = fail_refine(&working_state, "a", error());
                let failed = ready_of(&failed_state, "a");
                assert_eq!(
                    failed.refine,
                    RefineState::Failed {
                        instruction: formal(),
                        error: error(),
                    }
                );
                assert_eq!(
                    *failed,
                    ReadySuggestion {
                        refine: failed.refine.clone(),
                        ..before
                    }
                );
                assert!(in_flight_suggestion_ids(&failed_state).is_empty());
            }

            #[test]
            fn ignores_a_failure_for_a_suggestion_that_is_gone_or_not_refining() {
                let idle = opened();
                assert_eq!(fail_refine(&idle, "a", error()), idle);
                assert_eq!(fail_refine(&idle, "missing", error()), idle);
            }

            #[test]
            fn retry_runs_the_same_instruction_again_and_clears_the_error() {
                let retried = retry_refine(&fail_refine(&working(), "a", error()), "a");
                assert_eq!(ready_of(&retried, "a").refine, working_refine(formal()));
            }

            #[test]
            fn retry_then_success_appends_the_step() {
                let state = resolve_refine(
                    &retry_refine(&fail_refine(&working(), "a", error()), "a"),
                    "a",
                    &result("Greetings"),
                );
                let instructions: Vec<Instruction> = ready_of(&state, "a")
                    .history
                    .iter()
                    .map(|step| step.instruction.clone())
                    .collect();
                assert_eq!(instructions, [shorter(), formal()]);
            }

            #[test]
            fn retry_ignores_suggestions_that_did_not_fail_a_refine() {
                let idle = opened();
                assert_eq!(retry_refine(&idle, "a"), idle);
                let state = working();
                assert_eq!(retry_refine(&state, "a"), state);
                assert_eq!(retry_refine(&state, "missing"), state);
            }
        }

        mod cancel_refine_tests {
            use super::*;

            #[test]
            fn puts_a_working_refine_back_to_idle_and_keeps_what_the_suggestion_shows() {
                let opened_state = opened();
                let before = ready_of(&opened_state, "a");
                let state = cancel_refine(&working(), "a");
                assert_eq!(ready_of(&state, "a"), before);
                assert_eq!(state.notice, None);
                assert!(in_flight_suggestion_ids(&state).is_empty());
            }

            #[test]
            fn dismisses_the_error_of_a_failed_refine_and_keeps_the_replacement() {
                let opened_state = opened();
                let before = ready_of(&opened_state, "a");
                let state = cancel_refine(&fail_refine(&working(), "a", error()), "a");
                assert_eq!(ready_of(&state, "a"), before);
                assert_eq!(state.notice, None);
            }

            #[test]
            fn keeps_the_history_of_earlier_refines() {
                let refined = resolve_refine(&working(), "a", &result("Hi world"));
                let state = cancel_refine(&begin_refine(&refined, "a", shortest()), "a");
                assert_eq!(ready_of(&state, "a").history.len(), 2);
                assert_eq!(ready_of(&state, "a").replacement, "Hi world");
            }

            #[test]
            fn ignores_a_late_result_for_the_cancelled_refine_and_allows_a_new_one() {
                let cancelled = cancel_refine(&working(), "a");
                assert_eq!(resolve_refine(&cancelled, "a", &result("late")), cancelled);
                assert_eq!(fail_refine(&cancelled, "a", error()), cancelled);
                assert_eq!(
                    ready_of(&begin_refine(&cancelled, "a", shortest()), "a").refine,
                    working_refine(shortest())
                );
            }

            #[test]
            fn ignores_idle_pending_and_unknown_suggestions() {
                let idle = opened();
                assert_eq!(cancel_refine(&idle, "a"), idle);
                assert_eq!(cancel_refine(&idle, "missing"), idle);
                let pending = begun(vec![draft("a", 0, 10)]);
                assert_eq!(cancel_refine(&pending, "a"), pending);
            }
        }

        fn three_steps() -> SuggestionsState {
            resolve_refine(
                &begin_refine(
                    &resolve_refine(&working(), "a", &result_with("Hi world", "Short.")),
                    "a",
                    shortest(),
                ),
                "a",
                &result_with("Hi", "Tiny."),
            )
        }

        mod show_version_tests {
            use super::*;

            #[test]
            fn shows_the_newest_step_after_each_refine() {
                assert_eq!(ready_of(&opened(), "a").shown, 0);
                assert_eq!(ready_of(&three_steps(), "a").shown, 2);
            }

            #[test]
            fn makes_the_replacement_rationale_and_diff_follow_the_shown_step() {
                let steps = three_steps();
                let first_state = show_version(&steps, "a", 0);
                let first = ready_of(&first_state, "a");
                assert_eq!(first.shown, 0);
                assert_eq!(first.replacement, "Hello new world");
                assert_eq!(first.rationale, "Because.");
                assert_eq!(first.original, ORIGINAL);
                assert_eq!(first.refine, RefineState::Idle);
                assert_eq!(first.diff, adaptive_diff(ORIGINAL, "Hello new world"));
                assert_eq!(first.history, ready_of(&steps, "a").history);
                let second_state = show_version(&steps, "a", 1);
                let second = ready_of(&second_state, "a");
                assert_eq!(second.shown, 1);
                assert_eq!(second.replacement, "Hi world");
                assert_eq!(second.rationale, "Short.");
                assert_eq!(second.diff, adaptive_diff(ORIGINAL, "Hi world"));
            }

            #[test]
            fn steps_back_to_the_newest_step() {
                let back = show_version(&show_version(&three_steps(), "a", 0), "a", 2);
                assert_eq!(ready_of(&back, "a"), ready_of(&three_steps(), "a"));
            }

            #[test]
            fn ignores_an_index_outside_the_history() {
                let state = three_steps();
                assert_eq!(show_version(&state, "a", -1), state);
                assert_eq!(show_version(&state, "a", 3), state);
            }

            #[test]
            fn ignores_the_step_already_shown() {
                let state = three_steps();
                assert_eq!(show_version(&state, "a", 2), state);
            }

            #[test]
            fn ignores_a_suggestion_that_is_refining() {
                let state = begin_refine(&three_steps(), "a", formal());
                assert_eq!(show_version(&state, "a", 0), state);
            }

            #[test]
            fn still_steps_after_a_failed_refine_and_dismisses_its_error_so_retry_never_runs_on_another_step()
             {
                let failed =
                    fail_refine(&begin_refine(&three_steps(), "a", formal()), "a", error());
                let stepped = show_version(&failed, "a", 0);
                assert_eq!(ready_of(&stepped, "a").shown, 0);
                assert_eq!(ready_of(&stepped, "a").refine, RefineState::Idle);
                assert_eq!(ready_of(&stepped, "a").history.len(), 3);
                assert_eq!(retry_refine(&stepped, "a"), stepped);
            }

            #[test]
            fn ignores_unknown_pending_and_error_suggestions() {
                let pending = begun(vec![draft("a", 0, 10)]);
                assert_eq!(show_version(&pending, "a", 0), pending);
                let idle = three_steps();
                assert_eq!(show_version(&idle, "missing", 0), idle);
                let failed = fail_suggestion(&pending, "a", error());
                assert_eq!(show_version(&failed, "a", 0), failed);
            }

            #[test]
            fn leaves_other_suggestions_and_the_notice_alone() {
                let state =
                    ready_default(&begin_suggestion(&three_steps(), draft("b", 40, 50)), "b");
                let next = show_version(&state, "a", 0);
                assert_eq!(must_find(&next, "b"), must_find(&state, "b"));
                assert_eq!(next.notice, state.notice);
            }
        }

        mod refine_from_an_earlier_step {
            use super::*;

            fn from_first() -> SuggestionsState {
                begin_refine(&show_version(&three_steps(), "a", 0), "a", formal())
            }

            #[test]
            fn drops_the_later_steps_and_appends_the_new_one_on_success() {
                let state = resolve_refine(
                    &from_first(),
                    "a",
                    &result_with("Good day, world", "Formal."),
                );
                let after = ready_of(&state, "a");
                assert_eq!(
                    after.history,
                    vec![
                        RefineStep {
                            instruction: shorter(),
                            replacement: "Hello new world".into(),
                            rationale: "Because.".into(),
                        },
                        RefineStep {
                            instruction: formal(),
                            replacement: "Good day, world".into(),
                            rationale: "Formal.".into(),
                        },
                    ]
                );
                assert_eq!(after.shown, 1);
                assert_eq!(after.replacement, "Good day, world");
                assert_eq!(after.rationale, "Formal.");
                assert_eq!(after.refine, RefineState::Idle);
                assert_eq!(after.diff, adaptive_diff(ORIGINAL, "Good day, world"));
            }

            #[test]
            fn keeps_every_step_on_no_change_for_the_shown_replacement() {
                let state = resolve_refine(&from_first(), "a", &result("Hello new world"));
                assert_eq!(ready_of(&state, "a").history.len(), 3);
                assert_eq!(ready_of(&state, "a").shown, 0);
                assert_eq!(
                    state.notice.map(|notice| notice.kind),
                    Some(NoticeKind::NoChange)
                );
            }

            #[test]
            fn keeps_every_step_when_the_refine_fails() {
                let state = fail_refine(&from_first(), "a", error());
                let failed = ready_of(&state, "a");
                assert_eq!(failed.history.len(), 3);
                assert_eq!(failed.shown, 0);
                assert_eq!(failed.replacement, "Hello new world");
            }

            #[test]
            fn cuts_after_the_shown_step_when_a_retry_succeeds() {
                let state = resolve_refine(
                    &retry_refine(&fail_refine(&from_first(), "a", error()), "a"),
                    "a",
                    &result("Good day, world"),
                );
                let replacements: Vec<&str> = ready_of(&state, "a")
                    .history
                    .iter()
                    .map(|step| step.replacement.as_str())
                    .collect();
                assert_eq!(replacements, ["Hello new world", "Good day, world"]);
            }

            #[test]
            fn keeps_every_step_when_the_refine_is_cancelled() {
                let cancelled = cancel_refine(&from_first(), "a");
                assert_eq!(
                    ready_of(&cancelled, "a"),
                    ready_of(&show_version(&three_steps(), "a", 0), "a")
                );
            }
        }

        mod while_refining {
            use super::*;

            #[test]
            fn accept_removes_the_suggestion_and_nothing_is_in_flight() {
                let own = change(10, 31, "new");
                let state = accept_suggestion(&working(), "a", &own);
                assert_eq!(state.items, vec![]);
                assert_eq!(state.notice, None);
                assert!(in_flight_suggestion_ids(&state).is_empty());
            }

            #[test]
            fn reject_removes_the_suggestion_without_a_notice() {
                let state = reject_suggestion(&working(), "a");
                assert_eq!(state.items, vec![]);
                assert_eq!(state.notice, None);
                assert!(in_flight_suggestion_ids(&state).is_empty());
            }

            #[test]
            fn discard_removes_the_suggestion_with_a_notice() {
                let state = discard_suggestion(&working(), "a", NoticeKind::Replaced);
                assert_eq!(state.items, vec![]);
                assert_eq!(
                    state.notice.map(|notice| notice.kind),
                    Some(NoticeKind::Replaced)
                );
            }

            #[test]
            fn a_late_result_after_reject_changes_nothing() {
                let gone = reject_suggestion(&working(), "a");
                assert_eq!(resolve_refine(&gone, "a", &result("late")), gone);
                assert_eq!(fail_refine(&gone, "a", error()), gone);
            }

            #[test]
            fn a_newer_request_over_the_range_replaces_the_refining_suggestion() {
                let next = begin_suggestion(&working(), draft("b", 15, 25));
                assert_eq!(ids_of(&next), ["b"]);
                assert_eq!(
                    next.notice.map(|notice| notice.kind),
                    Some(NoticeKind::Replaced)
                );
                assert_eq!(in_flight_suggestion_ids(&next), ["b"]);
            }

            #[test]
            fn shifts_the_range_for_an_edit_outside_and_keeps_refining() {
                let next = map_suggestions(
                    &working(),
                    &ChangeSet::of(&[TextChange::insert(0, "abc")], 100),
                    None,
                );
                let found = ready_of(&next, "a");
                assert_eq!((found.from, found.to), (13, 34));
                assert_eq!(found.refine, working_refine(formal()));
                assert_eq!(next.notice, None);
            }

            #[test]
            fn an_edit_inside_discards_the_suggestion_and_stops_the_refine() {
                let next = map_suggestions(
                    &working(),
                    &ChangeSet::of(&[TextChange::insert(20, "q")], 100),
                    None,
                );
                assert_eq!(next.items, vec![]);
                assert_eq!(
                    next.notice.map(|notice| notice.kind),
                    Some(NoticeKind::TextChanged)
                );
                assert!(in_flight_suggestion_ids(&next).is_empty());
            }
        }
    }

    mod instruction_history_tests {
        use super::*;

        fn refined() -> SuggestionsState {
            resolve_refine(
                &begin_refine(
                    &ready(&begun(vec![draft("a", 0, 10)]), "a", "one"),
                    "a",
                    free("Now more formal."),
                ),
                "a",
                &result("two"),
            )
        }

        #[test]
        fn lists_the_first_instruction_and_every_refine_oldest_first() {
            let state = refined();
            assert_eq!(
                instruction_history(ready_of(&state, "a")),
                ["Make it shorter.", "Now more formal."]
            );
        }

        #[test]
        fn stops_at_the_shown_step() {
            let state = show_version(&refined(), "a", 0);
            assert_eq!(
                instruction_history(ready_of(&state, "a")),
                ["Make it shorter."]
            );
        }
    }

    mod in_flight_suggestion_ids_tests {
        use super::*;

        #[test]
        fn lists_pending_suggestions() {
            let state = begun(vec![draft("a", 0, 10), draft("b", 20, 30)]);
            assert_eq!(in_flight_suggestion_ids(&ready_default(&state, "a")), ["b"]);
        }
    }
}
