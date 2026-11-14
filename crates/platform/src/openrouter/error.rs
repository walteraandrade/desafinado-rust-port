use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ClientError {
    OutOfCredit { message: String },
    RateLimited { message: String },
    Timeout { message: String },
    Aborted { message: String },
    Network { message: String },
    InvalidOutput { message: String },
    Api { status: u16, message: String },
    Unknown { message: String },
}

impl ClientError {
    pub fn message(&self) -> &str {
        match self {
            Self::OutOfCredit { message }
            | Self::RateLimited { message }
            | Self::Timeout { message }
            | Self::Aborted { message }
            | Self::Network { message }
            | Self::InvalidOutput { message }
            | Self::Api { message, .. }
            | Self::Unknown { message } => message,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::OutOfCredit { .. } => "out-of-credit",
            Self::RateLimited { .. } => "rate-limited",
            Self::Timeout { .. } => "timeout",
            Self::Aborted { .. } => "aborted",
            Self::Network { .. } => "network",
            Self::InvalidOutput { .. } => "invalid-output",
            Self::Api { .. } => "api",
            Self::Unknown { .. } => "unknown",
        }
    }

    pub fn is_retryable_output_error(&self) -> bool {
        matches!(self, Self::InvalidOutput { .. })
    }

    pub fn is_retryable_transport_error(&self) -> bool {
        matches!(
            self,
            Self::Network { .. }
                | Self::RateLimited { .. }
                | Self::Api {
                    status: 500..=599,
                    ..
                }
        )
    }
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind(), self.message())
    }
}

impl std::error::Error for ClientError {}

pub fn message_from_body(body: &str, fallback: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            let error = value.get("error")?;
            error
                .get("message")
                .and_then(|m| m.as_str())
                .or_else(|| error.as_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| fallback.to_owned())
}

fn payload_status(body: &str) -> Option<u16> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let code = value
        .get("error")?
        .get("code")
        .or_else(|| value.get("code"))?;
    code.as_u64()
        .or_else(|| code.as_str().and_then(|s| s.parse().ok()))
        .and_then(|n| u16::try_from(n).ok())
}

fn from_status(status: u16, message: String) -> ClientError {
    match status {
        402 => ClientError::OutOfCredit { message },
        429 => ClientError::RateLimited { message },
        _ => ClientError::Api { status, message },
    }
}

pub fn classify_status(status: u16, body: &str) -> ClientError {
    let message = message_from_body(body, &format!("OpenRouter answered {status}"));
    let success = (200..300).contains(&status);
    let effective = if success {
        payload_status(body).unwrap_or(status)
    } else {
        status
    };
    if (200..300).contains(&effective) {
        ClientError::Unknown {
            message: "the answer stream broke off. Retry.".into(),
        }
    } else {
        from_status(effective, message)
    }
}

pub fn classify_transport(error: &ureq::Error) -> ClientError {
    let message = error.to_string();
    match error {
        ureq::Error::Timeout(_) => ClientError::Timeout { message },
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::TimedOut => {
            ClientError::Timeout { message }
        }
        _ => ClientError::Network { message },
    }
}

pub fn classify_io(error: &std::io::Error) -> ClientError {
    if let Some(inner) = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<ureq::Error>())
    {
        return classify_transport(inner);
    }
    match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => ClientError::Timeout {
            message: error.to_string(),
        },
        _ => ClientError::Network {
            message: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_402_to_out_of_credit_and_429_to_rate_limited() {
        assert_eq!(
            classify_status(402, r#"{"error":{"message":"no credit","code":402}}"#),
            ClientError::OutOfCredit {
                message: "no credit".into()
            }
        );
        assert!(matches!(
            classify_status(429, ""),
            ClientError::RateLimited { .. }
        ));
    }

    #[test]
    fn maps_other_http_failures_to_api_with_the_status() {
        assert_eq!(
            classify_status(500, "oops"),
            ClientError::Api {
                status: 500,
                message: "OpenRouter answered 500".into()
            }
        );
    }

    #[test]
    fn reads_the_status_from_an_error_payload_sent_with_http_200() {
        assert!(matches!(
            classify_status(200, r#"{"error":{"message":"slow down","code":429}}"#),
            ClientError::RateLimited { message } if message == "slow down"
        ));
        assert!(matches!(
            classify_status(200, r#"{"error":{"message":"bad","code":502}}"#),
            ClientError::Api { status: 502, .. }
        ));
    }

    #[test]
    fn maps_a_broken_response_under_a_2xx_status_to_unknown() {
        assert!(matches!(
            classify_status(200, "not json"),
            ClientError::Unknown { .. }
        ));
        assert!(matches!(
            classify_status(200, r#"{"error":{"message":"x","code":200}}"#),
            ClientError::Unknown { .. }
        ));
    }

    #[test]
    fn retries_only_invalid_output_at_the_output_level() {
        assert!(
            ClientError::InvalidOutput {
                message: "x".into()
            }
            .is_retryable_output_error()
        );
        assert!(
            !ClientError::Network {
                message: "x".into()
            }
            .is_retryable_output_error()
        );
    }

    #[test]
    fn transport_retry_covers_network_rate_limit_and_5xx() {
        assert!(classify_status(503, "").is_retryable_transport_error());
        assert!(classify_status(429, "").is_retryable_transport_error());
        assert!(!classify_status(402, "").is_retryable_transport_error());
        assert!(!classify_status(400, "").is_retryable_transport_error());
    }

    #[test]
    fn serializes_with_a_kebab_case_type() {
        let json = serde_json::to_value(ClientError::OutOfCredit {
            message: "m".into(),
        })
        .unwrap();
        assert_eq!(json["type"], "out-of-credit");
    }
}
