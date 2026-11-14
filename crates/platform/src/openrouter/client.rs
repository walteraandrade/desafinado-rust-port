use std::io::{BufRead, BufReader};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::Value;
use ureq::Agent;
use ureq::BodyReader;

use super::agent::{AgentOptions, build_agent};
use super::error::{ClientError, classify_io, classify_status, classify_transport};
use super::generation::{GenerationSchedule, fetch_generation};
use super::sse::{SseLine, parse_sse_line};
use super::types::{ChatCompletion, ChatRequest, StreamChunk, Usage};
use super::{PLACEHOLDER_KEY, base_url_from_env};
use crate::records::OpenRouterGeneration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub min_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 1,
            min_delay: Duration::from_millis(1000),
        }
    }
}

pub struct OpenRouterClient {
    agent: Agent,
    base_url: String,
    api_key: Option<String>,
    retry: RetryPolicy,
    timeout: Duration,
    stall: Option<Duration>,
}

struct Sent {
    status: u16,
    retry_after: Option<Duration>,
    body: ureq::Body,
}

impl OpenRouterClient {
    pub fn new(
        base_url: String,
        api_key: Option<String>,
        options: &AgentOptions,
    ) -> Result<Self, ClientError> {
        let agent = build_agent(options).map_err(|e| ClientError::Network {
            message: e.to_string(),
        })?;
        Ok(Self {
            agent,
            base_url: base_url.trim_end_matches('/').to_owned(),
            api_key: api_key.filter(|key| !key.is_empty()),
            retry: RetryPolicy::default(),
            timeout: DEFAULT_TIMEOUT,
            stall: None,
        })
    }

    pub fn from_env(api_key: Option<String>) -> Result<Self, ClientError> {
        let read = |name: &str| std::env::var(name).ok();
        Self::new(
            base_url_from_env(&read),
            api_key,
            &AgentOptions::from_env(&read),
        )
    }

    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_stall(mut self, stall: Option<Duration>) -> Self {
        self.stall = stall;
        self
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn authorization(&self) -> String {
        format!(
            "Bearer {}",
            self.api_key.as_deref().unwrap_or(PLACEHOLDER_KEY)
        )
    }

    pub fn try_get_text(&self, url: &str) -> Result<(u16, String), ClientError> {
        let mut response = self
            .agent
            .get(url)
            .header("Authorization", self.authorization())
            .config()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .call()
            .map_err(|e| classify_transport(&e))?;
        let status = response.status().as_u16();
        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|e| classify_transport(&e))?;
        Ok((status, text))
    }

    pub fn get_text(&self, url: &str) -> Option<(u16, String)> {
        self.try_get_text(url).ok()
    }

    fn post(&self, body: &Value, stream: bool) -> Result<Sent, ClientError> {
        let mut config = self
            .agent
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", self.authorization())
            .header("Content-Type", "application/json")
            .header(
                "Accept",
                if stream {
                    "text/event-stream"
                } else {
                    "application/json"
                },
            )
            .header("X-Title", "Desafinado")
            .config()
            .http_status_as_error(false)
            .timeout_global(Some(self.timeout));
        if stream {
            config = config.timeout_per_read(self.stall);
        }
        let response = config
            .build()
            .send_json(body)
            .map_err(|e| classify_transport(&e))?;
        let status = response.status().as_u16();
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        Ok(Sent {
            status,
            retry_after,
            body: response.into_body(),
        })
    }

    fn failure_from(&self, sent: Sent) -> ClientError {
        let status = sent.status;
        let text = sent
            .body
            .into_with_config()
            .lossy_utf8(true)
            .read_to_string()
            .unwrap_or_default();
        classify_status(status, &text)
    }

    pub fn chat_completion(&self, request: &ChatRequest) -> Result<ChatCompletion, ClientError> {
        let body = request.body(false);
        let mut attempt = 0;
        loop {
            let outcome = self.complete_once(&body);
            match outcome {
                Err((error, retry_after))
                    if error.is_retryable_transport_error() && attempt < self.retry.max_retries =>
                {
                    attempt += 1;
                    let delay = retry_after
                        .unwrap_or_default()
                        .max(self.retry.min_delay)
                        .min(MAX_RETRY_DELAY);
                    std::thread::sleep(delay);
                }
                Err((error, _)) => return Err(error),
                Ok(completion) => return Ok(completion),
            }
        }
    }

