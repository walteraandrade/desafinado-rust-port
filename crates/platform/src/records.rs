use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use desafinado_ai::records::AiCallRecord;

pub const AI_CALLS_FILE_NAME: &str = "ai-calls.jsonl";

#[derive(Debug)]
pub struct AiCallLog {
    file: PathBuf,
    lock: Mutex<()>,
}

impl AiCallLog {
    pub fn new(user_data_dir: &Path) -> Self {
        Self {
            file: user_data_dir.join(AI_CALLS_FILE_NAME),
            lock: Mutex::new(()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.file
    }

    pub fn append(&self, record: &AiCallRecord) -> io::Result<()> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(parent) = self.file.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut line = serde_json::to_string(record).map_err(io::Error::other)?;
        line.push('\n');
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file)?;
        file.write_all(line.as_bytes())
    }

    pub fn read_lines(&self) -> io::Result<Vec<AiCallRecord>> {
        match fs::read_to_string(&self.file) {
            Ok(text) => Ok(text
                .lines()
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    pub fn read_all(&self) -> io::Result<Vec<AiCallRecord>> {
        Ok(merge_enriched(self.read_lines()?))
    }
}

pub fn merge_enriched(lines: Vec<AiCallRecord>) -> Vec<AiCallRecord> {
    let mut merged: Vec<AiCallRecord> = Vec::new();
    let mut by_id: HashMap<String, usize> = HashMap::new();
    for record in lines {
        match record
            .generation_id
            .clone()
            .and_then(|id| by_id.get(&id).copied())
        {
            Some(index) => merged[index] = record,
            None => {
                if let Some(id) = &record.generation_id {
                    by_id.insert(id.clone(), merged.len());
                }
                merged.push(record);
            }
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use desafinado_ai::records::{
        CallObservation, CallOutcome, CallTiming, ShapeRecordInput, shape_ai_call_record,
        summarize_costs,
    };

    fn shaped() -> AiCallRecord {
        shape_ai_call_record(&ShapeRecordInput {
            task: "select paragraph 2 → Shorter".into(),
            attempt: 1,
            outcome: CallOutcome::Valid,
            started_at: "2026-11-14T10:00:00.000Z".into(),
            observation: CallObservation {
                requested_model: "vendor/model".into(),
                served_model: Some("vendor/model-served".into()),
                input_tokens: Some(100.0),
                output_tokens: Some(20.0),
                cost_usd: Some(0.0012),
                response_id: Some("gen-1".into()),
                ..CallObservation::default()
            },
            generation: None,
            timing: CallTiming {
                client_total_ms: 900,
                ..Default::default()
            },
        })
    }

    #[test]
    fn the_log_appends_one_json_line_per_record_and_reads_them_back() {
        let dir = tempfile::tempdir().unwrap();
        let log = AiCallLog::new(dir.path());
        assert!(log.read_all().unwrap().is_empty());
        let one = shaped();
        let two = AiCallRecord {
            generation_id: None,
            ..one.clone()
        };
        log.append(&one).unwrap();
        log.append(&two).unwrap();
        let text = fs::read_to_string(log.path()).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert_eq!(log.read_all().unwrap(), [one, two]);
    }

    #[test]
    fn a_later_line_with_the_same_generation_id_replaces_the_earlier_one() {
        let dir = tempfile::tempdir().unwrap();
        let log = AiCallLog::new(dir.path());
        let one = shaped();
        let enriched = AiCallRecord {
            served_model: Some("exact".into()),
            ..one.clone()
        };
        let unrelated = AiCallRecord {
            generation_id: None,
            ..one.clone()
        };
        log.append(&one).unwrap();
        log.append(&unrelated).unwrap();
        log.append(&enriched).unwrap();
        let all = log.read_all().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].served_model.as_deref(), Some("exact"));
        assert_eq!(log.read_lines().unwrap().len(), 3);
    }

    #[test]
    fn skips_corrupt_lines() {
        let dir = tempfile::tempdir().unwrap();
        let log = AiCallLog::new(dir.path());
        log.append(&shaped()).unwrap();
        fs::write(
            log.path(),
            format!("garbage\n{}", fs::read_to_string(log.path()).unwrap()),
        )
        .unwrap();
        assert_eq!(log.read_all().unwrap().len(), 1);
    }

    #[test]
    fn summarizes_spend_by_served_model() {
        let one = shaped();
        let unpriced = AiCallRecord {
            cost_usd: None,
            ..one.clone()
        };
        let other = AiCallRecord {
            served_model: None,
            requested_model: "x/y".into(),
            cost_usd: Some(0.5),
            ..one.clone()
        };
        let summary = summarize_costs(&[one, unpriced, other]);
        assert_eq!(summary.total.calls, 3);
        assert_eq!(summary.total.priced_calls, 2);
        assert!((summary.total.cost_usd - 0.5012).abs() < 1e-9);
        assert_eq!(summary.by_model.len(), 2);
        assert_eq!(summary.by_model["vendor/model-served"].calls, 2);
        assert_eq!(summary.total.input_tokens, 300.0);
    }
}
