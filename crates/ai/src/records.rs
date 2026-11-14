use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallObservation {
    pub requested_model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub served_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct OpenRouterGeneration {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub provider_name: Option<String>,
    #[serde(default)]
    pub latency: Option<f64>,
    #[serde(default)]
    pub generation_time: Option<f64>,
    #[serde(default)]
    pub finish_reason: Option<String>,
    #[serde(default)]
    pub tokens_prompt: Option<f64>,
    #[serde(default)]
    pub tokens_completion: Option<f64>,
    #[serde(default)]
    pub native_tokens_reasoning: Option<f64>,
    #[serde(default)]
    pub native_tokens_cached: Option<f64>,
    #[serde(default)]
    pub total_cost: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CallTiming {
    pub client_total_ms: u64,
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
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Invalid => "invalid",
            Self::Error => "error",
        }
    }
}

fn whole_number<S: Serializer>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error> {
    match value {
        None => serializer.serialize_none(),
        Some(number) if number.fract() == 0.0 && number.abs() < 9.0e15 => {
            serializer.serialize_i64(*number as i64)
        }
        Some(number) => serializer.serialize_f64(*number),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tokens {
    #[serde(serialize_with = "whole_number")]
    pub input: Option<f64>,
    #[serde(serialize_with = "whole_number")]
    pub output: Option<f64>,
    #[serde(serialize_with = "whole_number")]
    pub reasoning: Option<f64>,
    #[serde(serialize_with = "whole_number")]
    pub cached: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatencyMs {
    pub client: u64,
    #[serde(serialize_with = "whole_number")]
    pub open_router: Option<f64>,
    #[serde(serialize_with = "whole_number")]
    pub generation: Option<f64>,
    #[serde(serialize_with = "whole_number")]
    pub first_element: Option<f64>,
    #[serde(serialize_with = "whole_number")]
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

#[derive(Debug, Clone)]
pub struct ShapeRecordInput {
    pub task: String,
    pub attempt: u32,
    pub outcome: CallOutcome,
    pub started_at: String,
    pub observation: CallObservation,
    pub generation: Option<OpenRouterGeneration>,
    pub timing: CallTiming,
}

fn first_defined<T: Clone>(values: [Option<&T>; 2]) -> Option<T> {
    values.into_iter().flatten().next().cloned()
}

pub fn shape_ai_call_record(input: &ShapeRecordInput) -> AiCallRecord {
    let ShapeRecordInput {
        task,
        attempt,
        outcome,
        started_at,
        observation,
        generation,
        timing,
    } = input;
    let generation = generation.clone().unwrap_or_default();
    AiCallRecord {
        started_at: started_at.clone(),
        task: task.clone(),
        attempt: *attempt,
        outcome: *outcome,
        requested_model: observation.requested_model.clone(),
        served_model: first_defined([generation.model.as_ref(), observation.served_model.as_ref()]),
        provider: first_defined([
            generation.provider_name.as_ref(),
            observation.provider.as_ref(),
        ]),
        tokens: Tokens {
            input: first_defined([
                observation.input_tokens.as_ref(),
                generation.tokens_prompt.as_ref(),
            ]),
            output: first_defined([
                observation.output_tokens.as_ref(),
                generation.tokens_completion.as_ref(),
            ]),
            reasoning: first_defined([
                observation.reasoning_tokens.as_ref(),
                generation.native_tokens_reasoning.as_ref(),
            ]),
            cached: first_defined([
                observation.cached_tokens.as_ref(),
                generation.native_tokens_cached.as_ref(),
            ]),
        },
        cost_usd: first_defined([
            observation.cost_usd.as_ref(),
            generation.total_cost.as_ref(),
        ]),
        latency_ms: LatencyMs {
            client: timing.client_total_ms,
            open_router: generation.latency,
            generation: generation.generation_time,
            first_element: timing.first_element_ms,
            first_suggestion: timing.first_suggestion_ms,
        },
        finish_reason: first_defined([
            generation.finish_reason.as_ref(),
            observation.finish_reason.as_ref(),
        ]),
        generation_id: first_defined([observation.response_id.as_ref(), generation.id.as_ref()]),
        error: observation.error.clone(),
    }
}

const DASH: &str = "–";

fn cell(value: &str) -> String {
    let escaped = value.replace('|', "\\|");
    let mut out = String::new();
    let mut run = String::new();
    for c in escaped.chars() {
        if c.is_whitespace() {
            run.push(c);
        } else {
            flush_run(&mut out, &mut run);
            out.push(c);
        }
    }
    flush_run(&mut out, &mut run);
    out
}

fn flush_run(out: &mut String, run: &mut String) {
    if run.is_empty() {
        return;
    }
    if run.contains('\n') {
        out.push(' ');
    } else {
        out.push_str(run);
    }
    run.clear();
}

fn show_number(value: Option<f64>) -> String {
    value.map_or_else(|| DASH.to_owned(), |number| number.to_string())
}

fn show_text(value: Option<&str>) -> String {
    value.unwrap_or(DASH).to_owned()
}

fn format_cost(cost: Option<f64>) -> String {
    cost.map_or_else(|| DASH.to_owned(), |cost| format!("{cost:.6}"))
}

fn format_latency(record: &AiCallRecord) -> String {
    let latency = &record.latency_ms;
    let base = [
        latency.client.to_string(),
        show_number(latency.open_router),
        show_number(latency.generation),
    ]
    .join(" / ");
    match latency.first_suggestion {
        None => base,
        Some(first) => format!("{base} (first {first})"),
    }
}

fn format_outcome(record: &AiCallRecord) -> String {
    let retry = if record.attempt > 1 { " (retry)" } else { "" };
    let error = record
        .error
        .as_deref()
        .filter(|error| !error.is_empty())
        .map(|error| format!(": {error}"))
        .unwrap_or_default();
    format!("{}{retry}{error}", record.outcome.as_str())
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
    let served = [record.served_model.as_deref(), record.provider.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    let tokens = [
        record.tokens.input,
        record.tokens.output,
        record.tokens.reasoning,
        record.tokens.cached,
    ]
    .map(show_number)
    .join(" / ");
    let cells = [
        record.task.clone(),
        record.requested_model.clone(),
        if served.is_empty() {
            DASH.to_owned()
        } else {
            served
        },
        tokens,
        format_cost(record.cost_usd),
        format_latency(record),
        show_text(record.finish_reason.as_deref()),
        format_outcome(record),
        show_text(record.generation_id.as_deref()),
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
    let header = format!("| {} |", RECORD_TABLE_COLUMNS.join(" | "));
    let separator = format!("|{}", " --- |".repeat(RECORD_TABLE_COLUMNS.len()));
    [header, separator]
        .into_iter()
        .chain(records.iter().map(format_record_row))
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostBucket {
    pub calls: usize,
    pub priced_calls: usize,
    pub cost_usd: f64,
    pub input_tokens: f64,
    pub output_tokens: f64,
    pub reasoning_tokens: f64,
    pub cached_tokens: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostSummary {
    pub total: CostBucket,
    pub by_model: BTreeMap<String, CostBucket>,
    pub by_task: BTreeMap<String, CostBucket>,
}

fn add_to(bucket: &mut CostBucket, record: &AiCallRecord) {
    bucket.calls += 1;
    if let Some(cost) = record.cost_usd {
        bucket.priced_calls += 1;
        bucket.cost_usd += cost;
    }
    bucket.input_tokens += record.tokens.input.unwrap_or(0.0);
    bucket.output_tokens += record.tokens.output.unwrap_or(0.0);
    bucket.reasoning_tokens += record.tokens.reasoning.unwrap_or(0.0);
    bucket.cached_tokens += record.tokens.cached.unwrap_or(0.0);
}

pub fn total_cost_usd(records: &[AiCallRecord]) -> f64 {
    records.iter().filter_map(|record| record.cost_usd).sum()
}

pub fn summarize_costs(records: &[AiCallRecord]) -> CostSummary {
    records
        .iter()
        .fold(CostSummary::default(), |mut summary, record| {
            add_to(&mut summary.total, record);
            add_to(
                summary
                    .by_model
                    .entry(
                        record
                            .served_model
                            .clone()
                            .unwrap_or_else(|| record.requested_model.clone()),
                    )
                    .or_default(),
                record,
            );
            add_to(
                summary.by_task.entry(record.task.clone()).or_default(),
                record,
            );
            summary
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation() -> CallObservation {
        CallObservation {
            requested_model: "vendor/model-a".into(),
            served_model: Some("vendor/model-a-20260101".into()),
            provider: Some("FromTelemetry".into()),
            finish_reason: Some("stop".into()),
            input_tokens: Some(812.0),
            output_tokens: Some(120.0),
            reasoning_tokens: Some(30.0),
            cached_tokens: Some(0.0),
            cost_usd: Some(0.000412),
            response_id: Some("gen-abc".into()),
            error: None,
        }
    }

    fn generation() -> OpenRouterGeneration {
        OpenRouterGeneration {
            id: Some("gen-abc".into()),
            model: Some("vendor/model-a".into()),
            provider_name: Some("Acme".into()),
            latency: Some(1620.0),
            generation_time: Some(1410.0),
            finish_reason: Some("stop".into()),
            tokens_prompt: Some(800.0),
            tokens_completion: Some(110.0),
            native_tokens_reasoning: Some(25.0),
            native_tokens_cached: Some(5.0),
            total_cost: Some(0.0005),
        }
    }

    fn base(
        observation: CallObservation,
        generation: Option<OpenRouterGeneration>,
    ) -> ShapeRecordInput {
        ShapeRecordInput {
            task: "select paragraph 2 → Shorter".into(),
            attempt: 1,
            outcome: CallOutcome::Valid,
            started_at: "2026-10-28T12:00:00.000Z".into(),
            observation,
            generation,
            timing: CallTiming {
                client_total_ms: 1850,
                ..CallTiming::default()
            },
        }
    }

    #[test]
    fn combines_telemetry_and_the_generation_endpoint_into_one_record() {
        let record = shape_ai_call_record(&base(observation(), Some(generation())));
        assert_eq!(
            record,
            AiCallRecord {
                started_at: "2026-10-28T12:00:00.000Z".into(),
                task: "select paragraph 2 → Shorter".into(),
                attempt: 1,
                outcome: CallOutcome::Valid,
                requested_model: "vendor/model-a".into(),
                served_model: Some("vendor/model-a".into()),
                provider: Some("Acme".into()),
                tokens: Tokens {
                    input: Some(812.0),
                    output: Some(120.0),
                    reasoning: Some(30.0),
                    cached: Some(0.0)
                },
                cost_usd: Some(0.000412),
                latency_ms: LatencyMs {
                    client: 1850,
                    open_router: Some(1620.0),
                    generation: Some(1410.0),
                    first_element: None,
                    first_suggestion: None
                },
                finish_reason: Some("stop".into()),
                generation_id: Some("gen-abc".into()),
                error: None,
            }
        );
    }

    #[test]
    fn falls_back_to_telemetry_when_the_generation_endpoint_is_unavailable() {
        let record = shape_ai_call_record(&base(observation(), None));
        assert_eq!(record.provider.as_deref(), Some("FromTelemetry"));
        assert_eq!(
            record.served_model.as_deref(),
            Some("vendor/model-a-20260101")
        );
        assert_eq!(record.latency_ms.open_router, None);
        assert_eq!(record.latency_ms.generation, None);
        assert_eq!(record.generation_id.as_deref(), Some("gen-abc"));
    }

    #[test]
    fn falls_back_to_generation_data_for_missing_telemetry_fields() {
        let record = shape_ai_call_record(&base(
            CallObservation {
                requested_model: "vendor/model-a".into(),
                ..CallObservation::default()
            },
            Some(generation()),
        ));
        assert_eq!(
            record.tokens,
            Tokens {
                input: Some(800.0),
                output: Some(110.0),
                reasoning: Some(25.0),
                cached: Some(5.0)
            }
        );
        assert_eq!(record.cost_usd, Some(0.0005));
        assert_eq!(record.finish_reason.as_deref(), Some("stop"));
        assert_eq!(record.generation_id.as_deref(), Some("gen-abc"));
    }

    #[test]
    fn uses_none_for_anything_unknown() {
        let mut input = base(
            CallObservation {
                requested_model: "vendor/model-a".into(),
                error: Some("out-of-credit".into()),
                ..CallObservation::default()
            },
            None,
        );
        input.outcome = CallOutcome::Error;
        let record = shape_ai_call_record(&input);
        assert_eq!(
            record.tokens,
            Tokens {
                input: None,
                output: None,
                reasoning: None,
                cached: None
            }
        );
        assert_eq!(record.cost_usd, None);
        assert_eq!(record.served_model, None);
        assert_eq!(record.provider, None);
        assert_eq!(record.finish_reason, None);
        assert_eq!(record.generation_id, None);
        assert_eq!(record.error.as_deref(), Some("out-of-credit"));
    }

    #[test]
    fn keeps_the_time_to_the_first_suggestion_of_a_review_pass() {
        let mut input = base(observation(), Some(generation()));
        input.task = "Review pass".into();
        input.timing = CallTiming {
            client_total_ms: 4210,
            first_element_ms: Some(900.0),
            first_suggestion_ms: Some(1320.0),
        };
        let record = shape_ai_call_record(&input);
        assert_eq!(record.latency_ms.first_element, Some(900.0));
        assert_eq!(record.latency_ms.first_suggestion, Some(1320.0));
    }

    #[test]
    fn marks_a_retry_attempt_and_an_invalid_outcome() {
        let mut input = base(observation(), None);
        input.attempt = 2;
        input.outcome = CallOutcome::Invalid;
        let record = shape_ai_call_record(&input);
        assert_eq!(record.attempt, 2);
        assert_eq!(record.outcome, CallOutcome::Invalid);
    }

    fn full_record() -> AiCallRecord {
        shape_ai_call_record(&base(observation(), Some(generation())))
    }

    #[test]
    fn row_renders_every_field_of_the_ai_call_record() {
        let row = format_record_row(&full_record());
        assert!(row.starts_with("| "));
        assert!(row.ends_with(" |"));
        for expected in [
            "select paragraph 2 → Shorter",
            "vendor/model-a",
            "Acme",
            "812 / 120 / 30 / 0",
            "0.000412",
            "1850 / 1620 / 1410",
            "stop",
            "valid",
            "gen-abc",
        ] {
            assert!(row.contains(expected), "missing {expected}");
        }
    }

    #[test]
    fn row_marks_unknown_values_with_a_dash() {
        let mut input = base(
            CallObservation {
                requested_model: "vendor/model-a".into(),
                ..CallObservation::default()
            },
            None,
        );
        input.outcome = CallOutcome::Error;
        let row = format_record_row(&shape_ai_call_record(&input));
        assert!(row.contains("– / – / – / –"));
        assert!(row.contains("error"));
    }

    #[test]
    fn row_shows_the_time_to_first_suggestion() {
        let mut input = base(observation(), Some(generation()));
        input.timing = CallTiming {
            client_total_ms: 4210,
            first_suggestion_ms: Some(1320.0),
            ..CallTiming::default()
        };
        assert!(format_record_row(&shape_ai_call_record(&input)).contains("first 1320"));
    }

    #[test]
    fn row_escapes_pipes_and_newlines_in_the_task() {
        let row = format_record_row(&AiCallRecord {
            task: "a | b\nc".into(),
            ..full_record()
        });
        assert!(row.contains("a \\| b c"));
        assert_eq!(row.split('\n').count(), 1);
    }

    #[test]
    fn row_collapses_whitespace_runs_around_newlines_and_keeps_other_runs() {
        let row = format_record_row(&AiCallRecord {
            task: "a \t\r\n\n  b  c".into(),
            ..full_record()
        });
        assert!(row.contains("a b  c"));
    }

    #[test]
    fn row_shows_the_attempt_number_for_a_retry() {
        let row = format_record_row(&AiCallRecord {
            attempt: 2,
            ..full_record()
        });
        assert!(row.contains("retry"));
    }

    #[test]
    fn table_renders_a_header_a_separator_and_one_row_per_record() {
        let table = format_records_table(&[full_record(), full_record()]);
        let lines: Vec<&str> = table.split('\n').collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].starts_with("| Task |"));
        assert!(
            regex::Regex::new(r"^\|(?: --- \|)+$")
                .unwrap()
                .is_match(lines[1])
        );
    }

    #[test]
    fn serializes_to_the_same_json_shape_as_the_typescript_record() {
        let json = serde_json::to_value(full_record()).unwrap();
        assert_eq!(json["tokens"]["input"], 812);
        assert_eq!(json["latencyMs"]["openRouter"], 1620);
        assert_eq!(json["costUsd"], 0.000412);
        assert_eq!(json["generationId"], "gen-abc");
        assert!(json["error"].is_null());
        let back: AiCallRecord = serde_json::from_value(json).unwrap();
        assert_eq!(back, full_record());
    }

    #[test]
    fn sums_cost_and_tokens_by_model_and_task() {
        let cheap = full_record();
        let other = AiCallRecord {
            served_model: Some("vendor/model-b".into()),
            task: "Review pass".into(),
            cost_usd: Some(0.001),
            ..full_record()
        };
        let unpriced = AiCallRecord {
            cost_usd: None,
            ..full_record()
        };
        let records = [cheap, other, unpriced];
        assert!((total_cost_usd(&records) - 0.001412).abs() < 1e-12);
        let summary = summarize_costs(&records);
        assert_eq!(summary.total.calls, 3);
        assert_eq!(summary.total.priced_calls, 2);
        assert_eq!(summary.total.input_tokens, 2436.0);
        assert_eq!(summary.by_model["vendor/model-a"].calls, 2);
        assert_eq!(summary.by_model["vendor/model-b"].calls, 1);
        assert_eq!(summary.by_task["Review pass"].calls, 1);
    }
}
