use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use desafinado_ai::{
    records::{CallObservation, CallOutcome, CallTiming, ShapeRecordInput, shape_ai_call_record},
    request::{
        ChatRequest, DEFAULT_MAX_OUTPUT_TOKENS, DEFAULT_MAX_RETRIES, DEFAULT_REASONING, DEFAULT_STALL_MS,
        DEFAULT_TIMEOUTS_MS, OutputShape, chat_request_body, target_output_tokens,
    },
    schemas::parse_suggestion,
    text::utf16_len,
};
use desafinado_core::suggestions::{ClientError, SuggestionResult};
use desafinado_platform::openrouter::{GenerationSchedule, OpenRouterClient, fetch_generation};

use crate::{
    ai_backend::{AiBackend, DescriptionCall, ReviewCall, ReviewEvent, TargetCall},
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
        std::thread::spawn(move || {
            let generation = OpenRouterClient::from_env(services.keys.current())
                .ok()
                .and_then(|client| {
                    fetch_generation(&client, observation.response_id.as_deref(), &GenerationSchedule::default())
                });
            let record = shape_ai_call_record(&ShapeRecordInput {
                task,
                attempt,
                outcome,
                started_at,
                observation,
                generation,
                timing,
            });
            let _ = services.log.append(&record);
        });
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn failed_observation(model: &str, error: &ClientError) -> CallObservation {
    CallObservation {
        requested_model: model.to_string(),
        error: Some(error.message().to_string()),
        ..Default::default()
    }
}

impl AiBackend for OpenRouterAi {
    fn target(&self, call: TargetCall) -> Result<SuggestionResult, ClientError> {
        let model = self.services.model();
        let body = chat_request_body(&ChatRequest {
            model: &model,
            parts: &call.parts,
            shape: OutputShape::Suggestion,
            reasoning: DEFAULT_REASONING,
            max_output_tokens: target_output_tokens(utf16_len(&call.original), DEFAULT_MAX_OUTPUT_TOKENS.target),
        });
        let client = self.client(DEFAULT_TIMEOUTS_MS.target)?;
        let mut last_error = ClientError::Unknown {
            message: "No answer".into(),
        };
        for attempt in 1..=DEFAULT_MAX_RETRIES + 1 {
            let started_at = now_iso();
            let clock = Instant::now();
            match client.chat_completion(&body) {
                Ok(output) => {
                    let parsed = parse_suggestion(&output.content);
                    let outcome = if parsed.is_some() { CallOutcome::Valid } else { CallOutcome::Invalid };
                    self.record(
                        &call.task,
                        attempt,
                        outcome,
                        started_at,
                        output.observation,
                        CallTiming {
                            client_total_ms: clock.elapsed().as_millis() as u64,
                            ..Default::default()
                        },
                    );
                    if let Some(parsed) = parsed {
                        return Ok(SuggestionResult {
                            unchanged: parsed.replacement == call.original,
                            replacement: parsed.replacement,
                            rationale: parsed.rationale,
                        });
                    }
                    last_error = ClientError::InvalidOutput {
                        message: "The answer did not match the schema".into(),
                    };
                }
                Err(error) => {
                    self.record(
                        &call.task,
                        attempt,
                        CallOutcome::Error,
                        started_at,
                        failed_observation(&model, &error),
                        CallTiming {
                            client_total_ms: clock.elapsed().as_millis() as u64,
                            ..Default::default()
                        },
                    );
                    return Err(error);
                }
            }
        }
        Err(last_error)
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
            Ok(client) => client.with_stall(Some(Duration::from_millis(DEFAULT_STALL_MS.first_chunk))),
            Err(error) => {
                emit(ReviewEvent::Failed(error));
                return;
            }
        };
        let abort = Arc::new(AtomicBool::new(false));
        let started_at = now_iso();
        let clock = Instant::now();
        let mut first: Option<f64> = None;
        let result = client.chat_stream_collect(&body, Some(abort.clone()), &mut |delta| {
            first.get_or_insert(clock.elapsed().as_millis() as f64);
            if !emit(ReviewEvent::Chunk(delta.to_string())) {
                abort.store(true, Ordering::SeqCst);
            }
        });
        let timing = CallTiming {
            client_total_ms: clock.elapsed().as_millis() as u64,
            first_element_ms: first,
            first_suggestion_ms: None,
        };
        match result {
            Ok(output) => {
                self.record(&call.task, 1, CallOutcome::Valid, started_at, output.observation, timing);
                emit(ReviewEvent::Done);
            }
            Err(error) => {
                self.record(&call.task, 1, CallOutcome::Error, started_at, failed_observation(&model, &error), timing);
                if !abort.load(Ordering::SeqCst) {
                    emit(ReviewEvent::Failed(error));
                }
            }
        }
    }
}
