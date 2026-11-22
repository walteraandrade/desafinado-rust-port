//! The text caret the writer chooses in Preferences: its shape, its width and
//! how fast it blinks.

use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const CARET_FILE_NAME: &str = "caret.json";
pub const MIN_WIDTH: u8 = 1;
pub const MAX_WIDTH: u8 = 5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaretShape {
    /// A vertical bar before the next character.
    #[default]
    Bar,
    /// A block over the next character, which shows in the background color (Vim's caret).
    Block,
    /// A line under the next character.
    Underline,
}

impl CaretShape {
    pub const ALL: [Self; 3] = [Self::Bar, Self::Block, Self::Underline];

    pub fn label(self) -> &'static str {
        match self {
            Self::Bar => "Bar",
            Self::Block => "Block",
            Self::Underline => "Underline",
        }
    }

    /// Whether the width setting changes this shape; a block is as wide as its character.
    pub fn has_width(self) -> bool {
        self != Self::Block
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaretBlink {
    Off,
    #[default]
    Slow,
    Medium,
    Fast,
}

impl CaretBlink {
    pub const ALL: [Self; 4] = [Self::Off, Self::Slow, Self::Medium, Self::Fast];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Slow => "Slow",
            Self::Medium => "Medium",
            Self::Fast => "Fast",
        }
    }

    /// How long the caret stays shown, and then hidden; `None` when it does not blink.
    pub fn phase(self) -> Option<Duration> {
        match self {
            Self::Off => None,
            Self::Slow => Some(Duration::from_secs(1)),
            Self::Medium => Some(Duration::from_millis(530)),
            Self::Fast => Some(Duration::from_millis(300)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaretSettings {
    pub shape: CaretShape,
    /// Pixels, from [`MIN_WIDTH`] to [`MAX_WIDTH`].
    pub width: u8,
    pub blink: CaretBlink,
}

impl Default for CaretSettings {
    fn default() -> Self {
        Self {
            shape: CaretShape::Bar,
            width: 3,
            blink: CaretBlink::Slow,
        }
    }
}

impl CaretSettings {
    #[must_use]
    pub fn with_width(self, width: u8) -> Self {
        Self {
            width: width.clamp(MIN_WIDTH, MAX_WIDTH),
            ..self
        }
    }

    /// Reads saved settings. Unknown or broken values fall back to the defaults,
    /// and a width out of range is brought into it.
    pub fn parse(text: &str) -> Self {
        let settings: Self = serde_json::from_str(text).unwrap_or_default();
        settings.with_width(settings.width)
    }

    pub fn to_json(self) -> String {
        serde_json::to_string_pretty(&self).unwrap_or_default()
    }
}

/// Whether the caret shows `since` the writer last typed or moved it: it
/// starts shown, so it never hides while they work.
pub fn caret_shown(blink: CaretBlink, since: Duration) -> bool {
    blink
        .phase()
        .is_none_or(|phase| (since.as_millis() / phase.as_millis()).is_multiple_of(2))
}

/// How long until the caret next shows or hides, or `None` when it does not blink.
pub fn until_next_phase(blink: CaretBlink, since: Duration) -> Option<Duration> {
    let phase = blink.phase()?;
    let into = since.as_millis() % phase.as_millis();
    let left = phase.as_millis() - into;
    Some(Duration::from_millis(
        u64::try_from(left).unwrap_or(u64::MAX),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_a_three_pixel_bar_that_blinks_slowly() {
        let settings = CaretSettings::default();
        assert_eq!(settings.shape, CaretShape::Bar);
        assert_eq!(settings.width, 3);
        assert_eq!(settings.blink, CaretBlink::Slow);
    }

    #[test]
    fn round_trips_through_json() {
        let settings = CaretSettings {
            shape: CaretShape::Block,
            width: 5,
            blink: CaretBlink::Off,
        };
        assert_eq!(CaretSettings::parse(&settings.to_json()), settings);
    }

    #[test]
    fn falls_back_to_defaults_for_broken_or_unknown_text() {
        assert_eq!(CaretSettings::parse("not json"), CaretSettings::default());
        assert_eq!(
            CaretSettings::parse(r#"{"shape":"star"}"#),
            CaretSettings::default()
        );
        assert_eq!(
            CaretSettings::parse(r#"{"blink":"fast"}"#),
            CaretSettings {
                blink: CaretBlink::Fast,
                ..CaretSettings::default()
            }
        );
    }

    #[test]
    fn keeps_the_width_between_one_and_five_pixels() {
        assert_eq!(CaretSettings::parse(r#"{"width":0}"#).width, 1);
        assert_eq!(CaretSettings::parse(r#"{"width":9}"#).width, 5);
        assert_eq!(CaretSettings::default().with_width(4).width, 4);
    }

    #[test]
    fn a_block_ignores_the_width() {
        assert!(CaretShape::Bar.has_width());
        assert!(CaretShape::Underline.has_width());
        assert!(!CaretShape::Block.has_width());
    }

    #[test]
    fn slow_blinks_slower_than_medium_and_fast() {
        let ms = |blink: CaretBlink| blink.phase().map(|phase| phase.as_millis());
        assert!(ms(CaretBlink::Slow) > ms(CaretBlink::Medium));
        assert!(ms(CaretBlink::Medium) > ms(CaretBlink::Fast));
        assert_eq!(ms(CaretBlink::Off), None);
    }

    #[test]
    fn shows_first_then_alternates_each_phase() {
        let at = Duration::from_millis;
        assert!(caret_shown(CaretBlink::Slow, at(0)));
        assert!(caret_shown(CaretBlink::Slow, at(999)));
        assert!(!caret_shown(CaretBlink::Slow, at(1000)));
        assert!(caret_shown(CaretBlink::Slow, at(2000)));
        assert!(caret_shown(CaretBlink::Off, at(1500)));
    }

    #[test]
    fn waits_until_the_next_phase_boundary() {
        let at = Duration::from_millis;
        assert_eq!(until_next_phase(CaretBlink::Slow, at(250)), Some(at(750)));
        assert_eq!(until_next_phase(CaretBlink::Slow, at(1000)), Some(at(1000)));
        assert_eq!(until_next_phase(CaretBlink::Off, at(250)), None);
    }
}
