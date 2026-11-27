use super::state::suggestion_at;
use super::types::{Direction, Suggestion, SuggestionsState};

/// The Suggestions with something to review, in document order. A request still waiting for the
/// model shows in the requests tray, not on the text map or under `Alt+↑` and `Alt+↓`.
fn in_document_order(items: &[Suggestion]) -> Vec<&Suggestion> {
    let mut ordered: Vec<&Suggestion> = items
        .iter()
        .filter(|item| !matches!(item, Suggestion::Pending(_)))
        .collect();
    ordered.sort_by_key(|item| (item.from, item.to));
    ordered
}

fn anchor_of(state: &SuggestionsState, target: &Suggestion) -> usize {
    if suggestion_at(state, target.from).is_some_and(|found| found.id == target.id) {
        target.from
    } else {
        (target.from + 1).min(target.to)
    }
}

pub fn step_suggestion(
    state: &SuggestionsState,
    cursor: usize,
    direction: Direction,
) -> Option<usize> {
    let ordered = in_document_order(&state.items);
    let current = suggestion_at(state, cursor);
    let is_other = |item: &&&Suggestion| current.is_none_or(|found| item.id != found.id);
    let target = match direction {
        Direction::Forward => ordered
            .iter()
            .filter(is_other)
            .find(|item| match current {
                Some(found) => item.from >= found.to,
                None => item.from > cursor,
            })
            .or_else(|| ordered.iter().find(is_other)),
        Direction::Backward => ordered
            .iter()
            .rev()
            .filter(is_other)
            .find(|item| match current {
                Some(found) => item.to <= found.from,
                None => item.to < cursor,
            })
            .or_else(|| ordered.iter().rev().find(is_other)),
    };
    target.map(|item| anchor_of(state, item))
}

/// The first Suggestion that starts at `offset` or after it, without wrapping: where the caret
/// goes once the Suggestion it stood in is accepted or rejected.
pub fn next_from(state: &SuggestionsState, offset: usize) -> Option<usize> {
    in_document_order(&state.items)
        .into_iter()
        .find(|item| item.from >= offset)
        .map(|item| anchor_of(state, item))
}

/// The Suggestions the text map marks and its chips count, in the order the keys visit them.
pub fn on_text_map(state: &SuggestionsState) -> Vec<&Suggestion> {
    in_document_order(&state.items)
}

/// Where the caret goes to stand in a Suggestion, so that its Card opens.
pub fn suggestion_anchor(state: &SuggestionsState, id: &str) -> Option<usize> {
    let target = state.items.iter().find(|item| item.id == id)?;
    Some(anchor_of(state, target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suggestions::state::{
        begin_suggestion, empty_suggestions, fail_suggestion, resolve_suggestion,
    };
    use crate::suggestions::types::{
        ClientError, Instruction, SuggestionDraft, SuggestionResult, TargetSource,
    };

    fn draft(id: &str, from: usize, to: usize) -> SuggestionDraft {
        SuggestionDraft {
            id: id.into(),
            from,
            to,
            source: TargetSource::Block,
            original: "x".repeat(to - from),
            instruction: Instruction::FreeText { text: "go".into() },
            origin: None,
        }
    }

    fn answered(state: &SuggestionsState, id: &str) -> SuggestionsState {
        let result = SuggestionResult {
            replacement: "y".into(),
            rationale: String::new(),
            unchanged: false,
        };
        resolve_suggestion(state, id, &result)
    }

    fn with(drafts: impl IntoIterator<Item = SuggestionDraft>) -> SuggestionsState {
        drafts
            .into_iter()
            .fold(empty_suggestions(), |state, draft| {
                let id = draft.id.clone();
                answered(&begin_suggestion(&state, draft), &id)
            })
    }

    fn state() -> SuggestionsState {
        with([draft("a", 0, 10), draft("b", 20, 30), draft("c", 40, 50)])
    }

    #[test]
    fn moves_on_to_a_suggestion_that_starts_where_the_last_one_ended_and_never_wraps() {
        let after_accepting_a = with([draft("b", 10, 20), draft("c", 40, 50)]);
        assert_eq!(next_from(&after_accepting_a, 10), Some(10));
        assert_eq!(
            step_suggestion(&after_accepting_a, 10, Direction::Forward),
            Some(40),
            "stepping from the caret would skip b"
        );
        assert_eq!(next_from(&after_accepting_a, 21), Some(40));
        assert_eq!(next_from(&after_accepting_a, 51), None);
    }

    #[test]
    fn leaves_requests_still_waiting_for_the_model_off_the_map_and_the_keys() {
        let drafts = [draft("c", 40, 50), draft("b", 20, 30), draft("a", 0, 10)];
        let waiting = drafts
            .into_iter()
            .fold(empty_suggestions(), |state, draft| {
                begin_suggestion(&state, draft)
            });
        assert!(on_text_map(&waiting).is_empty());
        assert_eq!(step_suggestion(&waiting, 15, Direction::Backward), None);

        let failed = fail_suggestion(
            &waiting,
            "a",
            ClientError::Network {
                message: "down".into(),
            },
        );
        let mixed = answered(&failed, "c");
        let ids: Vec<&str> = on_text_map(&mixed)
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        assert_eq!(ids, ["a", "c"], "b still waits for the model");
        assert_eq!(step_suggestion(&mixed, 5, Direction::Forward), Some(40));
        assert_eq!(step_suggestion(&mixed, 45, Direction::Backward), Some(0));
        assert_eq!(next_from(&mixed, 11), Some(40));
    }

    #[test]
    fn anchors_a_suggestion_where_the_caret_finds_it() {
        let state = state();
        assert_eq!(suggestion_anchor(&state, "b"), Some(20));
        assert_eq!(
            suggestion_at(&state, 20).map(|item| item.id.as_str()),
            Some("b")
        );
        assert_eq!(suggestion_anchor(&state, "missing"), None);
    }

    #[test]
    fn goes_to_the_next_suggestion_in_document_order() {
        assert_eq!(step_suggestion(&state(), 5, Direction::Forward), Some(20));
        assert_eq!(step_suggestion(&state(), 25, Direction::Forward), Some(40));
    }

    #[test]
    fn goes_to_the_previous_suggestion() {
        assert_eq!(step_suggestion(&state(), 45, Direction::Backward), Some(20));
        assert_eq!(step_suggestion(&state(), 25, Direction::Backward), Some(0));
    }

    #[test]
    fn wraps_at_the_ends() {
        assert_eq!(step_suggestion(&state(), 45, Direction::Forward), Some(0));
        assert_eq!(step_suggestion(&state(), 5, Direction::Backward), Some(40));
    }

    #[test]
    fn starts_from_the_cursor_outside_any_suggestion() {
        assert_eq!(step_suggestion(&state(), 15, Direction::Forward), Some(20));
        assert_eq!(step_suggestion(&state(), 15, Direction::Backward), Some(0));
    }

    #[test]
    fn has_nowhere_to_go_without_other_suggestions() {
        assert_eq!(
            step_suggestion(&empty_suggestions(), 0, Direction::Forward),
            None
        );
        let single = with([draft("a", 0, 10)]);
        assert_eq!(step_suggestion(&single, 5, Direction::Forward), None);
    }
}
