use super::recorder::AcceptedInput;
use super::types::AcceptedChange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryEvent {
    Edited {
        text: String,
    },
    Accepted {
        before: String,
        text: String,
        changes: Vec<AcceptedChange>,
    },
}

pub trait EventSink {
    fn edited(&mut self, text: String, now: i64);
    fn accepted(&mut self, input: AcceptedInput, now: i64);
}

pub fn apply_history_event(sink: &mut impl EventSink, event: HistoryEvent, now: i64) {
    match event {
        HistoryEvent::Edited { text } => sink.edited(text, now),
        HistoryEvent::Accepted {
            before,
            text,
            changes,
        } => sink.accepted(
            AcceptedInput {
                before,
                text,
                changes,
            },
            now,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::types::SuggestionOrigin;

    #[derive(Default)]
    struct Recorder {
        edited: Vec<String>,
        accepted: Vec<AcceptedInput>,
    }

    impl EventSink for Recorder {
        fn edited(&mut self, text: String, _now: i64) {
            self.edited.push(text);
        }
        fn accepted(&mut self, input: AcceptedInput, _now: i64) {
            self.accepted.push(input);
        }
    }

    #[test]
    fn passes_edits_on_as_pending_text() {
        let mut recorder = Recorder::default();
        apply_history_event(
            &mut recorder,
            HistoryEvent::Edited { text: "abc".into() },
            0,
        );
        assert_eq!(recorder.edited, vec!["abc".to_string()]);
        assert!(recorder.accepted.is_empty());
    }

    #[test]
    fn passes_an_accept_on_with_its_suggestions() {
        let mut recorder = Recorder::default();
        let changes = vec![AcceptedChange {
            origin: SuggestionOrigin::Ask,
            instruction: "Clearer".into(),
            rationale: "Why.".into(),
        }];
        apply_history_event(
            &mut recorder,
            HistoryEvent::Accepted {
                before: "a".into(),
                text: "b".into(),
                changes: changes.clone(),
            },
            0,
        );
        assert_eq!(
            recorder.accepted,
            vec![AcceptedInput {
                before: "a".into(),
                text: "b".into(),
                changes
            }]
        );
    }
}
