#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardPlacement {
    Margin,
    #[default]
    Floating,
}

pub const MARGIN_CARD_WIDTH: f64 = 256.0;
/// Room between a margin card and the text, which keeps the Note marks beside the text clear.
const MARGIN_GAP: f64 = 40.0;
const VIEWPORT_GUTTER: f64 = 16.0;

/// Cards sit in the margin left of the text when one fits there; the right of the text is the
/// text map's.
pub fn placement_for(content_left: f64) -> CardPlacement {
    if content_left >= MARGIN_GAP + MARGIN_CARD_WIDTH + VIEWPORT_GUTTER {
        CardPlacement::Margin
    } else {
        CardPlacement::Floating
    }
}

/// The left edge of a card in the margin, one gap left of the text.
pub fn margin_card_left(content_left: f64) -> f64 {
    content_left - MARGIN_GAP - MARGIN_CARD_WIDTH
}

pub fn latest_placement(current: CardPlacement, effects: &[CardPlacement]) -> CardPlacement {
    effects
        .iter()
        .copied()
        .fold(current, |_, placement| placement)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn puts_cards_in_the_margin_when_a_card_its_gap_and_the_gutter_fit() {
        assert_eq!(placement_for(312.0), CardPlacement::Margin);
    }

    #[test]
    fn keeps_cards_floating_when_the_margin_is_one_pixel_short() {
        assert_eq!(placement_for(311.0), CardPlacement::Floating);
    }

    #[test]
    fn keeps_cards_floating_when_the_text_starts_at_the_edge() {
        assert_eq!(placement_for(24.0), CardPlacement::Floating);
    }

    #[test]
    fn a_margin_card_ends_one_gap_left_of_the_text() {
        assert!(
            (margin_card_left(400.0) + MARGIN_CARD_WIDTH - (400.0 - 40.0)).abs() < f64::EPSILON
        );
    }

    #[test]
    fn a_margin_card_keeps_the_gutter_from_the_window_edge() {
        assert!(margin_card_left(312.0) >= VIEWPORT_GUTTER);
    }

    #[test]
    fn the_field_starts_floating() {
        assert_eq!(CardPlacement::default(), CardPlacement::Floating);
        assert_eq!(
            latest_placement(CardPlacement::default(), &[]),
            CardPlacement::Floating
        );
    }

    #[test]
    fn the_field_follows_the_latest_placement_effect() {
        assert_eq!(
            latest_placement(CardPlacement::Floating, &[CardPlacement::Margin]),
            CardPlacement::Margin
        );
        assert_eq!(
            latest_placement(
                CardPlacement::Margin,
                &[CardPlacement::Margin, CardPlacement::Floating]
            ),
            CardPlacement::Floating
        );
    }
}
