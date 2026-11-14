use super::state::find_suggestion;
use super::types::{ReadySuggestion, Suggestion, SuggestionId, SuggestionsState};

fn shows_new_text(previous: Option<&Suggestion>, next: &ReadySuggestion) -> bool {
    match previous {
        Some(Suggestion::Ready(before)) => {
            before.replacement != next.replacement || before.shown != next.shown
        }
        _ => true,
    }
}

pub fn newly_revealed_ids(
    before: &SuggestionsState,
    after: &SuggestionsState,
) -> Vec<SuggestionId> {
    after
        .items
        .iter()
        .filter_map(Suggestion::as_ready)
        .filter(|item| shows_new_text(find_suggestion(before, &item.id), item))
        .map(|item| item.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suggestions::state::{
        begin_refine, begin_suggestion, empty_suggestions, reject_suggestion, resolve_refine,
        resolve_suggestion, show_version,
    };
    use crate::suggestions::types::{Instruction, SuggestionDraft, SuggestionResult, TargetSource};

    fn shorter() -> Instruction {
        Instruction::QuickAction {
            id: "shorter".into(),
            label: "Shorter".into(),
            text: "Make it shorter.".into(),
        }
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

    fn result(replacement: &str) -> SuggestionResult {
        SuggestionResult {
            replacement: replacement.into(),
            rationale: "Because.".into(),
            unchanged: false,
        }
    }

    fn pending(id: &str) -> SuggestionsState {
        begin_suggestion(&empty_suggestions(), draft(id, 0, 5))
    }

    fn ready(id: &str) -> SuggestionsState {
        resolve_suggestion(&pending(id), id, &result("short"))
    }

    #[test]
    fn reveals_a_suggestion_when_its_replacement_arrives() {
        assert_eq!(newly_revealed_ids(&pending("a"), &ready("a")), ["a"]);
    }

    #[test]
    fn reveals_nothing_when_a_ready_suggestion_is_unchanged() {
        let before = ready("a");
        let after = begin_suggestion(&before, draft("b", 10, 15));
        assert!(newly_revealed_ids(&before, &after).is_empty());
    }

    #[test]
    fn reveals_nothing_while_a_refine_is_working() {
        let before = ready("a");
        assert!(newly_revealed_ids(&before, &begin_refine(&before, "a", shorter())).is_empty());
    }

    #[test]
    fn reveals_a_suggestion_when_a_refine_lands() {
        let before = begin_refine(&ready("a"), "a", shorter());
        assert_eq!(
            newly_revealed_ids(&before, &resolve_refine(&before, "a", &result("tiny"))),
            ["a"]
        );
    }

    #[test]
    fn reveals_a_suggestion_when_another_version_is_shown() {
        let refining = begin_refine(&ready("a"), "a", shorter());
        let before = resolve_refine(&refining, "a", &result("tiny"));
        assert_eq!(
            newly_revealed_ids(&before, &show_version(&before, "a", 0)),
            ["a"]
        );
    }

    #[test]
    fn reveals_only_the_suggestion_whose_text_changed() {
        let both = resolve_suggestion(
            &begin_suggestion(&ready("a"), draft("b", 10, 15)),
            "b",
            &result("brief"),
        );
        let before = begin_refine(&both, "b", shorter());
        assert_eq!(
            newly_revealed_ids(&before, &resolve_refine(&before, "b", &result("tiny"))),
            ["b"]
        );
    }

    #[test]
    fn reveals_nothing_when_a_suggestion_goes_away() {
        let before = ready("a");
        assert!(newly_revealed_ids(&before, &reject_suggestion(&before, "a")).is_empty());
    }
}
