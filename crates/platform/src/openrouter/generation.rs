use std::time::Duration;

use serde::Deserialize;

use super::client::OpenRouterClient;
use crate::records::OpenRouterGeneration;

#[derive(Debug, Clone)]
pub struct GenerationSchedule {
    pub initial_delay: Duration,
    pub delays: Vec<Duration>,
}

impl Default for GenerationSchedule {
    fn default() -> Self {
        Self {
            initial_delay: Duration::ZERO,
            delays: [500, 1000, 1500, 2000, 2000, 3000]
                .map(Duration::from_millis)
                .to_vec(),
        }
    }
}

#[derive(Deserialize)]
struct Envelope {
    data: OpenRouterGeneration,
}

fn encode(id: &str) -> String {
    id.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

pub fn fetch_generation(
    client: &OpenRouterClient,
    id: Option<&str>,
    schedule: &GenerationSchedule,
) -> Option<OpenRouterGeneration> {
    let id = id?;
    let url = format!("{}/generation?id={}", client.base_url(), encode(id));
    if !schedule.initial_delay.is_zero() {
        std::thread::sleep(schedule.initial_delay);
    }
    for attempt in 0..=schedule.delays.len() {
        if let Some(text) = client
            .get_text(&url)
            .filter(|(status, _)| *status == 200)
            .map(|(_, text)| text)
            && let Ok(envelope) = serde_json::from_str::<Envelope>(&text)
        {
            return Some(envelope.data);
        }
        if let Some(delay) = schedule.delays.get(attempt) {
            std::thread::sleep(*delay);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openrouter::test_server::{Reply, TestServer};
    use crate::openrouter::{AgentOptions, OpenRouterClient};

    const BODY: &str = r#"{"data":{"id":"gen-1","model":"vendor/exact","provider_name":"Acme","latency":400,"generation_time":350,"finish_reason":"stop","tokens_prompt":10,"tokens_completion":5,"native_tokens_reasoning":2,"native_tokens_cached":0,"total_cost":0.0007}}"#;

    fn fast(count: usize) -> GenerationSchedule {
        GenerationSchedule {
            initial_delay: Duration::ZERO,
            delays: vec![Duration::from_millis(5); count],
        }
    }

    fn client(server: &TestServer, key: Option<&str>) -> OpenRouterClient {
        OpenRouterClient::new(
            server.base_url(),
            key.map(str::to_owned),
            &AgentOptions::direct(),
        )
        .unwrap()
    }

    #[test]
    fn requests_the_generation_by_id_with_the_authorization_header() {
        let server = TestServer::start(vec![Reply::json(200, BODY)]);
        let generation =
            fetch_generation(&client(&server, Some("sk-1")), Some("gen 1/ä"), &fast(0)).unwrap();
        let request = &server.requests()[0];
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/api/v1/generation?id=gen%201%2F%C3%A4");
        assert_eq!(request.headers["authorization"], "Bearer sk-1");
        assert_eq!(generation.model.as_deref(), Some("vendor/exact"));
        assert_eq!(generation.total_cost, Some(0.0007));
        assert_eq!(generation.native_tokens_reasoning, Some(2));
    }

    #[test]
    fn sends_a_placeholder_key_when_none_is_stored() {
        let server = TestServer::start(vec![Reply::json(200, BODY)]);
        fetch_generation(&client(&server, None), Some("gen-1"), &fast(0)).unwrap();
        assert_eq!(
            server.requests()[0].headers["authorization"],
            "Bearer placeholder"
        );
    }

    #[test]
    fn retries_a_404_with_backoff_until_the_generation_shows_up() {
        let not_found = || Reply::json(404, r#"{"error":{"message":"not found","code":404}}"#);
        let server = TestServer::start(vec![not_found(), not_found(), Reply::json(200, BODY)]);
        let generation = fetch_generation(&client(&server, None), Some("gen-1"), &fast(5));
        assert!(generation.is_some());
        assert_eq!(server.requests().len(), 3);
    }

    #[test]
    fn gives_up_after_the_last_delay() {
        let server = TestServer::start(vec![]);
        assert!(fetch_generation(&client(&server, None), Some("gen-1"), &fast(2)).is_none());
        assert_eq!(server.requests().len(), 3);
    }

    #[test]
    fn returns_none_when_the_body_is_not_a_generation() {
        let server = TestServer::start(vec![Reply::json(200, r#"{"nope":1}"#)]);
        assert!(fetch_generation(&client(&server, None), Some("gen-1"), &fast(0)).is_none());
    }

    #[test]
    fn returns_none_when_the_network_fails() {
        let server = TestServer::start(vec![]);
        let dead = OpenRouterClient::new(
            "http://127.0.0.1:1/api/v1".into(),
            None,
            &AgentOptions::direct(),
        )
        .unwrap();
        drop(server);
        assert!(fetch_generation(&dead, Some("gen-1"), &fast(1)).is_none());
    }

    #[test]
    fn does_not_call_the_endpoint_without_a_generation_id() {
        let server = TestServer::start(vec![Reply::json(200, BODY)]);
        assert!(fetch_generation(&client(&server, None), None, &fast(2)).is_none());
        assert!(server.requests().is_empty());
    }

    #[test]
    fn the_default_schedule_waits_about_ten_seconds_in_total() {
        let schedule = GenerationSchedule::default();
        let total: Duration = schedule.delays.iter().sum();
        assert_eq!(total, Duration::from_secs(10));
    }
}
