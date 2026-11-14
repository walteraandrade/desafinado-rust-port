use serde_json::Value;

use crate::errors::ClientError;
use crate::records::CallObservation;

#[derive(Debug, Clone, PartialEq)]
pub enum ChatStreamEvent {
    Delta(String),
    Failed(ClientError),
    Done,
}

#[derive(Debug, Clone)]
pub struct ChatStreamParser {
    requested_model: String,
    buffer: String,
    observation: CallObservation,
    done: bool,
}

fn error_from_payload(error: &Value) -> ClientError {
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("The model reported an error")
        .to_owned();
    let code = error.get("code").and_then(Value::as_u64);
    match code {
        Some(402) => ClientError::OutOfCredit { message },
        Some(429) => ClientError::RateLimited { message },
        Some(code) if (400..600).contains(&code) => ClientError::Api {
            status: code as u16,
            message,
        },
        _ => ClientError::Unknown { message },
    }
}

fn number(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64)
}

impl ChatStreamParser {
    pub fn new(requested_model: &str) -> Self {
        Self {
            requested_model: requested_model.to_owned(),
            buffer: String::new(),
            observation: CallObservation {
                requested_model: requested_model.to_owned(),
                ..CallObservation::default()
            },
            done: false,
        }
    }

    pub fn observation(&self) -> CallObservation {
        CallObservation {
            requested_model: self.requested_model.clone(),
            ..self.observation.clone()
        }
    }

    pub const fn is_done(&self) -> bool {
        self.done
    }

    pub fn feed(&mut self, chunk: &str) -> Vec<ChatStreamEvent> {
        self.buffer.push_str(chunk);
        let mut events = Vec::new();
        while let Some(newline) = self.buffer.find('\n') {
            let line: String = self.buffer.drain(..=newline).collect();
            self.read_line(line.trim_end_matches(['\n', '\r']), &mut events);
        }
        events
    }

    pub fn finish(&mut self) -> Vec<ChatStreamEvent> {
        let rest = std::mem::take(&mut self.buffer);
        let mut events = Vec::new();
        if !rest.trim().is_empty() {
            self.read_line(rest.trim_end_matches(['\n', '\r']), &mut events);
        }
        events
    }

    fn read_line(&mut self, line: &str, events: &mut Vec<ChatStreamEvent>) {
        let Some(data) = line.strip_prefix("data:") else {
            return;
        };
        let data = data.trim();
        if data == "[DONE]" {
            self.done = true;
            events.push(ChatStreamEvent::Done);
            return;
        }
        let Ok(payload) = serde_json::from_str::<Value>(data) else {
            return;
        };
        self.read_payload(&payload, events);
    }

    fn read_payload(&mut self, payload: &Value, events: &mut Vec<ChatStreamEvent>) {
        if let Some(error) = payload.get("error") {
            events.push(ChatStreamEvent::Failed(error_from_payload(error)));
            return;
        }
        if let Some(id) = payload.get("id").and_then(Value::as_str) {
            self.observation.response_id = Some(id.to_owned());
        }
        if let Some(model) = payload.get("model").and_then(Value::as_str) {
            self.observation.served_model = Some(model.to_owned());
        }
        if let Some(provider) = payload.get("provider").and_then(Value::as_str) {
            self.observation.provider = Some(provider.to_owned());
        }
        if let Some(choice) = payload.get("choices").and_then(|c| c.get(0)) {
            if let Some(content) = choice
                .get("delta")
                .and_then(|delta| delta.get("content"))
                .and_then(Value::as_str)
                .filter(|content| !content.is_empty())
            {
                events.push(ChatStreamEvent::Delta(content.to_owned()));
            }
            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                self.observation.finish_reason = Some(reason.to_owned());
            }
        }
        if let Some(usage) = payload.get("usage").filter(|usage| usage.is_object()) {
            self.read_usage(usage);
        }
    }

    fn read_usage(&mut self, usage: &Value) {
        let observation = &mut self.observation;
        observation.input_tokens = number(usage.get("prompt_tokens")).or(observation.input_tokens);
        observation.output_tokens =
            number(usage.get("completion_tokens")).or(observation.output_tokens);
        observation.reasoning_tokens = number(
            usage
                .get("completion_tokens_details")
                .and_then(|d| d.get("reasoning_tokens")),
        )
        .or(observation.reasoning_tokens);
        observation.cached_tokens = number(
            usage
                .get("prompt_tokens_details")
                .and_then(|d| d.get("cached_tokens")),
        )
        .or(observation.cached_tokens);
        observation.cost_usd = number(usage.get("cost")).or(observation.cost_usd);
    }
}

