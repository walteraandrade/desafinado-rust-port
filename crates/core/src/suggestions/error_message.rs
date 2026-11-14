use super::types::ClientError;

pub fn describe_error(error: &ClientError) -> String {
    match error {
        ClientError::OutOfCredit { .. } => {
            "OpenRouter is out of credit. Add credit to your OpenRouter account, then retry."
                .to_string()
        }
        ClientError::RateLimited { .. } => {
            "Too many requests. Wait a moment, then retry.".to_string()
        }
        ClientError::Timeout { .. } => "The model took too long to answer. Retry.".to_string(),
        ClientError::Network { .. } => {
            "Could not reach the model. Check your connection, then retry.".to_string()
        }
        ClientError::InvalidOutput { .. } => {
            "The model returned an unusable answer twice. Retry, or rephrase the Instruction."
                .to_string()
        }
        ClientError::Aborted { .. } => "The request was cancelled.".to_string(),
        ClientError::Api { status, message } => {
            format!("The model request failed ({status}): {message}")
        }
        ClientError::Unknown { message } => format!("Something went wrong: {message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(error: ClientError) -> String {
        describe_error(&error)
    }

    fn empty() -> String {
        String::new()
    }

    #[test]
    fn gives_out_of_credit_its_own_message() {
        let text = message(ClientError::OutOfCredit {
            message: "Insufficient credits".into(),
        });
        assert!(text.starts_with("OpenRouter is out of credit"));
    }

    #[test]
    fn describes_rate_limits_timeouts_and_network_failures_briefly() {
        assert_eq!(
            message(ClientError::RateLimited { message: empty() }),
            "Too many requests. Wait a moment, then retry."
        );
        assert_eq!(
            message(ClientError::Timeout { message: empty() }),
            "The model took too long to answer. Retry."
        );
        assert_eq!(
            message(ClientError::Network { message: empty() }),
            "Could not reach the model. Check your connection, then retry."
        );
    }

    #[test]
    fn explains_an_invalid_output_that_survived_the_silent_retry() {
        assert_eq!(
            message(ClientError::InvalidOutput { message: empty() }),
            "The model returned an unusable answer twice. Retry, or rephrase the Instruction."
        );
    }

    #[test]
    fn shows_the_status_of_other_api_errors() {
        assert_eq!(
            message(ClientError::Api {
                status: 500,
                message: "Boom".into()
            }),
            "The model request failed (500): Boom"
        );
    }

    #[test]
    fn falls_back_to_the_raw_message_for_unknown_errors() {
        assert_eq!(
            message(ClientError::Unknown {
                message: "Weird".into()
            }),
            "Something went wrong: Weird"
        );
    }

    #[test]
    fn describes_an_aborted_request() {
        assert_eq!(
            message(ClientError::Aborted { message: empty() }),
            "The request was cancelled."
        );
    }
}
