use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::Value;
use ureq::Agent;
use ureq::BodyReader;

use super::agent::{AgentOptions, build_agent};
use super::error::{
    ClientError, classify_io, classify_status, classify_transport, is_retryable_transport_error,
};
use super::generation::{GenerationSchedule, fetch_generation};
use super::{PLACEHOLDER_KEY, base_url_from_env};
use desafinado_ai::chat_stream::{ChatStreamEvent, ChatStreamParser, observation_from_response};
use desafinado_ai::records::{CallObservation, OpenRouterGeneration};

const DEFAULT_TIMEOUT: Duration = Duration::from_mins(10);
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
            min_delay: Duration::from_secs(1),
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

    #[must_use]
    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[must_use]
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

    pub fn chat_completion(&self, body: &Value) -> Result<ChatOutput, ClientError> {
        let body = with_stream(body, false);
        let model = requested_model(&body);
        let mut attempt = 0;
        loop {
            match self.complete_once(&model, &body) {
                Err((error, retry_after))
                    if is_retryable_transport_error(&error) && attempt < self.retry.max_retries =>
                {
                    attempt += 1;
                    std::thread::sleep(retry_delay(self.retry, retry_after));
                }
                Err((error, _)) => return Err(error),
                Ok(output) => return Ok(output),
            }
        }
    }

    fn complete_once(
        &self,
        model: &str,
        body: &Value,
    ) -> Result<ChatOutput, (ClientError, Option<Duration>)> {
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
        parse_completion(model, &text).map_err(|e| (e, retry_after))
    }

    pub fn chat_stream(
        &self,
        body: &Value,
        abort: Option<Arc<AtomicBool>>,
    ) -> Result<ChatStream, ClientError> {
        let body = with_stream(body, true);
        let mut attempt = 0;
        loop {
            let sent = match self.post(&body, true) {
                Ok(sent) if (200..300).contains(&sent.status) => sent,
                Ok(sent) => {
                    let retry_after = sent.retry_after;
                    let error = self.failure_from(sent);
                    if is_retryable_transport_error(&error) && attempt < self.retry.max_retries {
                        attempt += 1;
                        std::thread::sleep(retry_delay(self.retry, retry_after));
                        continue;
                    }
                    return Err(error);
                }
                Err(error) => {
                    if is_retryable_transport_error(&error) && attempt < self.retry.max_retries {
                        attempt += 1;
                        std::thread::sleep(self.retry.min_delay);
                        continue;
                    }
                    return Err(error);
                }
            };
            return Ok(ChatStream {
                reader: BufReader::new(sent.body.into_reader()),
                parser: ChatStreamParser::new(&requested_model(&body)),
                pending: VecDeque::new(),
                abort,
                finished: false,
                at_end: false,
            });
        }
    }

    pub fn chat_stream_collect(
        &self,
        body: &Value,
        abort: Option<Arc<AtomicBool>>,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<ChatOutput, ClientError> {
        let mut stream = self.chat_stream(body, abort)?;
        let mut content = String::new();
        for delta in stream.by_ref() {
            let delta = delta?;
            on_delta(&delta);
            content.push_str(&delta);
        }
        Ok(ChatOutput {
            content,
            observation: stream.observation(),
        })
    }

    pub fn fetch_generation(
        &self,
        id: Option<&str>,
        schedule: &GenerationSchedule,
    ) -> Option<OpenRouterGeneration> {
        fetch_generation(self, id, schedule)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatOutput {
    pub content: String,
    pub observation: CallObservation,
}

fn with_stream(body: &Value, stream: bool) -> Value {
    let mut body = body.clone();
    if let Some(object) = body.as_object_mut() {
        object.insert("stream".into(), Value::Bool(stream));
    }
    body
}

fn requested_model(body: &Value) -> String {
    body.get("model")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn retry_delay(retry: RetryPolicy, retry_after: Option<Duration>) -> Duration {
    retry_after
        .unwrap_or_default()
        .max(retry.min_delay)
        .min(MAX_RETRY_DELAY)
}

fn parse_completion(model: &str, text: &str) -> Result<ChatOutput, ClientError> {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Err(classify_status(200, text));
    };
    if value.get("error").is_some() {
        return Err(classify_status(200, text));
    }
    if value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .is_none()
    {
        return Err(ClientError::InvalidOutput {
            message: "The model answered without text".into(),
        });
    }
    let (content, observation) = observation_from_response(model, &value);
    Ok(ChatOutput {
        content,
        observation,
    })
}

pub struct ChatStream {
    reader: BufReader<BodyReader<'static>>,
    parser: ChatStreamParser,
    pending: VecDeque<ChatStreamEvent>,
    abort: Option<Arc<AtomicBool>>,
    finished: bool,
    at_end: bool,
}

impl ChatStream {
    pub fn observation(&self) -> CallObservation {
        self.parser.observation()
    }

    fn broke_off(&self) -> Option<ClientError> {
        (!self.parser.is_done() && self.parser.observation().finish_reason.is_none()).then(|| {
            ClientError::Unknown {
                message: "the answer stream broke off. Retry.".into(),
            }
        })
    }
}

impl Iterator for ChatStream {
    type Item = Result<String, ClientError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        loop {
            match self.pending.pop_front() {
                Some(ChatStreamEvent::Delta(text)) => return Some(Ok(text)),
                Some(ChatStreamEvent::Failed(error)) => {
                    self.finished = true;
                    return Some(Err(error));
                }
                Some(ChatStreamEvent::Done) => {
                    self.finished = true;
                    return None;
                }
                None => {}
            }
            if self.at_end {
                self.finished = true;
                return self.broke_off().map(Err);
            }
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
                    self.at_end = true;
                    self.pending.extend(self.parser.finish());
                }
                Ok(_) => self.pending.extend(self.parser.feed(&line)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openrouter::test_server::{Reply, TestServer};
    use desafinado_core::log_err::LogErr;
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

    fn request() -> Value {
        json!({
            "model": "vendor/model",
            "messages": [{"role": "user", "content": "hi"}],
            "response_format": {"type": "json_schema", "json_schema": {"name": "answer"}},
            "stream": true,
            "usage": {"include": true}
        })
    }

    #[test]
    fn posts_the_json_schema_request_and_parses_the_completion() {
        let server = TestServer::start(vec![Reply::json(200, COMPLETION)]);
        let completion = client(&server, Some("sk-key"))
            .chat_completion(&request())
            .unwrap();
        let observation = &completion.observation;
        assert_eq!(completion.content, r#"{"ok":true}"#);
        assert_eq!(observation.requested_model, "vendor/model");
        assert_eq!(observation.response_id.as_deref(), Some("gen-abc"));
        assert_eq!(
            observation.served_model.as_deref(),
            Some("vendor/model-served")
        );
        assert_eq!(observation.provider.as_deref(), Some("Acme"));
        assert_eq!(observation.finish_reason.as_deref(), Some("stop"));
        assert_eq!(observation.input_tokens, Some(12.0));
        assert_eq!(observation.cost_usd, Some(0.00031));
        assert_eq!(observation.reasoning_tokens, Some(1.0));
        assert_eq!(observation.cached_tokens, Some(2.0));
        let sent = &server.requests()[0];
        assert_eq!(sent.method, "POST");
        assert_eq!(sent.path, "/api/v1/chat/completions");
        assert_eq!(sent.headers["authorization"], "Bearer sk-key");
        assert_eq!(sent.headers["content-type"], "application/json");
        let body: Value = serde_json::from_str(&sent.body).unwrap();
        assert_eq!(body["response_format"]["json_schema"]["name"], "answer");
        assert_eq!(body["usage"]["include"], true);
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
        assert!(matches!(error, ClientError::InvalidOutput { .. }));
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
        let mut stream = client(&server, Some("k"))
            .chat_stream(&request(), None)
            .unwrap();
        let deltas: Vec<String> = stream.by_ref().map(Result::unwrap).collect();
        assert_eq!(deltas, ["Hel", "lo"]);
        let observation = stream.observation();
        assert_eq!(observation.response_id.as_deref(), Some("gen-s"));
        assert_eq!(observation.served_model.as_deref(), Some("m-served"));
        assert_eq!(observation.provider.as_deref(), Some("Acme"));
        assert_eq!(observation.finish_reason.as_deref(), Some("stop"));
        assert_eq!(observation.requested_model, "vendor/model");
        assert_eq!(
            (observation.input_tokens, observation.cost_usd),
            (Some(8.0), Some(0.0002))
        );
        let body: Value = serde_json::from_str(&server.requests()[0].body).unwrap();
        assert_eq!(body["stream"], true);
        assert_eq!(body["usage"]["include"], true);
    }

    #[test]
    fn collects_a_stream_into_a_completion_calling_back_on_each_delta() {
        let a = data(json!({"id":"g","model":"m","choices":[{"delta":{"content":"a"}}]}));
        let b =
            data(json!({"id":"g","choices":[{"delta":{"content":"b"},"finish_reason":"length"}]}));
        let u = data(json!({"choices":[],"usage":{"prompt_tokens":1,"completion_tokens":2}}));
        let server = TestServer::start(vec![Reply::sse(&[&a, &b, &u, "data: [DONE]\n\n"])]);
        let mut seen = Vec::new();
        let output = client(&server, None)
            .chat_stream_collect(&request(), None, &mut |d| seen.push(d.to_owned()))
            .unwrap();
        assert_eq!(seen, ["a", "b"]);
        assert_eq!(output.content, "ab");
        assert_eq!(output.observation.response_id.as_deref(), Some("g"));
        assert_eq!(output.observation.finish_reason.as_deref(), Some("length"));
        assert_eq!(output.observation.output_tokens, Some(2.0));
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
        assert_eq!(chunks[0].as_deref().unwrap(), "split");
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
            .chat_stream(&request(), Some(Arc::clone(&flag)))
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
        let log = Arc::clone(&seen);
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
                std::io::copy(&mut a, &mut b).log_warn();
            });
            std::io::copy(&mut outbound, &mut inbound).log_warn();
            up.join().unwrap();
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