    fn complete_once(
        &self,
        body: &Value,
    ) -> Result<ChatCompletion, (ClientError, Option<Duration>)> {
        let sent = self.post(body, false).map_err(|e| (e, None))?;
        let (status, retry_after) = (sent.status, sent.retry_after);
        if !(200..300).contains(&status) {
            return Err((self.failure_from(sent), retry_after));
        }
        let text = sent
            .body
            .into_with_config()
            .lossy_utf8(true)
            .limit(64 * 1024 * 1024)
            .read_to_string()
            .map_err(|e| (classify_transport(&e), None))?;
        parse_completion(&text).map_err(|e| (e, retry_after))
    }

    pub fn chat_stream(
        &self,
        request: &ChatRequest,
        abort: Option<Arc<AtomicBool>>,
    ) -> Result<ChatStream, ClientError> {
        let body = request.body(true);
        let mut attempt = 0;
        loop {
            let sent = self.post(&body, true);
            let sent = match sent {
                Ok(sent) if (200..300).contains(&sent.status) => sent,
                Ok(sent) => {
                    let retry_after = sent.retry_after;
                    let error = self.failure_from(sent);
                    if error.is_retryable_transport_error() && attempt < self.retry.max_retries {
                        attempt += 1;
                        std::thread::sleep(
                            retry_after
                                .unwrap_or_default()
                                .max(self.retry.min_delay)
                                .min(MAX_RETRY_DELAY),
                        );
                        continue;
                    }
                    return Err(error);
                }
                Err(error) => {
                    if error.is_retryable_transport_error() && attempt < self.retry.max_retries {
                        attempt += 1;
                        std::thread::sleep(self.retry.min_delay);
                        continue;
                    }
                    return Err(error);
                }
            };
            return Ok(ChatStream {
                reader: BufReader::new(sent.body.into_reader()),
                abort,
                finished: false,
                saw_finish: false,
            });
        }
    }

