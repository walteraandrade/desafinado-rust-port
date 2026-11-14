use serde::{Deserialize, Serialize};

use crate::records::AiCallRecord;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDropEntry {
    pub block_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DevLog {
    pub ai_calls: Vec<AiCallRecord>,
    pub review_drops: Vec<ReviewDropEntry>,
}

impl DevLog {
    pub fn append_ai_call(&mut self, record: AiCallRecord) {
        self.ai_calls.push(record);
    }

    pub fn enrich_ai_call(&mut self, record: AiCallRecord) {
        let index = self.ai_calls.iter().position(|entry| {
            entry.generation_id.is_some() && entry.generation_id == record.generation_id
        });
        match index {
            Some(index) => self.ai_calls[index] = record,
            None => self.ai_calls.push(record),
        }
    }

    pub fn append_review_drop(&mut self, drop: ReviewDropEntry) {
        self.review_drops.push(drop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::{
        CallObservation, CallOutcome, CallTiming, ShapeRecordInput, shape_ai_call_record,
    };

    fn record() -> AiCallRecord {
        shape_ai_call_record(&ShapeRecordInput {
            task: "task".into(),
            attempt: 1,
            outcome: CallOutcome::Valid,
            started_at: "2026-10-28T12:00:00.000Z".into(),
            observation: CallObservation {
                requested_model: "vendor/model-a".into(),
                ..CallObservation::default()
            },
            generation: None,
            timing: CallTiming {
                client_total_ms: 10,
                ..CallTiming::default()
            },
        })
    }

    fn with_id(id: &str) -> AiCallRecord {
        AiCallRecord {
            generation_id: Some(id.into()),
            ..record()
        }
    }

    #[test]
    fn append_creates_the_log_on_first_use() {
        let mut log = DevLog::default();
        log.append_ai_call(record());
        assert_eq!(log.ai_calls, vec![record()]);
    }

    #[test]
    fn append_keeps_records_in_order() {
        let mut log = DevLog::default();
        log.append_ai_call(record());
        log.append_ai_call(AiCallRecord {
            task: "second".into(),
            ..record()
        });
        let tasks: Vec<&str> = log.ai_calls.iter().map(|e| e.task.as_str()).collect();
        assert_eq!(tasks, ["task", "second"]);
    }

    #[test]
    fn enrich_replaces_the_record_with_the_same_generation_id() {
        let mut log = DevLog::default();
        log.append_ai_call(with_id("gen-1"));
        let enriched = AiCallRecord {
            provider: Some("Acme".into()),
            ..with_id("gen-1")
        };
        log.enrich_ai_call(enriched.clone());
        assert_eq!(log.ai_calls, vec![enriched]);
    }

    #[test]
    fn enrich_appends_when_no_record_has_that_generation_id() {
        let mut log = DevLog::default();
        log.append_ai_call(with_id("gen-1"));
        log.enrich_ai_call(with_id("gen-2"));
        let ids: Vec<Option<&str>> = log
            .ai_calls
            .iter()
            .map(|e| e.generation_id.as_deref())
            .collect();
        assert_eq!(ids, [Some("gen-1"), Some("gen-2")]);
    }

    #[test]
    fn enrich_never_matches_records_without_a_generation_id() {
        let mut log = DevLog::default();
        log.append_ai_call(record());
        log.enrich_ai_call(record());
        assert_eq!(log.ai_calls.len(), 2);
    }

    #[test]
    fn keeps_one_entry_per_dropped_review_edit_in_order() {
        let mut log = DevLog::default();
        log.append_review_drop(ReviewDropEntry {
            block_id: "p3".into(),
            reason: "junk".into(),
        });
        log.append_review_drop(ReviewDropEntry {
            block_id: "p5".into(),
            reason: "text-changed".into(),
        });
        assert_eq!(
            log.review_drops,
            vec![
                ReviewDropEntry {
                    block_id: "p3".into(),
                    reason: "junk".into()
                },
                ReviewDropEntry {
                    block_id: "p5".into(),
                    reason: "text-changed".into()
                },
            ]
        );
    }
}
