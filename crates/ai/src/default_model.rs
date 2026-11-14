use crate::text::js_trim;

pub const FALLBACK_MODEL: &str = "openai/gpt-6.1-terra";

pub fn resolve_model(configured: Option<&str>) -> String {
    configured
        .map(js_trim)
        .filter(|trimmed| !trimmed.is_empty())
        .unwrap_or(FALLBACK_MODEL)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_the_configured_model() {
        assert_eq!(resolve_model(Some(" vendor/model-a ")), "vendor/model-a");
    }

    #[test]
    fn falls_back_when_nothing_is_configured() {
        assert_eq!(resolve_model(None), FALLBACK_MODEL);
        assert_eq!(resolve_model(Some("")), FALLBACK_MODEL);
        assert_eq!(resolve_model(Some("   ")), FALLBACK_MODEL);
    }
}
