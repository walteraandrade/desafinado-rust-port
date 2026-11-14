use serde_json::Value;

use crate::array_stream::ElementScanner;
use crate::edits::{
    DroppedEdit, ReviewOutcome, ReviewValidationContext, ReviewValidationState, ValidatedEdit,
    validate_review_edit,
};
use crate::errors::{ClientError, is_retryable_output_error};
use crate::schemas::{ReviewEdit, validate_review_edit as validate_schema};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewStreamEvent {
    Edit(ValidatedEdit),
    Dropped(DroppedEdit),
    InvalidElement { raw: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewStreamTally {
    pub validated: usize,
    pub dropped: usize,
    pub invalid: usize,
}

#[derive(Debug, Clone)]
pub struct ReviewStream {
    scanner: ElementScanner,
    context: ReviewValidationContext,
    state: ReviewValidationState,
    tally: ReviewStreamTally,
}

impl ReviewStream {
    pub fn new(context: ReviewValidationContext) -> Self {
        Self {
            scanner: ElementScanner::new(),
            context,
            state: ReviewValidationState::default(),
            tally: ReviewStreamTally {
                validated: 0,
                dropped: 0,
                invalid: 0,
            },
        }
    }

    pub fn feed(&mut self, chunk: &str) -> Vec<ReviewStreamEvent> {
        self.scanner
            .feed(chunk)
            .into_iter()
            .map(|raw| self.accept(raw))
            .collect()
    }

    fn accept(&mut self, raw: String) -> ReviewStreamEvent {
        let edit = serde_json::from_str::<Value>(&raw)
            .ok()
            .and_then(|value| validate_schema(&value).ok());
        let Some(edit) = edit else {
            self.tally.invalid += 1;
            return ReviewStreamEvent::InvalidElement { raw };
        };
        let (state, outcome) = validate_review_edit(&self.context, &self.state, &edit);
        self.state = state;
        match outcome {
            ReviewOutcome::Validated(edit) => {
                self.tally.validated += 1;
                ReviewStreamEvent::Edit(edit)
            }
            ReviewOutcome::Dropped(dropped) => {
                self.tally.dropped += 1;
                ReviewStreamEvent::Dropped(dropped)
            }
        }
    }

    pub const fn tally(&self) -> &ReviewStreamTally {
        &self.tally
    }

    pub const fn is_complete(&self) -> bool {
        self.scanner.is_closed()
    }

    pub fn finish(&self) -> Result<ReviewStreamTally, ClientError> {
        if self.tally.invalid > 0 {
            return Err(ClientError::InvalidOutput {
                message: "An element of the answer did not match the schema".to_owned(),
            });
        }
        if !self.scanner.is_closed() {
            return Err(ClientError::InvalidOutput {
                message: "The answer ended before the list of edits was complete".to_owned(),
            });
        }
        Ok(self.tally.clone())
    }
}

pub fn parse_review_output(text: &str) -> Option<Vec<ReviewEdit>> {
    let value: Value = serde_json::from_str(text).ok()?;
    let elements = value.get("elements")?.as_array()?;
    elements
        .iter()
        .map(|element| validate_schema(element).ok())
        .collect()
}

pub fn should_retry_review(failure: Option<&ClientError>, validated: usize) -> bool {
    validated == 0 && failure.is_some_and(is_retryable_output_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Block;
    use crate::testing::extract_blocks;
    use std::collections::BTreeSet;

    fn blocks() -> Vec<Block> {
        extract_blocks("# Title\n\nOne.\n\nTwo.\n\n```js\ncode\n```\n")
    }

    fn stream() -> ReviewStream {
        ReviewStream::new(ReviewValidationContext {
            blocks: blocks(),
            occupied: BTreeSet::new(),
        })
    }

    fn element(block: &str, replacement: &str) -> String {
        format!(r#"{{"blockId":"{block}","replacement":"{replacement}","rationale":"Why."}}"#)
    }

    #[test]
    fn emits_each_suggestion_as_soon_as_its_element_validates() {
        let mut stream = stream();
        let first = element("p2", "One edited.");
        let second = element("p3", "Two edited.");
        assert!(
            stream
                .feed(&format!(r#"{{"elements":[{}"#, &first[..20]))
                .is_empty()
        );
        let events = stream.feed(&format!("{},{}", &first[20..], &second[..10]));
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ReviewStreamEvent::Edit(e) if e.block_id == "p2"));
        let events = stream.feed(&format!("{}]}}", &second[10..]));
        assert!(matches!(&events[0], ReviewStreamEvent::Edit(e) if e.block_id == "p3"));
        assert_eq!(stream.finish().unwrap().validated, 2);
    }

    #[test]
    fn reports_dropped_edits_with_their_reason() {
        let mut stream = stream();
        let text = format!(
            r#"{{"elements":[{},{},{}]}}"#,
            element("p9", "x"),
            element("p4", "changed"),
            element("p2", "One.")
        );
        let events = stream.feed(&text);
        let reasons: Vec<&str> = events
            .iter()
            .map(|event| match event {
                ReviewStreamEvent::Dropped(d) => d.reason.as_str(),
                _ => "other",
            })
            .collect();
        assert_eq!(reasons, ["unknown-block", "code-block", "unchanged"]);
        assert_eq!(stream.finish().unwrap().dropped, 3);
    }

    #[test]
    fn flags_an_element_that_fails_the_schema() {
        let mut stream = stream();
        let events =
            stream.feed(r#"{"elements":[{"blockId":"p2","replacement":1,"rationale":"x"}]}"#);
        assert!(matches!(
            events[0],
            ReviewStreamEvent::InvalidElement { .. }
        ));
        assert!(matches!(
            stream.finish(),
            Err(ClientError::InvalidOutput { .. })
        ));
    }

    #[test]
    fn an_unfinished_answer_is_invalid_output_but_keeps_what_was_emitted() {
        let mut stream = stream();
        let events = stream.feed(&format!(
            r#"{{"elements":[{},{{"blockId":"p3""#,
            element("p2", "One edited.")
        ));
        assert_eq!(events.len(), 1);
        assert!(!stream.is_complete());
        assert!(matches!(
            stream.finish(),
            Err(ClientError::InvalidOutput { .. })
        ));
        assert_eq!(stream.tally().validated, 1);
    }

    #[test]
    fn an_empty_list_is_a_complete_answer() {
        let mut stream = stream();
        assert!(stream.feed(r#"{"elements":[]}"#).is_empty());
        assert_eq!(stream.finish().unwrap().validated, 0);
    }

    #[test]
    fn parses_a_whole_output() {
        let text = format!(r#"{{"elements":[{}]}}"#, element("p2", "x"));
        assert_eq!(parse_review_output(&text).unwrap().len(), 1);
        assert_eq!(
            parse_review_output(r#"{"elements":[{"blockId":"p1"}]}"#),
            None
        );
        assert_eq!(parse_review_output(r#"[]"#), None);
    }

    #[test]
    fn retries_only_when_nothing_validated_and_the_output_was_invalid() {
        let invalid = ClientError::InvalidOutput {
            message: "x".into(),
        };
        let timeout = ClientError::Timeout {
            message: "x".into(),
        };
        assert!(should_retry_review(Some(&invalid), 0));
        assert!(!should_retry_review(Some(&invalid), 1));
        assert!(!should_retry_review(Some(&timeout), 0));
        assert!(!should_retry_review(None, 0));
    }
}
