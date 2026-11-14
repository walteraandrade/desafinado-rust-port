use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

pub use desafinado_core::suggestions::ClientError;

pub type CallResult<T> = Result<T, ClientError>;

#[derive(Debug, Clone, PartialEq)]
pub enum RawError {
    ApiCall {
        status_code: Option<u16>,
        data: Option<Value>,
        message: String,
    },
    Retry {
        errors: Vec<RawError>,
    },
    NoObjectGenerated {
        message: String,
    },
    Timeout {
        message: String,
    },
    Aborted {
        message: String,
    },
    FetchFailure {
        message: String,
    },
    Other {
        message: String,
        cause: Option<Box<RawError>>,
    },
}

static FETCH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)fetch").expect("valid regex"));

fn payload_status(data: Option<&Value>) -> Option<u16> {
    data?
        .get("code")?
        .as_f64()
        .filter(|code| code.fract() == 0.0 && (0.0..=65535.0).contains(code))
        .map(|code| code as u16)
}

fn from_status(status: u16, message: &str) -> ClientError {
    let message = message.to_owned();
    match status {
        402 => ClientError::OutOfCredit { message },
        429 => ClientError::RateLimited { message },
        status => ClientError::Api { status, message },
    }
}

const fn is_success_status(status: u16) -> bool {
    status >= 200 && status < 300
}

fn from_api_call_error(
    status_code: Option<u16>,
    data: Option<&Value>,
    message: &str,
) -> ClientError {
    let status = match status_code {
        Some(code) if !is_success_status(code) => Some(code),
        _ => payload_status(data).or(status_code),
    };
    match status {
        None => ClientError::Network {
            message: message.to_owned(),
        },
        Some(status) if is_success_status(status) => ClientError::Unknown {
            message: "the answer stream broke off. Retry.".to_owned(),
        },
        Some(status) => from_status(status, message),
    }
}

fn from_direct_error(error: &RawError) -> Option<ClientError> {
    match error {
        RawError::Retry { errors } => Some(errors.last().map_or_else(
            || ClientError::Unknown {
                message: "undefined".to_owned(),
            },
            classify_error,
        )),
        RawError::ApiCall {
            status_code,
            data,
            message,
        } => Some(from_api_call_error(*status_code, data.as_ref(), message)),
        RawError::NoObjectGenerated { message } => Some(ClientError::InvalidOutput {
            message: message.clone(),
        }),
        RawError::Timeout { message } => Some(ClientError::Timeout {
            message: message.clone(),
        }),
        RawError::Aborted { message } => Some(ClientError::Aborted {
            message: message.clone(),
        }),
        RawError::FetchFailure { message } if FETCH.is_match(message) => {
            Some(ClientError::Network {
                message: message.clone(),
            })
        }
        RawError::FetchFailure { .. } | RawError::Other { .. } => None,
    }
}

fn message_of(error: &RawError) -> String {
    match error {
        RawError::ApiCall { message, .. }
        | RawError::NoObjectGenerated { message }
        | RawError::Timeout { message }
        | RawError::Aborted { message }
        | RawError::FetchFailure { message }
        | RawError::Other { message, .. } => message.clone(),
        RawError::Retry { .. } => "RetryError".to_owned(),
    }
}

pub fn classify_error(error: &RawError) -> ClientError {
    if let Some(direct) = from_direct_error(error) {
        return direct;
    }
    if let RawError::Other {
        cause: Some(cause), ..
    } = error
    {
        let nested = classify_error(cause);
        if !matches!(nested, ClientError::Unknown { .. }) {
            return nested;
        }
    }
    ClientError::Unknown {
        message: message_of(error),
    }
}