    pub fn chat_stream_collect(
        &self,
        request: &ChatRequest,
        abort: Option<Arc<AtomicBool>>,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<ChatCompletion, ClientError> {
        let mut completion = ChatCompletion::default();
        for chunk in self.chat_stream(request, abort)? {
            let chunk = chunk?;
            completion.id = chunk.id.or(completion.id);
            completion.model = chunk.model.or(completion.model);
            completion.provider = chunk.provider.or(completion.provider);
            completion.finish_reason = chunk.finish_reason.or(completion.finish_reason);
            if let Some(usage) = chunk.usage {
                completion.usage = usage;
            }
            if let Some(delta) = chunk.delta {
                on_delta(&delta);
                completion.content.push_str(&delta);
            }
        }
        Ok(completion)
    }

    pub fn fetch_generation(
        &self,
        id: Option<&str>,
        schedule: &GenerationSchedule,
    ) -> Option<OpenRouterGeneration> {
        fetch_generation(self, id, schedule)
    }
}

fn parse_completion(text: &str) -> Result<ChatCompletion, ClientError> {
    let value: Value = serde_json::from_str(text).map_err(|_| classify_status(200, text))?;
    if value.get("error").is_some() {
        return Err(classify_status(200, text));
    }
    let choice = value.pointer("/choices/0");
    let content = choice
        .and_then(|c| c.pointer("/message/content"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| ClientError::InvalidOutput {
            message: "The model answered without text".into(),
        })?;
    Ok(ChatCompletion {
        id: value.get("id").and_then(Value::as_str).map(str::to_owned),
        model: value
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_owned),
        provider: value
            .get("provider")
            .and_then(Value::as_str)
            .map(str::to_owned),
        content,
        finish_reason: choice
            .and_then(|c| c.get("finish_reason"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        usage: value.get("usage").map(Usage::from_json).unwrap_or_default(),
    })
}

pub struct ChatStream {
    reader: BufReader<BodyReader<'static>>,
    abort: Option<Arc<AtomicBool>>,
    finished: bool,
    saw_finish: bool,
}

fn parse_chunk(data: &str) -> Result<Option<StreamChunk>, ClientError> {
    let value: Value = serde_json::from_str(data).map_err(|_| ClientError::Unknown {
        message: "the answer stream broke off. Retry.".into(),
    })?;
    if value.get("error").is_some() {
        return Err(classify_status(200, data));
    }
    let choice = value.pointer("/choices/0");
    let text = |pointer: &str| {
        choice
            .and_then(|c| c.pointer(pointer))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let string = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
    Ok(Some(StreamChunk {
        id: string("id"),
        model: string("model"),
        provider: string("provider"),
        delta: text("/delta/content"),
        reasoning_delta: text("/delta/reasoning"),
        finish_reason: choice
            .and_then(|c| c.get("finish_reason"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        usage: value
            .get("usage")
            .filter(|u| !u.is_null())
            .map(Usage::from_json),
    }))
}

impl Iterator for ChatStream {
    type Item = Result<StreamChunk, ClientError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        loop {
            if self
                .abort
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::SeqCst))
            {
                self.finished = true;
                return Some(Err(ClientError::Aborted {
                    message: "The request was cancelled".into(),
                }));
            }
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Err(error) => {
                    self.finished = true;
                    return Some(Err(classify_io(&error)));
                }
                Ok(0) => {
                    self.finished = true;
                    return if self.saw_finish {
                        None
                    } else {
                        Some(Err(ClientError::Unknown {
                            message: "the answer stream broke off. Retry.".into(),
                        }))
                    };
                }
                Ok(_) => match parse_sse_line(&line) {
                    SseLine::Ignored => {}
                    SseLine::Done => {
                        self.finished = true;
                        return None;
                    }
                    SseLine::Data(data) => match parse_chunk(data) {
                        Err(error) => {
                            self.finished = true;
                            return Some(Err(error));
                        }
                        Ok(None) => {}
                        Ok(Some(chunk)) => {
                            self.saw_finish |= chunk.finish_reason.is_some();
                            return Some(Ok(chunk));
                        }
                    },
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openrouter::test_server::{Reply, TestServer};
    use crate::openrouter::{ChatMessage, JsonSchemaFormat};
    use serde_json::json;

    const COMPLETION: &str = r#"{"id":"gen-abc","model":"vendor/model-served","provider":"Acme","choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"{\"ok\":true}"}}],"usage":{"prompt_tokens":12,"completion_tokens":4,"cost":0.00031,"completion_tokens_details":{"reasoning_tokens":1},"prompt_tokens_details":{"cached_tokens":2}}}"#;

    fn client(server: &TestServer, key: Option<&str>) -> OpenRouterClient {
        OpenRouterClient::new(
            server.base_url(),
            key.map(str::to_owned),
            &AgentOptions::direct(),
        )
        .unwrap()
        .with_retry(RetryPolicy {
            max_retries: 1,
            min_delay: Duration::from_millis(5),
        })
    }

    fn request() -> ChatRequest {
        let mut request = ChatRequest::new("vendor/model", vec![ChatMessage::user("hi")]);
        request.response_format = Some(JsonSchemaFormat {
            name: "answer".into(),
            schema: json!({"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"]}),
        });
        request
    }

    #[test]
    fn posts_the_json_schema_request_and_parses_the_completion() {
        let server = TestServer::start(vec![Reply::json(200, COMPLETION)]);
        let completion = client(&server, Some("sk-key"))
            .chat_completion(&request())
            .unwrap();
        assert_eq!(completion.content, r#"{"ok":true}"#);
        assert_eq!(completion.id.as_deref(), Some("gen-abc"));
        assert_eq!(completion.model.as_deref(), Some("vendor/model-served"));
        assert_eq!(completion.provider.as_deref(), Some("Acme"));
        assert_eq!(completion.finish_reason.as_deref(), Some("stop"));
        assert_eq!(completion.usage.prompt_tokens, Some(12));
        assert_eq!(completion.usage.cost, Some(0.00031));
        assert_eq!(completion.usage.reasoning_tokens, Some(1));
        assert_eq!(completion.usage.cached_tokens, Some(2));
        let sent = &server.requests()[0];
        assert_eq!(sent.method, "POST");
        assert_eq!(sent.path, "/api/v1/chat/completions");
        assert_eq!(sent.headers["authorization"], "Bearer sk-key");
        assert_eq!(sent.headers["content-type"], "application/json");
        let body: Value = serde_json::from_str(&sent.body).unwrap();
        assert_eq!(body["response_format"]["json_schema"]["name"], "answer");
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn sends_a_placeholder_key_when_none_is_stored() {
        let server = TestServer::start(vec![Reply::json(200, COMPLETION)]);
        client(&server, None).chat_completion(&request()).unwrap();
        assert_eq!(
            server.requests()[0].headers["authorization"],
            "Bearer placeholder"
        );
        let server = TestServer::start(vec![Reply::json(200, COMPLETION)]);
        client(&server, Some(""))
            .chat_completion(&request())
            .unwrap();
        assert_eq!(
            server.requests()[0].headers["authorization"],
            "Bearer placeholder"
        );
    }

    #[test]
    fn classifies_402_and_429_without_retrying_402() {
        let server = TestServer::start(vec![Reply::json(
            402,
            r#"{"error":{"message":"Insufficient credits","code":402}}"#,
        )]);
        let error = client(&server, None)
            .chat_completion(&request())
            .unwrap_err();
        assert_eq!(
            error,
            ClientError::OutOfCredit {
                message: "Insufficient credits".into()
            }
        );
        assert_eq!(server.requests().len(), 1);
    }

    #[test]
    fn retries_a_rate_limit_once_and_then_succeeds() {
        let server = TestServer::start(vec![
            Reply::json(429, r#"{"error":{"message":"slow down","code":429}}"#),
            Reply::json(200, COMPLETION),
        ]);
        assert!(client(&server, None).chat_completion(&request()).is_ok());
        assert_eq!(server.requests().len(), 2);
    }

    #[test]
    fn gives_up_after_the_retries_with_the_last_error() {
        let rate = || Reply::json(503, r#"{"error":{"message":"down","code":503}}"#);
        let server = TestServer::start(vec![rate(), rate(), rate()]);
        let error = client(&server, None)
            .chat_completion(&request())
            .unwrap_err();
        assert!(matches!(error, ClientError::Api { status: 503, .. }));
        assert_eq!(server.requests().len(), 2);
    }

    #[test]
    fn reads_an_error_payload_sent_with_http_200() {
        let server = TestServer::start(vec![Reply::json(
            200,
            r#"{"error":{"message":"no credit","code":402}}"#,
        )]);
        let error = client(&server, None)
            .chat_completion(&request())
            .unwrap_err();
        assert!(matches!(error, ClientError::OutOfCredit { .. }));
    }

    #[test]
    fn reports_a_completion_without_text_as_invalid_output() {
        let server = TestServer::start(vec![Reply::json(
            200,
            r#"{"id":"g","choices":[{"message":{"content":null}}]}"#,
        )]);
        let error = client(&server, None)
            .chat_completion(&request())
            .unwrap_err();
        assert!(error.is_retryable_output_error());
    }

    #[test]
    fn reports_a_refused_connection_as_a_network_error() {
        let dead = OpenRouterClient::new(
            "http://127.0.0.1:1/api/v1".into(),
            None,
            &AgentOptions::direct(),
        )
        .unwrap()
        .with_retry(RetryPolicy {
            max_retries: 0,
            min_delay: Duration::ZERO,
        });
        assert!(matches!(
            dead.chat_completion(&request()).unwrap_err(),
            ClientError::Network { .. }
        ));
    }

    #[test]
    fn reports_a_slow_answer_as_a_timeout() {
        let server = TestServer::start(vec![
            Reply::json(200, COMPLETION).delayed(Duration::from_millis(600)),
        ]);
        let slow = client(&server, None)
            .with_timeout(Duration::from_millis(150))
            .with_retry(RetryPolicy {
                max_retries: 0,
                min_delay: Duration::ZERO,
            });
        assert!(matches!(
            slow.chat_completion(&request()).unwrap_err(),
            ClientError::Timeout { .. }
        ));
    }

    fn data(value: Value) -> String {
        format!("data: {value}\n\n")
    }

    #[test]
    fn streams_deltas_the_finish_reason_usage_and_the_generation_id() {
        let first = data(
            json!({"id":"gen-s","model":"m-served","provider":"Acme","choices":[{"delta":{"role":"assistant","content":"Hel"}}]}),
        );
        let second = data(json!({"id":"gen-s","choices":[{"delta":{"content":"lo"}}]}));
        let third =
            data(json!({"id":"gen-s","choices":[{"delta":{"content":""},"finish_reason":"stop"}]}));
        let usage = data(
            json!({"id":"gen-s","choices":[],"usage":{"prompt_tokens":8,"completion_tokens":2,"cost":0.0002}}),
        );
        let server = TestServer::start(vec![Reply::sse(&[
            ": OPENROUTER PROCESSING\n\n",
            &first,
            &second,
            &third,
            &usage,
            "data: [DONE]\n\n",
        ])]);
        let chunks: Vec<StreamChunk> = client(&server, Some("k"))
            .chat_stream(&request(), None)
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let deltas: Vec<&str> = chunks.iter().filter_map(|c| c.delta.as_deref()).collect();
        assert_eq!(deltas, ["Hel", "lo"]);
        assert_eq!(chunks[0].id.as_deref(), Some("gen-s"));
        assert_eq!(chunks[0].provider.as_deref(), Some("Acme"));
        assert_eq!(
            chunks
                .iter()
                .find_map(|c| c.finish_reason.clone())
                .as_deref(),
            Some("stop")
        );
        let usage = chunks.iter().find_map(|c| c.usage.clone()).unwrap();
        assert_eq!((usage.prompt_tokens, usage.cost), (Some(8), Some(0.0002)));
        let body: Value = serde_json::from_str(&server.requests()[0].body).unwrap();
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
    }

    #[test]
    fn collects_a_stream_into_a_completion_calling_back_on_each_delta() {
        let a = data(json!({"id":"g","model":"m","choices":[{"delta":{"content":"a"}}]}));
        let b =
            data(json!({"id":"g","choices":[{"delta":{"content":"b"},"finish_reason":"length"}]}));
        let u = data(json!({"choices":[],"usage":{"prompt_tokens":1,"completion_tokens":2}}));
        let server = TestServer::start(vec![Reply::sse(&[&a, &b, &u, "data: [DONE]\n\n"])]);
        let mut seen = Vec::new();
        let completion = client(&server, None)
            .chat_stream_collect(&request(), None, &mut |d| seen.push(d.to_owned()))
            .unwrap();
        assert_eq!(seen, ["a", "b"]);
        assert_eq!(completion.content, "ab");
        assert_eq!(completion.id.as_deref(), Some("g"));
        assert_eq!(completion.finish_reason.as_deref(), Some("length"));
        assert_eq!(completion.usage.completion_tokens, Some(2));
    }

    #[test]
    fn reads_chunks_split_across_network_writes() {
        let line = data(
            json!({"id":"g","choices":[{"delta":{"content":"split"},"finish_reason":"stop"}]}),
        );
        let (head, tail) = line.split_at(20);
        let server = TestServer::start(vec![Reply::sse(&[head, tail, "data: [DONE]\n\n"])]);
        let chunks: Vec<_> = client(&server, None)
            .chat_stream(&request(), None)
            .unwrap()
            .collect();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].as_ref().unwrap().delta.as_deref(), Some("split"));
    }

    #[test]
    fn a_stream_that_ends_without_a_finish_is_an_error() {
        let a = data(json!({"id":"g","choices":[{"delta":{"content":"a"}}]}));
        let server = TestServer::start(vec![Reply::sse(&[&a])]);
        let results: Vec<_> = client(&server, None)
            .chat_stream(&request(), None)
            .unwrap()
            .collect();
        assert!(results[0].is_ok());
        assert!(matches!(results[1], Err(ClientError::Unknown { .. })));
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn an_error_inside_the_stream_is_classified_and_ends_it() {
        let a = data(json!({"id":"g","choices":[{"delta":{"content":"a"}}]}));
        let err = data(json!({"error":{"message":"upstream rate limit","code":429}}));
        let server = TestServer::start(vec![Reply::sse(&[&a, &err, "data: [DONE]\n\n"])]);
        let results: Vec<_> = client(&server, None)
            .chat_stream(&request(), None)
            .unwrap()
            .collect();
        assert_eq!(results.len(), 2);
        assert!(matches!(results[1], Err(ClientError::RateLimited { .. })));
    }

    #[test]
    fn a_failing_status_before_the_stream_is_an_error_not_a_stream() {
        let server = TestServer::start(vec![Reply::json(
            401,
            r#"{"error":{"message":"bad key","code":401}}"#,
        )]);
        let error = client(&server, None)
            .chat_stream(&request(), None)
            .err()
            .unwrap();
        assert_eq!(
            error,
            ClientError::Api {
                status: 401,
                message: "bad key".into()
            }
        );
    }

    #[test]
    fn stops_with_aborted_when_the_flag_is_set() {
        let a = data(json!({"id":"g","choices":[{"delta":{"content":"a"}}]}));
        let b =
            data(json!({"id":"g","choices":[{"delta":{"content":"b"},"finish_reason":"stop"}]}));
        let server = TestServer::start(vec![Reply::sse(&[&a, &b, "data: [DONE]\n\n"])]);
        let flag = Arc::new(AtomicBool::new(false));
        let mut stream = client(&server, None)
            .chat_stream(&request(), Some(flag.clone()))
            .unwrap();
        assert!(stream.next().unwrap().is_ok());
        flag.store(true, Ordering::SeqCst);
        assert!(matches!(
            stream.next().unwrap(),
            Err(ClientError::Aborted { .. })
        ));
        assert!(stream.next().is_none());
    }

    #[test]
    fn a_stalled_stream_times_out() {
        let a = data(json!({"id":"g","choices":[{"delta":{"content":"a"}}]}));
        let mut reply = Reply::sse(&[&a, "data: [DONE]\n\n"]);
        reply.chunks[1].0 = Duration::from_millis(700);
        let server = TestServer::start(vec![reply]);
        let stalled = client(&server, None).with_stall(Some(Duration::from_millis(150)));
        let results: Vec<_> = stalled.chat_stream(&request(), None).unwrap().collect();
        assert!(results[0].is_ok());
        assert!(
            matches!(results[1], Err(ClientError::Timeout { .. })),
            "{:?}",
            results[1]
        );
    }

    #[test]
    fn goes_through_an_http_proxy_with_connect() {
        let target = TestServer::start(vec![Reply::json(200, COMPLETION)]);
        let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy_addr = proxy.local_addr().unwrap();
        let target_addr = target.addr;
        let seen = Arc::new(std::sync::Mutex::new(String::new()));
        let log = seen.clone();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut inbound, _) = proxy.accept().unwrap();
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                inbound.read_exact(&mut byte).unwrap();
                head.push(byte[0]);
            }
            *log.lock().unwrap() = String::from_utf8_lossy(&head).into_owned();
            let mut outbound = std::net::TcpStream::connect(target_addr).unwrap();
            inbound
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .unwrap();
            let (mut a, mut b) = (inbound.try_clone().unwrap(), outbound.try_clone().unwrap());
            let up = std::thread::spawn(move || {
                let _ = std::io::copy(&mut a, &mut b);
            });
            let _ = std::io::copy(&mut outbound, &mut inbound);
            let _ = up.join();
        });
        let options = AgentOptions {
            proxy_from_env: false,
            proxy_url: Some(format!("http://{proxy_addr}")),
            ssl_cert_file: None,
        };
        let proxied =
            OpenRouterClient::new("http://openrouter.invalid/api/v1".into(), None, &options)
                .unwrap();
        let completion = proxied.chat_completion(&request()).unwrap();
        assert_eq!(completion.content, r#"{"ok":true}"#);
        assert!(
            seen.lock()
                .unwrap()
                .starts_with("CONNECT openrouter.invalid:80")
        );
    }

    #[test]
    #[ignore = "uses the real network through HTTPS_PROXY and SSL_CERT_FILE"]
    fn live_reads_the_public_model_list_through_the_environment() {
        let client = OpenRouterClient::from_env(None).unwrap();
        let (status, text) = client
            .try_get_text(&format!("{}/models", client.base_url()))
            .expect("the request went through");
        assert_eq!(status, 200);
        assert!(text.contains("\"data\""));
    }
}
