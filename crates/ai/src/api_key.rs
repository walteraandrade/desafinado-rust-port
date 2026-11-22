//! The OpenRouter key, kept out of every `{:?}` and log line.

use std::fmt;

#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    /// Wraps a key, or returns `None` for an empty one.
    pub fn new(key: impl Into<String>) -> Option<Self> {
        let key = key.into();
        (!key.is_empty()).then_some(Self(key))
    }

    /// The key itself, for the one place that sends it: the `Authorization` header.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ApiKey(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_shows_the_key_when_debug_printed() {
        let key = ApiKey::new("sk-or-v1-secret").unwrap();
        let printed = format!("{key:?} {:?} {:#?}", Some(&key), vec![key.clone()]);
        assert!(!printed.contains("secret"));
        assert!(printed.contains("ApiKey(***)"));
    }

    #[test]
    fn treats_an_empty_key_as_none() {
        assert_eq!(ApiKey::new(""), None);
        assert_eq!(
            ApiKey::new("sk").map(|key| key.expose().to_owned()),
            Some("sk".into())
        );
    }
}
