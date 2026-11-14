use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JsonSchemaFormat {
    pub name: String,
    pub schema: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub response_format: Option<JsonSchemaFormat>,
    pub max_tokens: Option<u32>,
    pub reasoning_effort: Option<String>,
    pub require_parameters: bool,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            response_format: None,
            max_tokens: None,
            reasoning_effort: None,
            require_parameters: true,
        }
    }

    pub fn body(&self, stream: bool) -> Value {
        let mut body = json!({
            "model": self.model,
            "messages": self.messages,
            "stream": stream,
            "usage": { "include": true },
        });
        let object = body.as_object_mut().expect("body is an object");
        if stream {
            object.insert("stream_options".into(), json!({ "include_usage": true }));
        }
        if let Some(format) = &self.response_format {
            object.insert(
                "response_format".into(),
                json!({
                    "type": "json_schema",
                    "json_schema": { "name": format.name, "strict": true, "schema": format.schema },
                }),
            );
        }
        if let Some(max_tokens) = self.max_tokens {
            object.insert("max_tokens".into(), json!(max_tokens));
        }
        if let Some(effort) = &self.reasoning_effort {
            object.insert("reasoning".into(), json!({ "effort": effort }));
        }
        if self.require_parameters {
            object.insert("provider".into(), json!({ "require_parameters": true }));
        }
        body
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Usage {
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub cached_tokens: Option<i64>,
    pub cost: Option<f64>,
}

impl Usage {
    pub fn from_json(value: &Value) -> Self {
        let int = |v: Option<&Value>| v.and_then(Value::as_i64);
        Self {
            prompt_tokens: int(value.get("prompt_tokens")),
            completion_tokens: int(value.get("completion_tokens")),
            reasoning_tokens: int(value.pointer("/completion_tokens_details/reasoning_tokens")),
            cached_tokens: int(value.pointer("/prompt_tokens_details/cached_tokens")),
            cost: value.get("cost").and_then(Value::as_f64),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatCompletion {
    pub id: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub content: String,
    pub finish_reason: Option<String>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StreamChunk {
    pub id: Option<String>,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub delta: Option<String>,
    pub reasoning_delta: Option<String>,
    pub finish_reason: Option<String>,
    pub usage: Option<Usage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_body_with_a_strict_json_schema_and_the_provider_options() {
        let mut request = ChatRequest::new("vendor/model", vec![ChatMessage::user("hi")]);
        request.response_format = Some(JsonSchemaFormat {
            name: "suggestion".into(),
            schema: json!({"type": "object"}),
        });
        request.max_tokens = Some(4096);
        request.reasoning_effort = Some("low".into());
        let body = request.body(false);
        assert_eq!(body["model"], "vendor/model");
        assert_eq!(body["stream"], false);
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
        assert_eq!(body["response_format"]["json_schema"]["name"], "suggestion");
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["reasoning"]["effort"], "low");
        assert_eq!(body["provider"]["require_parameters"], true);
        assert!(body.get("stream_options").is_none());
    }

    #[test]
    fn a_streaming_body_asks_for_usage() {
        let body = ChatRequest::new("m", vec![]).body(true);
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
        assert!(body.get("response_format").is_none());
        assert!(body.get("reasoning").is_none());
    }

    #[test]
    fn reads_usage_with_the_nested_details() {
        let usage = Usage::from_json(&json!({
            "prompt_tokens": 10, "completion_tokens": 5, "cost": 0.0004,
            "completion_tokens_details": {"reasoning_tokens": 2},
            "prompt_tokens_details": {"cached_tokens": 3}
        }));
        assert_eq!(
            usage,
            Usage {
                prompt_tokens: Some(10),
                completion_tokens: Some(5),
                reasoning_tokens: Some(2),
                cached_tokens: Some(3),
                cost: Some(0.0004)
            }
        );
        assert_eq!(Usage::from_json(&json!({})), Usage::default());
    }
}
