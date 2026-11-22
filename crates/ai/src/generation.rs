use crate::api_key::ApiKey;
use serde_json::Value;

use crate::records::OpenRouterGeneration;
use crate::text::percent_encode_component;

pub const DEFAULT_GENERATION_INITIAL_DELAY_MS: u64 = 30_000;
pub const DEFAULT_GENERATION_ATTEMPT_DELAY_MS: u64 = 10_000;
pub const DEFAULT_GENERATION_RETRIES: usize = 24;

pub fn default_generation_delays_ms() -> Vec<u64> {
    vec![DEFAULT_GENERATION_ATTEMPT_DELAY_MS; DEFAULT_GENERATION_RETRIES]
}

#[expect(
    clippy::option_option,
    reason = "the outer None marks a malformed field; the inner None an absent or null one"
)]
fn nullish_number(data: &serde_json::Map<String, Value>, key: &str) -> Option<Option<f64>> {
    match data.get(key) {
        None | Some(Value::Null) => Some(None),
        Some(Value::Number(number)) => number.as_f64().map(Some),
        Some(_) => None,
    }
}

#[expect(
    clippy::option_option,
    reason = "the outer None marks a malformed field; the inner None an absent or null one"
)]
fn nullish_string(data: &serde_json::Map<String, Value>, key: &str) -> Option<Option<String>> {
    match data.get(key) {
        None | Some(Value::Null) => Some(None),
        Some(Value::String(text)) => Some(Some(text.clone())),
        Some(_) => None,
    }
}

#[expect(
    clippy::option_option,
    reason = "the outer None marks a malformed field; the inner None an absent or null one"
)]
fn optional_string(data: &serde_json::Map<String, Value>, key: &str) -> Option<Option<String>> {
    match data.get(key) {
        None => Some(None),
        Some(Value::String(text)) => Some(Some(text.clone())),
        Some(_) => None,
    }
}

pub fn parse_generation(body: &Value) -> Option<OpenRouterGeneration> {
    let data = body.get("data")?.as_object()?;
    Some(OpenRouterGeneration {
        id: optional_string(data, "id")?,
        model: optional_string(data, "model")?,
        provider_name: nullish_string(data, "provider_name")?,
        latency: nullish_number(data, "latency")?,
        generation_time: nullish_number(data, "generation_time")?,
        finish_reason: nullish_string(data, "finish_reason")?,
        tokens_prompt: nullish_number(data, "tokens_prompt")?,
        tokens_completion: nullish_number(data, "tokens_completion")?,
        native_tokens_reasoning: nullish_number(data, "native_tokens_reasoning")?,
        native_tokens_cached: nullish_number(data, "native_tokens_cached")?,
        total_cost: nullish_number(data, "total_cost")?,
    })
}

pub fn generation_url(base_url: &str, id: &str) -> String {
    format!("{base_url}/generation?id={}", percent_encode_component(id))
}

#[derive(Debug, Clone)]
pub struct FetchGenerationOptions {
    pub base_url: String,
    pub api_key: ApiKey,
    pub id: Option<String>,
    pub initial_delay_ms: u64,
    pub delays_ms: Vec<u64>,
}

impl FetchGenerationOptions {
    pub fn new(base_url: &str, api_key: ApiKey, id: Option<&str>) -> Self {
        Self {
            base_url: base_url.to_owned(),
            api_key,
            id: id.map(str::to_owned),
            initial_delay_ms: DEFAULT_GENERATION_INITIAL_DELAY_MS,
            delays_ms: default_generation_delays_ms(),
        }
    }
}

pub struct GenerationRequest<'a> {
    pub url: &'a str,
    pub authorization: &'a str,
}