pub const fn is_retryable_output_error(error: &ClientError) -> bool {
    matches!(error, ClientError::InvalidOutput { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn api_error(status: Option<u16>, data: Option<Value>) -> RawError {
        RawError::ApiCall {
            status_code: status,
            data,
            message: format!("HTTP {status:?}"),
        }
    }

    fn kind(error: &RawError) -> &'static str {
        classify_error(error).kind()
    }

    #[test]
    fn maps_402_to_out_of_credit() {
        assert_eq!(kind(&api_error(Some(402), None)), "out-of-credit");
    }

    #[test]
    fn maps_429_to_rate_limited() {
        assert_eq!(kind(&api_error(Some(429), None)), "rate-limited");
    }

    #[test]
    fn maps_other_http_failures_to_api_with_the_status() {
        assert!(matches!(
            classify_error(&api_error(Some(500), None)),
            ClientError::Api { status: 500, .. }
        ));
    }

    #[test]
    fn maps_a_failed_connection_to_network() {
        assert_eq!(kind(&api_error(None, None)), "network");
    }

    #[test]
    fn reads_the_status_from_an_error_payload_sent_with_http_200() {
        assert_eq!(
            kind(&api_error(Some(200), Some(json!({"code": 402})))),
            "out-of-credit"
        );
        assert_eq!(
            kind(&api_error(Some(200), Some(json!({"code": 429})))),
            "rate-limited"
        );
    }

    #[test]
    fn maps_a_broken_response_under_a_2xx_status_to_unknown() {
        for status in [200, 201, 204] {
            let error = classify_error(&api_error(Some(status), None));
            assert_eq!(error.kind(), "unknown");
            assert_eq!(error.message(), "the answer stream broke off. Retry.");
        }
        assert_eq!(
            kind(&api_error(Some(200), Some(json!({"code": 200})))),
            "unknown"
        );
    }

    #[test]
    fn still_reads_a_failure_code_from_the_payload_under_http_200() {
        assert!(matches!(
            classify_error(&api_error(Some(200), Some(json!({"code": 500})))),
            ClientError::Api { status: 500, .. }
        ));
    }

    #[test]
    fn maps_a_browser_fetch_failure_to_network() {
        let error = RawError::FetchFailure {
            message: "Failed to fetch".into(),
        };
        assert_eq!(kind(&error), "network");
    }

    #[test]
    fn maps_a_timeout_to_timeout() {
        let error = RawError::Timeout {
            message: "The operation timed out.".into(),
        };
        assert_eq!(kind(&error), "timeout");
    }

    #[test]
    fn maps_an_abort_to_aborted() {
        let error = RawError::Aborted {
            message: "Aborted".into(),
        };
        assert_eq!(kind(&error), "aborted");
    }

    #[test]
    fn maps_a_schema_failure_to_invalid_output() {
        let error = RawError::NoObjectGenerated {
            message: "No object generated".into(),
        };
        assert_eq!(kind(&error), "invalid-output");
    }

    #[test]
    fn unwraps_a_retry_error_to_its_last_error() {
        let wrapped = RawError::Retry {
            errors: vec![api_error(Some(500), None), api_error(Some(429), None)],
        };
        assert_eq!(kind(&wrapped), "rate-limited");
    }

    #[test]
    fn finds_a_classified_error_in_the_cause_chain() {
        let outer = RawError::Other {
            message: "wrapper".into(),
            cause: Some(Box::new(api_error(Some(402), None))),
        };
        assert_eq!(kind(&outer), "out-of-credit");
    }

    #[test]
    fn falls_back_to_unknown_with_the_message() {
        let boom = RawError::Other {
            message: "boom".into(),
            cause: None,
        };
        assert_eq!(
            classify_error(&boom),
            ClientError::Unknown {
                message: "boom".into()
            }
        );
        let plain = RawError::Other {
            message: "plain string".into(),
            cause: None,
        };
        assert_eq!(kind(&plain), "unknown");
    }

    #[test]
    fn retries_only_invalid_output() {
        let error = |kind: &str| match kind {
            "invalid-output" => ClientError::InvalidOutput {
                message: "x".into(),
            },
            "timeout" => ClientError::Timeout {
                message: "x".into(),
            },
            _ => ClientError::OutOfCredit {
                message: "x".into(),
            },
        };
        assert!(is_retryable_output_error(&error("invalid-output")));
        assert!(!is_retryable_output_error(&error("timeout")));
        assert!(!is_retryable_output_error(&error("out-of-credit")));
    }
}
