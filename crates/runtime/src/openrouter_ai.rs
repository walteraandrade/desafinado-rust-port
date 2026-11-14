use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use desafinado_ai::{
    edits::check_suggestion,
    errors::is_retryable_output_error,
    records::{CallObservation, CallOutcome, CallTiming, ShapeRecordInput, shape_ai_call_record},
    request::{
        ChatRequest, DEFAULT_MAX_OUTPUT_TOKENS, DEFAULT_MAX_RETRIES, DEFAULT_REASONING,
        DEFAULT_STALL_MS, DEFAULT_TIMEOUTS_MS, OutputShape, chat_request_body,
        target_output_tokens,
    },
    review_stream::ReviewStream,
    schemas::parse_suggestion,
    text::utf16_len,
};
use desafinado_core::suggestions::{ClientError, SuggestionResult};
use desafinado_platform::openrouter::{GenerationSchedule, OpenRouterClient, fetch_generation};

use crate::{
    ai_backend::{
        AiBackend, DescriptionCall, ReviewAttempt, ReviewCall, ReviewEvent, TargetCall,
        drive_review, feed_review, outcome_of, stopped_error,
    },
    services::Services,
};

pub struct OpenRouterAi {
    services: Arc<Services>,
}

impl OpenRouterAi {
    pub fn new(services: Arc<Services>) -> Self {
        Self { services }
    }

    fn client(&self, timeout_ms: u64) -> Result<OpenRouterClient, ClientError> {
        Ok(OpenRouterClient::from_env(self.services.keys.current())?
            .with_timeout(Duration::from_millis(timeout_ms))
            .with_stall(Some(Duration::from_millis(DEFAULT_STALL_MS.chunk))))
    }

