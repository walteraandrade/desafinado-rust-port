//! The spell checker's settings, chosen in Preferences: whether it runs, and the key that
//! opens the suggestions for the word under the caret.

use serde::{Deserialize, Serialize};

pub const SPELLING_FILE_NAME: &str = "spelling.json";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MenuKey {
    /// "?" inside a marked word. At its end or anywhere else it types "?".
    #[default]
    Question,
    CtrlPeriod,
    AltEnter,
}

/// One key press, as the window reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Press<'a> {
    pub key: &'a str,
    pub key_char: Option<&'a str>,
    pub control: bool,
    pub alt: bool,
}

impl MenuKey {
    pub const ALL: [Self; 3] = [Self::Question, Self::CtrlPeriod, Self::AltEnter];

    pub fn label(self) -> &'static str {
        match self {
            Self::Question => "?",
            Self::CtrlPeriod => "Ctrl+.",
            Self::AltEnter => "Alt+Enter",
        }
    }

    pub fn matches(self, press: Press<'_>) -> bool {
        match self {
            Self::Question => press.key_char == Some("?") && !press.control && !press.alt,
            Self::CtrlPeriod => press.key == "." && press.control && !press.alt,
            Self::AltEnter => press.key == "enter" && press.alt && !press.control,
        }
    }

    /// "?" is also text, so it opens a menu only strictly inside a marked word.
    pub fn strictly_inside(self) -> bool {
        self == Self::Question
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpellSettings {
    pub enabled: bool,
    pub menu_key: MenuKey,
}

impl Default for SpellSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            menu_key: MenuKey::default(),
        }
    }
}

impl SpellSettings {
    /// Reads saved settings. A field that is missing or unknown falls back to its default
    /// alone; text that is not JSON at all is an error.
    pub fn parse(text: &str) -> Result<Self, serde_json::Error> {
        let value: serde_json::Value = serde_json::from_str(text)?;
        let defaults = Self::default();
        let field = |name: &str| value.get(name).cloned();
        Ok(Self {
            enabled: field("enabled")
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or(defaults.enabled),
            menu_key: field("menu_key")
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or(defaults.menu_key),
        })
    }

    pub fn to_json(self) -> String {
        serde_json::to_string_pretty(&self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press<'a>(key: &'a str, key_char: Option<&'a str>, control: bool, alt: bool) -> Press<'a> {
        Press {
            key,
            key_char,
            control,
            alt,
        }
    }

    #[test]
    fn saved_settings_read_back_and_each_broken_field_falls_back_alone() {
        let settings = SpellSettings {
            enabled: false,
            menu_key: MenuKey::AltEnter,
        };
        assert_eq!(
            SpellSettings::parse(&settings.to_json()).ok(),
            Some(settings)
        );
        assert_eq!(
            SpellSettings::parse("{}").ok(),
            Some(SpellSettings {
                enabled: true,
                menu_key: MenuKey::Question
            })
        );
        assert_eq!(
            SpellSettings::parse(r#"{"enabled": false, "menu_key": "f9"}"#).ok(),
            Some(SpellSettings {
                enabled: false,
                menu_key: MenuKey::Question
            })
        );
        assert!(SpellSettings::parse("not json").is_err());
    }

    #[test]
    fn each_key_matches_its_own_press_only() {
        let question = press("/", Some("?"), false, false);
        let ctrl_period = press(".", Some("."), true, false);
        let alt_enter = press("enter", None, false, true);
        assert!(MenuKey::Question.matches(question));
        assert!(!MenuKey::Question.matches(press("/", Some("/"), false, false)));
        assert!(MenuKey::CtrlPeriod.matches(ctrl_period));
        assert!(!MenuKey::CtrlPeriod.matches(press(".", Some("."), false, false)));
        assert!(MenuKey::AltEnter.matches(alt_enter));
        assert!(!MenuKey::AltEnter.matches(press("enter", None, true, false)));
        assert!(!MenuKey::AltEnter.matches(question));
    }
}
