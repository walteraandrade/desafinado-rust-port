use desafinado_core::suggestions::{
    RefineState, Suggestion, SuggestionOrigin, SuggestionsState, instruction_label,
};

pub const REVIEW_LABEL: &str = "Reviewing";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCaret {
    pub pos: usize,
    pub label: String,
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
            }),
            _ => None,
        },
        _ => None,
    });
    if refining.is_some() {
        return refining;
    }
    if let Some(pending) = latest(state.items.iter().filter(|item| item.is_pending())) {
        return Some(AiCaret {
            pos: pending.to,
            label: instruction_label(&pending.instruction).to_string(),
        });
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
    fn sits_at_the_end_of_the_latest_pending_suggestion_with_its_label() {
        let state = begin_suggestion(&empty_suggestions(), draft("a", 0, 4, None));
        let state = begin_suggestion(&state, draft("b", 10, 14, None));
        assert_eq!(
            ai_caret_of(&state, false),
            Some(AiCaret {
                pos: 14,
                label: "Shorter".into()
            })
        );
    }

    #[test]
    fn labels_free_text_with_the_text_itself() {
        let mut free = draft("a", 0, 4, None);
        free.instruction = Instruction::FreeText {
            text: "make it rhyme".into(),
        };
        let state = begin_suggestion(&empty_suggestions(), free);
        assert_eq!(ai_caret_of(&state, false).unwrap().label, "make it rhyme");
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
                label: "warmer".into()
            })
        );
    }

    #[test]
    fn follows_the_latest_review_suggestion_while_a_review_pass_runs() {
        assert_eq!(
            ai_caret_of(&empty_suggestions(), true),
            Some(AiCaret {
                pos: 0,
                label: "Reviewing".into()
            })
        );
        let state = begin_suggestion(
            &empty_suggestions(),
            draft("r1", 5, 9, Some(SuggestionOrigin::Review)),
        );
        let state = resolve_suggestion(&state, "r1", &result());
        assert_eq!(ai_caret_of(&state, true).unwrap().pos, 9);
        assert_eq!(ai_caret_of(&state, false), None);
    }
}