pub fn observation_from_response(requested_model: &str, body: &Value) -> (String, CallObservation) {
    let mut parser = ChatStreamParser::new(requested_model);
    let mut events = Vec::new();
    parser.read_payload(body, &mut events);
    let content = body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if let Some(reason) = body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("finish_reason"))
        .and_then(Value::as_str)
    {
        parser.observation.finish_reason = Some(reason.to_owned());
    }
    (content, parser.observation())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sse(payload: &Value) -> String {
        format!("data: {payload}\n\n")
    }

    #[test]
    fn collects_deltas_and_the_observation() {
        let mut parser = ChatStreamParser::new("vendor/model-a");
        let mut events = Vec::new();
        events.extend(parser.feed(": OPENROUTER PROCESSING\n\n"));
        events.extend(parser.feed(&sse(&json!({
            "id": "gen-1", "model": "vendor/model-a-2026", "provider": "Acme",
            "choices": [{"delta": {"content": "{\"a\""}}]
        }))));
        events.extend(parser.feed(&sse(
            &json!({"choices": [{"delta": {"content": ":1}"}, "finish_reason": "stop"}]}),
        )));
        events.extend(parser.feed(&sse(&json!({
            "choices": [],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "cost": 0.0002,
                "completion_tokens_details": {"reasoning_tokens": 2},
                "prompt_tokens_details": {"cached_tokens": 1}}
        }))));
        events.extend(parser.feed("data: [DONE]\n\n"));
        let text: String = events
            .iter()
            .filter_map(|e| match e {
                ChatStreamEvent::Delta(d) => Some(d.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(text, "{\"a\":1}");
        assert!(parser.is_done());
        let observation = parser.observation();
        assert_eq!(observation.response_id.as_deref(), Some("gen-1"));
        assert_eq!(
            observation.served_model.as_deref(),
            Some("vendor/model-a-2026")
        );
        assert_eq!(observation.provider.as_deref(), Some("Acme"));
        assert_eq!(observation.finish_reason.as_deref(), Some("stop"));
        assert_eq!(observation.input_tokens, Some(10.0));
        assert_eq!(observation.reasoning_tokens, Some(2.0));
        assert_eq!(observation.cached_tokens, Some(1.0));
        assert_eq!(observation.cost_usd, Some(0.0002));
    }

    #[test]
    fn handles_lines_split_across_chunks() {
        let mut parser = ChatStreamParser::new("m/x");
        let line = sse(&json!({"choices": [{"delta": {"content": "héllo"}}]}));
        let (a, b) = line.split_at(20);
        assert!(parser.feed(a).is_empty());
        assert_eq!(parser.feed(b), [ChatStreamEvent::Delta("héllo".into())]);
    }

    #[test]
    fn classifies_an_error_payload() {
        let mut parser = ChatStreamParser::new("m/x");
        let events = parser.feed(&sse(
            &json!({"error": {"code": 402, "message": "no credit"}}),
        ));
        assert_eq!(
            events,
            [ChatStreamEvent::Failed(ClientError::OutOfCredit {
                message: "no credit".into()
            })]
        );
    }

    #[test]
    fn reads_a_non_streaming_response() {
        let (content, observation) = observation_from_response(
            "m/x",
            &json!({
                "id": "gen-9", "model": "m/x", "provider": "Acme",
                "choices": [{"message": {"content": "{}"}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 3, "completion_tokens": 4, "cost": 0.5}
            }),
        );
        assert_eq!(content, "{}");
        assert_eq!(observation.cost_usd, Some(0.5));
        assert_eq!(observation.finish_reason.as_deref(), Some("stop"));
    }
}
