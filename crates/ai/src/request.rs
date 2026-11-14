use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::errors::ClientError;
use crate::prompts::PromptParts;
use crate::schemas::{review_response_format, suggestion_response_format};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningSetting {
    Default,
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

impl ReasoningSetting {
    pub const fn effort(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::None => Some("none"),
            Self::Minimal => Some("minimal"),
            Self::Low => Some("low"),
            Self::Medium => Some("medium"),
            Self::High => Some("high"),
            Self::Xhigh => Some("xhigh"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestKind {
    Target,
    Review,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestKindLimits {
    pub target: u64,
    pub review: u64,
}

impl RequestKindLimits {
    pub const fn get(&self, kind: RequestKind) -> u64 {
        match kind {
            RequestKind::Target => self.target,
            RequestKind::Review => self.review,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StallLimits {
    pub first_chunk: u64,
    pub chunk: u64,
}

pub const DEFAULT_TIMEOUTS_MS: RequestKindLimits = RequestKindLimits {
    target: 600_000,
    review: 30_000,
};
pub const DEFAULT_STALL_MS: StallLimits = StallLimits {
    first_chunk: 30_000,
    chunk: 15_000,
};
pub const MIN_RETRY_MS: u64 = 1000;
pub const DEFAULT_MAX_OUTPUT_TOKENS: RequestKindLimits = RequestKindLimits {
    target: 4096,
    review: 8192,
};
pub const MAX_TARGET_OUTPUT_TOKENS: u64 = 32_768;
pub const DEFAULT_MAX_RETRIES: u32 = 1;
pub const DEFAULT_REASONING: ReasoningSetting = ReasoningSetting::Low;

pub fn target_output_tokens(original_utf16_len: usize, configured_target: u64) -> u64 {
    let scaled = (original_utf16_len as f64 * 0.75).ceil() as u64;
    MAX_TARGET_OUTPUT_TOKENS.min(configured_target.max(scaled))
}

pub fn timeout_error(kind: RequestKind, limits: &RequestKindLimits) -> ClientError {
    ClientError::Timeout {
        message: format!("The request took longer than {} ms", limits.get(kind)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputShape {
    Suggestion,
    ReviewEdits,
}

#[derive(Debug, Clone)]
pub struct ChatRequest<'a> {
    pub model: &'a str,
    pub parts: &'a PromptParts,
    pub shape: OutputShape,
    pub reasoning: ReasoningSetting,
    pub max_output_tokens: u64,
}

pub fn chat_request_body(request: &ChatRequest<'_>) -> Value {
    let response_format = match request.shape {
        OutputShape::Suggestion => suggestion_response_format(),
        OutputShape::ReviewEdits => review_response_format(),
    };
    let mut body = json!({
        "model": request.model,
        "messages": [
            { "role": "system", "content": request.parts.instructions },
            { "role": "user", "content": request.parts.prompt }
        ],
        "max_tokens": request.max_output_tokens,
        "stream": true,
        "usage": { "include": true },
        "response_format": response_format,
        "provider": { "require_parameters": true }
    });
    if let Some(effort) = request.reasoning.effort() {
        body["reasoning"] = json!({ "effort": effort });
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts() -> PromptParts {
        PromptParts {
            instructions: "rules".into(),
            prompt: "do it".into(),
        }
    }

    #[test]
    fn builds_a_strict_structured_streaming_request() {
        let parts = parts();
        let body = chat_request_body(&ChatRequest {
            model: "vendor/model-a",
            parts: &parts,
            shape: OutputShape::Suggestion,
            reasoning: ReasoningSetting::Low,
            max_output_tokens: 4096,
        });
        assert_eq!(body["model"], "vendor/model-a");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "rules");
        assert_eq!(body["messages"][1]["content"], "do it");
        assert_eq!(body["response_format"]["json_schema"]["strict"], true);
        assert_eq!(body["provider"]["require_parameters"], true);
        assert_eq!(body["reasoning"]["effort"], "low");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 4096);
    }

    #[test]
    fn omits_reasoning_for_the_default_setting_and_wraps_review_edits() {
        let parts = parts();
        let body = chat_request_body(&ChatRequest {
            model: "m/x",
            parts: &parts,
            shape: OutputShape::ReviewEdits,
            reasoning: ReasoningSetting::Default,
            max_output_tokens: 8192,
        });
        assert!(body.get("reasoning").is_none());
        assert_eq!(
            body["response_format"]["json_schema"]["schema"]["required"][0],
            "elements"
        );
    }

    #[test]
    fn scales_the_target_output_budget_with_the_original() {
        assert_eq!(target_output_tokens(100, 4096), 4096);
        assert_eq!(target_output_tokens(10_000, 4096), 7500);
        assert_eq!(
            target_output_tokens(1_000_000, 4096),
            MAX_TARGET_OUTPUT_TOKENS
        );
    }

    #[test]
    fn names_the_timeout_after_the_limit() {
        assert_eq!(
            timeout_error(RequestKind::Review, &DEFAULT_TIMEOUTS_MS).message(),
            "The request took longer than 30000 ms"
        );
    }
}
