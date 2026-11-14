use super::state::suggestion_at;
use super::types::{Direction, Suggestion, SuggestionsState};

fn in_document_order(items: &[Suggestion]) -> Vec<&Suggestion> {
    let mut ordered: Vec<&Suggestion> = items.iter().collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suggestions::state::{begin_suggestion, empty_suggestions};
    use crate::suggestions::types::{Instruction, SuggestionDraft, TargetSource};

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

    fn state() -> SuggestionsState {
        [draft("a", 0, 10), draft("b", 20, 30), draft("c", 40, 50)]
            .into_iter()
            .fold(empty_suggestions(), |state, draft| {
                begin_suggestion(&state, draft)
            })
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
        let single = begin_suggestion(&empty_suggestions(), draft("a", 0, 10));
        assert_eq!(step_suggestion(&single, 5, Direction::Forward), None);
    }
}