pub fn fetch_generation<F, S>(
    options: &FetchGenerationOptions,
    mut fetch_json: F,
    mut sleep: S,
) -> Option<OpenRouterGeneration>
where
    F: FnMut(&GenerationRequest<'_>) -> Option<Value>,
    S: FnMut(u64),
{
    let id = options.id.as_deref()?;
    let url = generation_url(&options.base_url, id);
    let authorization = format!("Bearer {}", options.api_key.expose());
    if options.initial_delay_ms > 0 {
        sleep(options.initial_delay_ms);
    }
    for attempt in 0..=options.delays_ms.len() {
        let request = GenerationRequest {
            url: &url,
            authorization: &authorization,
        };
        if let Some(generation) = fetch_json(&request).and_then(|body| parse_generation(&body)) {
            return Some(generation);
        }
        if let Some(delay) = options.delays_ms.get(attempt) {
            sleep(*delay);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn generation_body() -> Value {
        json!({"data": {
            "id": "gen-1", "model": "vendor/model-a", "provider_name": "Acme",
            "latency": 1200, "generation_time": 1000, "finish_reason": "stop",
            "tokens_prompt": 10, "tokens_completion": 5,
            "native_tokens_reasoning": 0, "native_tokens_cached": 0, "total_cost": 0.0001
        }})
    }

    struct Scripted {
        responses: Vec<Option<Value>>,
        calls: Vec<(String, String)>,
    }

    impl Scripted {
        fn new(responses: Vec<Option<Value>>) -> Self {
            Self {
                responses,
                calls: Vec::new(),
            }
        }

        fn fetch(&mut self, request: &GenerationRequest<'_>) -> Option<Value> {
            let index = self.calls.len().min(self.responses.len() - 1);
            self.calls
                .push((request.url.to_owned(), request.authorization.to_owned()));
            self.responses[index].clone()
        }
    }

    fn options(
        id: Option<&str>,
        initial: u64,
        delays: &[u64],
        key: &str,
    ) -> FetchGenerationOptions {
        FetchGenerationOptions {
            base_url: "http://localhost/api/openrouter".into(),
            api_key: ApiKey::new(key).unwrap(),
            id: id.map(str::to_owned),
            initial_delay_ms: initial,
            delays_ms: delays.to_vec(),
        }
    }

    #[test]
    fn requests_the_generation_by_id_with_the_authorization_header() {
        let mut script = Scripted::new(vec![Some(generation_body())]);
        let mut delays = Vec::new();
        let generation = fetch_generation(
            &options(Some("gen-1"), 0, &[], "placeholder"),
            |r| script.fetch(r),
            |ms| delays.push(ms),
        )
        .unwrap();
        assert_eq!(generation.id.as_deref(), Some("gen-1"));
        assert_eq!(generation.provider_name.as_deref(), Some("Acme"));
        assert_eq!(script.calls.len(), 1);
        assert_eq!(
            script.calls[0].0,
            "http://localhost/api/openrouter/generation?id=gen-1"
        );
        assert_eq!(script.calls[0].1, "Bearer placeholder");
    }

    #[test]
    fn waits_before_the_first_attempt() {
        let mut script = Scripted::new(vec![Some(generation_body())]);
        let mut delays = Vec::new();
        fetch_generation(
            &options(Some("gen-1"), 500, &[], "k"),
            |r| script.fetch(r),
            |ms| delays.push(ms),
        );
        assert_eq!(delays, [500]);
        assert_eq!(script.calls.len(), 1);
    }

    #[test]
    fn retries_a_404_with_backoff_until_the_generation_shows_up() {
        let mut script = Scripted::new(vec![None, None, Some(generation_body())]);
        let mut delays = Vec::new();
        let generation = fetch_generation(
            &options(Some("gen-1"), 0, &[100, 200, 400], "k"),
            |r| script.fetch(r),
            |ms| delays.push(ms),
        );
        assert_eq!(generation.unwrap().id.as_deref(), Some("gen-1"));
        assert_eq!(script.calls.len(), 3);
        assert_eq!(delays, [100, 200]);
    }

    #[test]
    fn gives_up_after_the_last_delay() {
        let mut script = Scripted::new(vec![None]);
        let mut delays = Vec::new();
        let generation = fetch_generation(
            &options(Some("gen-1"), 0, &[10, 20], "k"),
            |r| script.fetch(r),
            |ms| delays.push(ms),
        );
        assert_eq!(generation, None);
        assert_eq!(script.calls.len(), 3);
        assert_eq!(delays, [10, 20]);
    }

    #[test]
    fn returns_none_when_the_body_is_not_a_generation() {
        let mut script = Scripted::new(vec![Some(json!({"data": "nope"}))]);
        let generation = fetch_generation(
            &options(Some("gen-1"), 0, &[], "k"),
            |r| script.fetch(r),
            |_| {},
        );
        assert_eq!(generation, None);
    }

    #[test]
    fn returns_none_when_the_network_fails() {
        let generation = fetch_generation(&options(Some("gen-1"), 0, &[1], "k"), |_| None, |_| {});
        assert_eq!(generation, None);
    }

    #[test]
    fn does_not_call_the_endpoint_without_a_generation_id() {
        let mut script = Scripted::new(vec![Some(generation_body())]);
        let generation = fetch_generation(
            &options(None, 30_000, &[], "k"),
            |r| script.fetch(r),
            |_| {},
        );
        assert_eq!(generation, None);
        assert!(script.calls.is_empty());
    }

    #[test]
    fn accepts_nulls_and_rejects_wrong_types() {
        let nulls = json!({"data": {"provider_name": null, "total_cost": null}});
        assert!(parse_generation(&nulls).is_some());
        let null_id = json!({"data": {"id": null}});
        assert!(parse_generation(&null_id).is_none());
        let bad = json!({"data": {"latency": "fast"}});
        assert!(parse_generation(&bad).is_none());
    }

    #[test]
    fn defaults_match_the_original_backoff() {
        assert_eq!(DEFAULT_GENERATION_INITIAL_DELAY_MS, 30_000);
        assert_eq!(default_generation_delays_ms(), vec![10_000; 24]);
    }
}
