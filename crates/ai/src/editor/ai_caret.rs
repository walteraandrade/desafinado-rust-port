use desafinado_core::suggestions::{
    RefineState, Suggestion, SuggestionId, SuggestionOrigin, SuggestionsState, instruction_label,
};

pub const REVIEW_LABEL: &str = "Reviewing";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCaret {
    pub pos: usize,
    pub label: String,
    /// The Suggestion the caret marks; `None` while a Review pass has none yet.
    pub target: Option<SuggestionId>,
}

fn latest<'a>(items: impl Iterator<Item = &'a Suggestion>) -> Option<&'a Suggestion> {
    items.fold(None, |best: Option<&Suggestion>, item| match best {
        Some(current) if item.sequence <= current.sequence => Some(current),
        _ => Some(item),
    })
}

pub fn ai_caret_of(state: &SuggestionsState, reviewing: bool) -> Option<AiCaret> {
    let refining = state.items.iter().find_map(|item| match item {
        Suggestion::Ready(ready) => match &ready.refine {
            RefineState::Working { instruction } => Some(AiCaret {
                pos: ready.to,
                label: instruction_label(instruction).to_string(),
                target: Some(ready.id.clone()),
            }),
            _ => None,
        },
        _ => None,
    });
    if refining.is_some() {
        return refining;
    }
    if !reviewing {
        return None;
    }
    let reviewed = latest(
        state
            .items
            .iter()
            .filter(|item| item.origin == SuggestionOrigin::Review),
    );
    Some(AiCaret {
        pos: reviewed.map_or(0, |item| item.to),
        label: REVIEW_LABEL.to_string(),
        target: reviewed.map(|item| item.id.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use desafinado_core::suggestions::{
        Instruction, SuggestionDraft, SuggestionResult, TargetSource, begin_refine,
        begin_suggestion, empty_suggestions, resolve_suggestion,
    };

    fn shorter() -> Instruction {
        Instruction::QuickAction {
            id: "shorter".into(),
            label: "Shorter".into(),
            text: "Make the text shorter.".into(),
        }
    }

    fn draft(
        id: &str,
        from: usize,
        to: usize,
        origin: Option<SuggestionOrigin>,
    ) -> SuggestionDraft {
        SuggestionDraft {
            id: id.into(),
            from,
            to,
            source: TargetSource::Block,
            original: "x".repeat(to - from),
            instruction: shorter(),
            origin,
        }
    }

    fn result() -> SuggestionResult {
        SuggestionResult {
            replacement: "y".into(),
            rationale: "Because.".into(),
            unchanged: false,
        }
    }

    #[test]
    fn shows_nothing_while_the_ai_is_idle() {
        assert_eq!(ai_caret_of(&empty_suggestions(), false), None);
    }

    #[test]
    fn leaves_a_pending_request_to_the_requests_tray() {
        let state = begin_suggestion(&empty_suggestions(), draft("a", 0, 4, None));
        assert_eq!(ai_caret_of(&state, false), None);
    }

    #[test]
    fn a_refine_in_flight_wins_over_a_pending_request() {
        let state = begin_suggestion(&empty_suggestions(), draft("a", 0, 4, None));
        let state = resolve_suggestion(&state, "a", &result());
        let state = begin_refine(
            &state,
            "a",
            Instruction::FreeText {
                text: "warmer".into(),
            },
        );
        let state = begin_suggestion(&state, draft("b", 10, 14, None));
        assert_eq!(
            ai_caret_of(&state, false),
            Some(AiCaret {
                pos: 4,
                label: "warmer".into(),
                target: Some("a".into()),
            })
        );
    }

    #[test]
    fn follows_the_latest_review_suggestion_while_a_review_pass_runs() {
        assert_eq!(
            ai_caret_of(&empty_suggestions(), true),
            Some(AiCaret {
                pos: 0,
                label: "Reviewing".into(),
                target: None,
            })
        );
        let state = begin_suggestion(
            &empty_suggestions(),
            draft("r1", 5, 9, Some(SuggestionOrigin::Review)),
        );
        let state = resolve_suggestion(&state, "r1", &result());
        let caret = ai_caret_of(&state, true).unwrap();
        assert_eq!((caret.pos, caret.target.as_deref()), (9, Some("r1")));
        assert_eq!(ai_caret_of(&state, false), None);
    }
}