    fn record(
        &self,
        task: &str,
        attempt: u32,
        outcome: CallOutcome,
        started_at: String,
        observation: CallObservation,
        timing: CallTiming,
    ) {
        let services = self.services.clone();
        let task = task.to_string();
        let response_id = observation.response_id.clone();
        let shape = move |generation| {
            shape_ai_call_record(&ShapeRecordInput {
                task: task.clone(),
                attempt,
                outcome: outcome.clone(),
                started_at: started_at.clone(),
                observation: observation.clone(),
                generation,
                timing: timing.clone(),
            })
        };
        let held = services.hold_record(shape(None));
        std::thread::spawn(move || {
            let generation = OpenRouterClient::from_env(services.keys.current())
                .ok()
                .and_then(|client| {
                    fetch_generation(
                        &client,
                        response_id.as_deref(),
                        &GenerationSchedule::default(),
                    )
                });
            services.complete_record(held, shape(generation));
        });
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn observed(
    mut observation: CallObservation,
    model: &str,
    failure: Option<&ClientError>,
) -> CallObservation {
    if observation.requested_model.is_empty() {
        observation.requested_model = model.to_string();
    }
    observation.error = failure.map(|error| error.kind().to_string());
    observation
}

impl AiBackend for OpenRouterAi {
    fn target(&self, call: TargetCall) -> Result<SuggestionResult, ClientError> {
        let model = self.services.model();
        let body = chat_request_body(&ChatRequest {
            model: &model,
            parts: &call.parts,
            shape: OutputShape::Suggestion,
            reasoning: DEFAULT_REASONING,
            max_output_tokens: target_output_tokens(
                utf16_len(&call.original),
                DEFAULT_MAX_OUTPUT_TOKENS.target,
            ),
        });
        let client = self.client(DEFAULT_TIMEOUTS_MS.target)?;
        let mut attempt = 1;
        loop {
            let started_at = now_iso();
            let clock = Instant::now();
            let (result, observation) = match client.chat_completion(&body) {
                Ok(output) => (
                    checked_suggestion(&call.original, &output.content),
                    output.observation,
                ),
                Err(error) => (Err(error), CallObservation::default()),
            };
            let failure = result.as_ref().err();
            self.record(
                &call.task,
                attempt,
                outcome_of(failure),
                started_at,
                observed(observation, &model, failure),
                CallTiming {
                    client_total_ms: clock.elapsed().as_millis() as u64,
                    ..Default::default()
                },
            );
            match result {
                Err(error)
                    if is_retryable_output_error(&error) && attempt <= DEFAULT_MAX_RETRIES =>
                {
                    attempt += 1;
                }
                result => return result,
            }
        }
    }

    fn description(&self, call: DescriptionCall) -> Result<SuggestionResult, ClientError> {
        self.target(call.target_call())
    }

    fn review(&self, call: ReviewCall, emit: &mut dyn FnMut(ReviewEvent) -> bool) {
        let model = self.services.model();
        let body = chat_request_body(&ChatRequest {
            model: &model,
            parts: &call.parts,
            shape: OutputShape::ReviewEdits,
            reasoning: DEFAULT_REASONING,
            max_output_tokens: DEFAULT_MAX_OUTPUT_TOKENS.review,
        });
        let client = match self.client(DEFAULT_TIMEOUTS_MS.review.max(120_000)) {
            Ok(client) => {
                client.with_stall(Some(Duration::from_millis(DEFAULT_STALL_MS.first_chunk)))
            }
            Err(error) => {
                emit(ReviewEvent::Failed(error));
                return;
            }
        };
        drive_review(
            &mut |attempt, emit| {
                let abort = Arc::new(AtomicBool::new(false));
                let started_at = now_iso();
                let clock = Instant::now();
                let mut stream = ReviewStream::new(call.context.clone());
                let mut first_element: Option<f64> = None;
                let mut first_suggestion: Option<f64> = None;
                let mut validated = 0;
                let result = client.chat_stream_collect(&body, Some(abort.clone()), &mut |delta| {
                    if abort.load(Ordering::SeqCst) {
                        return;
                    }
                    first_element.get_or_insert(clock.elapsed().as_millis() as f64);
                    let alive = feed_review(&mut stream, delta, emit, &mut || {
                        validated += 1;
                        first_suggestion.get_or_insert(clock.elapsed().as_millis() as f64);
                    });
                    if !alive {
                        abort.store(true, Ordering::SeqCst);
                    }
                });
                let stopped = abort.load(Ordering::SeqCst);
                let (failure, observation) = match result {
                    _ if stopped => (Some(stopped_error()), CallObservation::default()),
                    Ok(output) => (stream.finish().err(), output.observation),
                    Err(error) => (Some(error), CallObservation::default()),
                };
                self.record(
                    &call.task,
                    attempt,
                    outcome_of(failure.as_ref()),
                    started_at,
                    observed(observation, &model, failure.as_ref()),
                    CallTiming {
                        client_total_ms: clock.elapsed().as_millis() as u64,
                        first_element_ms: first_element,
                        first_suggestion_ms: first_suggestion,
                    },
                );
                ReviewAttempt {
                    validated,
                    failure: if stopped { None } else { failure },
                    stopped,
                }
            },
            emit,
        );
    }
}

fn checked_suggestion(original: &str, content: &str) -> Result<SuggestionResult, ClientError> {
    let output = parse_suggestion(content).ok_or_else(|| ClientError::InvalidOutput {
        message: "The answer did not match the schema".into(),
    })?;
    check_suggestion(original, &output).map_err(|_| ClientError::InvalidOutput {
        message: "The replacement was empty".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_a_suggestion_answer_against_the_schema_and_the_original() {
        let valid = checked_suggestion(
            "Old text.",
            r#"{"replacement":"New text.","rationale":"Shorter."}"#,
        )
        .unwrap();
        assert_eq!(valid.replacement, "New text.");
        assert!(!valid.unchanged);
        let same = checked_suggestion(
            "Old text.",
            r#"{"replacement":" Old text. ","rationale":"Same."}"#,
        )
        .unwrap();
        assert!(same.unchanged);
        assert!(matches!(
            checked_suggestion("Old.", r#"{"replacement":"  ","rationale":"x"}"#),
            Err(ClientError::InvalidOutput { .. })
        ));
        assert!(matches!(
            checked_suggestion("Old.", "not json"),
            Err(ClientError::InvalidOutput { .. })
        ));
    }

    #[test]
    fn records_the_failure_kind_and_the_requested_model() {
        let observation = observed(
            CallObservation::default(),
            "openai/gpt",
            Some(&ClientError::RateLimited {
                message: "slow down".into(),
            }),
        );
        assert_eq!(observation.requested_model, "openai/gpt");
        assert_eq!(observation.error.as_deref(), Some("rate-limited"));
        let served = observed(
            CallObservation {
                requested_model: "a/b".into(),
                served_model: Some("a/b-1".into()),
                ..Default::default()
            },
            "x/y",
            None,
        );
        assert_eq!(served.requested_model, "a/b");
        assert_eq!(served.error, None);
    }
}
