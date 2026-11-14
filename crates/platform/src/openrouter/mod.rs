mod agent;
mod client;
mod error;
mod generation;

#[cfg(test)]
mod test_server;

pub use agent::{AgentOptions, build_agent, extra_root_certs_from_env};
pub use client::{ChatOutput, ChatStream, OpenRouterClient, RetryPolicy};
pub use error::{ClientError, classify_status, is_retryable_transport_error};
pub use generation::{GenerationSchedule, fetch_generation};

pub const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
pub const BASE_URL_ENV: &str = "DESAFINADO_OPENROUTER_BASE_URL";
pub const PLACEHOLDER_KEY: &str = "placeholder";

pub fn base_url_from_env(read_var: &dyn Fn(&str) -> Option<String>) -> String {
    read_var(BASE_URL_ENV)
        .map(|value| value.trim().trim_end_matches('/').to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_base_url_defaults_and_can_be_overridden_by_env() {
        assert_eq!(base_url_from_env(&|_| None), DEFAULT_BASE_URL);
        assert_eq!(
            base_url_from_env(&|_| Some("http://127.0.0.1:9/v1/".into())),
            "http://127.0.0.1:9/v1"
        );
        assert_eq!(base_url_from_env(&|_| Some("  ".into())), DEFAULT_BASE_URL);
    }
}
