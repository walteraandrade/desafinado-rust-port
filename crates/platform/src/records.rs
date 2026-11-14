use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

pub const AI_CALLS_FILE_NAME: &str = "ai-calls.jsonl";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CallObservation {
    pub requested_model: String,
    pub served_model: Option<String>,
    pub provider: Option<String>,
    pub finish_reason: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub cached_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub response_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct OpenRouterGeneration {
    pub id: Option<String>,
    pub model: Option<String>,
    pub provider_name: Option<String>,
    pub latency: Option<f64>,
    pub generation_time: Option<f64>,
    pub finish_reason: Option<String>,
    pub tokens_prompt: Option<i64>,
    pub tokens_completion: Option<i64>,
    pub native_tokens_reasoning: Option<i64>,
    pub native_tokens_cached: Option<i64>,
    pub total_cost: Option<f64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CallTiming {
    pub client_total_ms: f64,
    pub first_element_ms: Option<f64>,
    pub first_suggestion_ms: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CallOutcome {
    Valid,
    Invalid,
    Error,
}

impl CallOutcome {
    fn label(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Invalid => "invalid",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tokens {
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub reasoning: Option<i64>,
    pub cached: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatencyMs {
    pub client: f64,
    pub open_router: Option<f64>,
    pub generation: Option<f64>,
    pub first_element: Option<f64>,
    pub first_suggestion: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCallRecord {
    pub started_at: String,
    pub task: String,
    pub attempt: u32,
    pub outcome: CallOutcome,
    pub requested_model: String,
    pub served_model: Option<String>,
    pub provider: Option<String>,
    pub tokens: Tokens,
    pub cost_usd: Option<f64>,
    pub latency_ms: LatencyMs,
    pub finish_reason: Option<String>,
    pub generation_id: Option<String>,
    pub error: Option<String>,
}

pub struct ShapeRecordInput<'a> {
    pub task: &'a str,
    pub attempt: u32,
    pub outcome: CallOutcome,
    pub started_at: &'a str,
    pub observation: &'a CallObservation,
    pub generation: Option<&'a OpenRouterGeneration>,
    pub timing: CallTiming,
}

fn first<T: Clone>(values: [Option<&T>; 2]) -> Option<T> {
    values.into_iter().flatten().next().cloned()
}

pub fn shape_ai_call_record(input: &ShapeRecordInput) -> AiCallRecord {
    let o = input.observation;
    let g = input.generation;
    AiCallRecord {
        started_at: input.started_at.to_owned(),
        task: input.task.to_owned(),
        attempt: input.attempt,
        outcome: input.outcome,
        requested_model: o.requested_model.clone(),
        served_model: first([g.and_then(|g| g.model.as_ref()), o.served_model.as_ref()]),
        provider: first([
            g.and_then(|g| g.provider_name.as_ref()),
            o.provider.as_ref(),
        ]),
        tokens: Tokens {
            input: first([
                o.input_tokens.as_ref(),
                g.and_then(|g| g.tokens_prompt.as_ref()),
            ]),
            output: first([
                o.output_tokens.as_ref(),
                g.and_then(|g| g.tokens_completion.as_ref()),
            ]),
            reasoning: first([
                o.reasoning_tokens.as_ref(),
                g.and_then(|g| g.native_tokens_reasoning.as_ref()),
            ]),
            cached: first([
                o.cached_tokens.as_ref(),
                g.and_then(|g| g.native_tokens_cached.as_ref()),
            ]),
        },
        cost_usd: first([o.cost_usd.as_ref(), g.and_then(|g| g.total_cost.as_ref())]),
        latency_ms: LatencyMs {
            client: input.timing.client_total_ms,
            open_router: g.and_then(|g| g.latency),
            generation: g.and_then(|g| g.generation_time),
            first_element: input.timing.first_element_ms,
            first_suggestion: input.timing.first_suggestion_ms,
        },
        finish_reason: first([
            g.and_then(|g| g.finish_reason.as_ref()),
            o.finish_reason.as_ref(),
        ]),
        generation_id: first([o.response_id.as_ref(), g.and_then(|g| g.id.as_ref())]),
        error: o.error.clone(),
    }
}

const DASH: &str = "–";

fn cell(value: &str) -> String {
    let escaped = value.replace('|', "\\|");
    let mut out = String::new();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if run.contains('\n') {
            out.push(' ');
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for c in escaped.chars() {
        if c.is_whitespace() {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out
}

fn show<T: ToString>(value: &Option<T>) -> String {
    value
        .as_ref()
        .map_or_else(|| DASH.to_owned(), ToString::to_string)
}

fn number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}

fn show_number(value: Option<f64>) -> String {
    value.map_or_else(|| DASH.to_owned(), number)
}

fn format_latency(record: &AiCallRecord) -> String {
    let l = &record.latency_ms;
    let base = [
        number(l.client),
        show_number(l.open_router),
        show_number(l.generation),
    ]
    .join(" / ");
    match l.first_suggestion {
        None => base,
        Some(first) => format!("{base} (first {})", number(first)),
    }
}

fn format_outcome(record: &AiCallRecord) -> String {
    let retry = if record.attempt > 1 { " (retry)" } else { "" };
    let error = match record.error.as_deref() {
        Some(error) if !error.is_empty() => format!(": {error}"),
        _ => String::new(),
    };
    format!("{}{retry}{error}", record.outcome.label())
}

pub const RECORD_TABLE_COLUMNS: [&str; 9] = [
    "Task",
    "Model requested",
    "Served by",
    "Tokens in / out / reasoning / cached",
    "Cost (US$)",
    "Latency client / OpenRouter / generation (ms)",
    "Finish",
    "Valid",
    "Generation id",
];

pub fn format_record_row(record: &AiCallRecord) -> String {
    let served: Vec<&str> = [record.served_model.as_deref(), record.provider.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    let cells = [
        record.task.clone(),
        record.requested_model.clone(),
        if served.is_empty() {
            DASH.to_owned()
        } else {
            served.join(" · ")
        },
        [
            show(&record.tokens.input),
            show(&record.tokens.output),
            show(&record.tokens.reasoning),
            show(&record.tokens.cached),
        ]
        .join(" / "),
        record
            .cost_usd
            .map_or_else(|| DASH.to_owned(), |cost| format!("{cost:.6}")),
        format_latency(record),
        show(&record.finish_reason),
        format_outcome(record),
        show(&record.generation_id),
    ];
    format!(
        "| {} |",
        cells
            .iter()
            .map(|c| cell(c))
            .collect::<Vec<_>>()
            .join(" | ")
    )
}

pub fn format_records_table(records: &[AiCallRecord]) -> String {
    let mut lines = vec![
        format!("| {} |", RECORD_TABLE_COLUMNS.join(" | ")),
        format!("|{}", RECORD_TABLE_COLUMNS.map(|_| " --- |").concat()),
    ];
    lines.extend(records.iter().map(format_record_row));
    lines.join("\n")
}

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

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSpend {
    pub model: String,
    pub calls: usize,
    pub cost_usd: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendSummary {
    pub calls: usize,
    pub cost_usd: f64,
    pub unpriced_calls: usize,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub by_model: Vec<ModelSpend>,
}

pub fn summarize_spend(records: &[AiCallRecord]) -> SpendSummary {
    let mut summary = SpendSummary::default();
    let mut models: Vec<ModelSpend> = Vec::new();
    for record in records {
        let model = record
            .served_model
            .clone()
            .unwrap_or_else(|| record.requested_model.clone());
        let index = models
            .iter()
            .position(|m| m.model == model)
            .unwrap_or_else(|| {
                models.push(ModelSpend {
                    model,
                    ..Default::default()
                });
                models.len() - 1
            });
        let entry = &mut models[index];
        entry.calls += 1;
        entry.cost_usd += record.cost_usd.unwrap_or(0.0);
        entry.input_tokens += record.tokens.input.unwrap_or(0);
        entry.output_tokens += record.tokens.output.unwrap_or(0);
        summary.calls += 1;
        summary.cost_usd += record.cost_usd.unwrap_or(0.0);
        summary.input_tokens += record.tokens.input.unwrap_or(0);
        summary.output_tokens += record.tokens.output.unwrap_or(0);
        if record.cost_usd.is_none() {
            summary.unpriced_calls += 1;
        }
    }
    summary.by_model = models;
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation() -> CallObservation {
        CallObservation {
            requested_model: "vendor/model".into(),
            served_model: Some("vendor/model-served".into()),
            provider: Some("Acme".into()),
            finish_reason: Some("stop".into()),
            input_tokens: Some(100),
            output_tokens: Some(20),
            reasoning_tokens: Some(5),
            cached_tokens: Some(0),
            cost_usd: Some(0.0012),
            response_id: Some("gen-1".into()),
            error: None,
        }
    }

    fn shaped(
        observation: &CallObservation,
        generation: Option<&OpenRouterGeneration>,
    ) -> AiCallRecord {
        shape_ai_call_record(&ShapeRecordInput {
            task: "select paragraph 2 → Shorter",
            attempt: 1,
            outcome: CallOutcome::Valid,
            started_at: "2026-11-14T10:00:00.000Z",
            observation,
            generation,
            timing: CallTiming {
                client_total_ms: 900.0,
                ..Default::default()
            },
        })
    }

    #[test]
    fn combines_telemetry_and_the_generation_endpoint_into_one_record() {
        let generation = OpenRouterGeneration {
            id: Some("gen-1".into()),
            model: Some("vendor/exact".into()),
            provider_name: Some("Other".into()),
            latency: Some(400.0),
            generation_time: Some(350.0),
            finish_reason: Some("length".into()),
            total_cost: Some(0.5),
            ..Default::default()
        };
        let record = shaped(&observation(), Some(&generation));
        assert_eq!(record.served_model.as_deref(), Some("vendor/exact"));
        assert_eq!(record.provider.as_deref(), Some("Other"));
        assert_eq!(record.finish_reason.as_deref(), Some("length"));
        assert_eq!(record.cost_usd, Some(0.0012));
        assert_eq!(record.latency_ms.open_router, Some(400.0));
        assert_eq!(record.latency_ms.generation, Some(350.0));
        assert_eq!(record.generation_id.as_deref(), Some("gen-1"));
    }

    #[test]
    fn falls_back_to_telemetry_when_the_generation_endpoint_is_unavailable() {
        let record = shaped(&observation(), None);
        assert_eq!(record.served_model.as_deref(), Some("vendor/model-served"));
        assert_eq!(record.latency_ms.open_router, None);
        assert_eq!(record.tokens.input, Some(100));
    }

    #[test]
    fn falls_back_to_generation_data_for_missing_telemetry_fields() {
        let sparse = CallObservation {
            requested_model: "m".into(),
            ..Default::default()
        };
        let generation = OpenRouterGeneration {
            id: Some("gen-9".into()),
            tokens_prompt: Some(7),
            tokens_completion: Some(3),
            native_tokens_reasoning: Some(1),
            native_tokens_cached: Some(2),
            total_cost: Some(0.01),
            ..Default::default()
        };
        let record = shaped(&sparse, Some(&generation));
        assert_eq!(
            record.tokens,
            Tokens {
                input: Some(7),
                output: Some(3),
                reasoning: Some(1),
                cached: Some(2)
            }
        );
        assert_eq!(record.cost_usd, Some(0.01));
        assert_eq!(record.generation_id.as_deref(), Some("gen-9"));
    }

    #[test]
    fn uses_none_for_anything_unknown_and_keeps_the_first_suggestion_time() {
        let sparse = CallObservation {
            requested_model: "m".into(),
            ..Default::default()
        };
        let record = shape_ai_call_record(&ShapeRecordInput {
            task: "t",
            attempt: 2,
            outcome: CallOutcome::Invalid,
            started_at: "x",
            observation: &sparse,
            generation: None,
            timing: CallTiming {
                client_total_ms: 10.0,
                first_element_ms: Some(4.0),
                first_suggestion_ms: Some(6.0),
            },
        });
        assert_eq!(record.cost_usd, None);
        assert_eq!(record.served_model, None);
        assert_eq!(record.latency_ms.first_suggestion, Some(6.0));
        assert_eq!(record.latency_ms.first_element, Some(4.0));
        assert_eq!(record.attempt, 2);
        assert_eq!(record.outcome, CallOutcome::Invalid);
    }

    #[test]
    fn serializes_with_the_field_names_of_the_typescript_record() {
        let json = serde_json::to_value(shaped(&observation(), None)).unwrap();
        for key in [
            "startedAt",
            "task",
            "attempt",
            "outcome",
            "requestedModel",
            "servedModel",
            "provider",
            "tokens",
            "costUsd",
            "latencyMs",
            "finishReason",
            "generationId",
            "error",
        ] {
            assert!(json.get(key).is_some(), "{key}");
        }
        assert_eq!(json["outcome"], "valid");
        assert!(json["latencyMs"].get("openRouter").is_some());
        assert!(json["latencyMs"].get("firstSuggestion").is_some());
    }

    #[test]
    fn renders_every_field_in_a_row() {
        let row = format_record_row(&shaped(&observation(), None));
        assert_eq!(
            row,
            "| select paragraph 2 → Shorter | vendor/model | vendor/model-served · Acme | 100 / 20 / 5 / 0 | 0.001200 | 900 / – / – | stop | valid | gen-1 |"
        );
    }

    #[test]
    fn marks_unknown_values_with_a_dash_and_escapes_pipes_and_newlines() {
        let record = AiCallRecord {
            task: "a | b\nc  d".into(),
            attempt: 2,
            outcome: CallOutcome::Error,
            error: Some("rate-limited".into()),
            ..shaped(
                &CallObservation {
                    requested_model: "m".into(),
                    ..Default::default()
                },
                None,
            )
        };
        let row = format_record_row(&record);
        assert!(row.starts_with("| a \\| b c  d | m | – | – / – / – / – | – |"));
        assert!(row.contains("error (retry): rate-limited"));
    }

    #[test]
    fn renders_a_header_a_separator_and_one_row_per_record() {
        let table = format_records_table(&[shaped(&observation(), None)]);
        let lines: Vec<&str> = table.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with("| Task | Model requested"));
        assert_eq!(
            lines[1],
            "| --- | --- | --- | --- | --- | --- | --- | --- | --- |"
        );
    }

    #[test]
    fn the_log_appends_one_json_line_per_record_and_reads_them_back() {
        let dir = tempfile::tempdir().unwrap();
        let log = AiCallLog::new(dir.path());
        assert!(log.read_all().unwrap().is_empty());
        let one = shaped(&observation(), None);
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
        let one = shaped(&observation(), None);
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
        log.append(&shaped(&observation(), None)).unwrap();
        fs::write(
            log.path(),
            format!("garbage\n{}", fs::read_to_string(log.path()).unwrap()),
        )
        .unwrap();
        assert_eq!(log.read_all().unwrap().len(), 1);
    }

    #[test]
    fn summarizes_spend_by_served_model() {
        let one = shaped(&observation(), None);
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
        let summary = summarize_spend(&[one, unpriced, other]);
        assert_eq!(summary.calls, 3);
        assert_eq!(summary.unpriced_calls, 1);
        assert!((summary.cost_usd - 0.5012).abs() < 1e-9);
        assert_eq!(summary.by_model.len(), 2);
        assert_eq!(summary.by_model[0].calls, 2);
        assert_eq!(summary.input_tokens, 300);
    }
}
