use super::geometry::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardPlacement {
    Margin,
    #[default]
    Floating,
}

pub const MARGIN_CARD_WIDTH: f64 = 256.0;
const MARGIN_GAP: f64 = 24.0;
const VIEWPORT_GUTTER: f64 = 16.0;

pub fn placement_for(content_right: f64, viewport_width: f64) -> CardPlacement {
    if viewport_width - content_right >= MARGIN_GAP + MARGIN_CARD_WIDTH + VIEWPORT_GUTTER {
        CardPlacement::Margin
    } else {
        CardPlacement::Floating
    }
}

pub fn margin_anchor(content_right: f64, line_top: f64) -> Rect {
    Rect {
        left: content_right + MARGIN_GAP,
        right: content_right + MARGIN_GAP,
        top: line_top,
        bottom: line_top,
    }
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
        assert_eq!(placement_for(1016.0, 1312.0), CardPlacement::Margin);
    }

    #[test]
    fn keeps_cards_floating_when_the_margin_is_one_pixel_short() {
        assert_eq!(placement_for(1016.0, 1311.0), CardPlacement::Floating);
    }

    #[test]
    fn keeps_cards_floating_when_the_text_fills_the_viewport() {
        assert_eq!(placement_for(1000.0, 1024.0), CardPlacement::Floating);
    }

    #[test]
    fn margin_anchor_starts_one_gap_right_of_the_text_level_with_the_line_top() {
        assert_eq!(
            margin_anchor(1016.0, 340.0),
            Rect {
                left: 1040.0,
                right: 1040.0,
                top: 340.0,
                bottom: 340.0
            }
        );
    }

    #[test]
    fn margin_anchor_leaves_room_for_the_card_inside_the_measured_margin() {
        let anchor = margin_anchor(1016.0, 0.0);
        assert!(anchor.left + MARGIN_CARD_WIDTH <= 1312.0 - 16.0);
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
