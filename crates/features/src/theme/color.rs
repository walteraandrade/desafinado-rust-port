pub type Rgb = [u8; 3];

pub fn rgb_of(hex: &str) -> Rgb {
    let digits: Vec<char> = hex.chars().skip(1).collect();
    let full: String = if digits.len() == 3 {
        digits.iter().flat_map(|d| [*d, *d]).collect()
    } else {
        digits.iter().take(6).collect()
    };
    let channel = |start: usize| -> u8 {
        full.get(start..start + 2)
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
            .unwrap_or(0)
    };
    [channel(0), channel(2), channel(4)]
}

fn linear(channel: u8) -> f64 {
    let value = f64::from(channel) / 255.0;
    if value <= 0.03928 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(hex: &str) -> f64 {
    let [r, g, b] = rgb_of(hex);
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

pub fn contrast(a: &str, b: &str) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (high, low) = if la >= lb { (la, lb) } else { (lb, la) };
    (high + 0.05) / (low + 0.05)
}

fn to_hex(channel: f64) -> String {
    format!("{:02x}", channel.round().clamp(0.0, 255.0) as u8)
}

pub fn mix_hex(from: &str, to: &str, share: f64) -> String {
    let [r1, g1, b1] = rgb_of(from);
    let [r2, g2, b2] = rgb_of(to);
    let mix = |a: u8, b: u8| to_hex(f64::from(a) + (f64::from(b) - f64::from(a)) * share);
    format!("#{}{}{}", mix(r1, r2), mix(g1, g2), mix(b1, b2))
}

pub fn six_digit_hex(hex: &str) -> String {
    let [r, g, b] = rgb_of(hex);
    format!(
        "#{}{}{}",
        to_hex(r.into()),
        to_hex(g.into()),
        to_hex(b.into())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64, digits: i32) {
        assert!(
            (actual - expected).abs() < 0.5 * 10f64.powi(-digits),
            "{actual} vs {expected}"
        );
    }

    #[test]
    fn reads_six_digit_short_and_alpha_hex() {
        assert_eq!(rgb_of("#ff8000"), [255, 128, 0]);
        assert_eq!(rgb_of("#f80"), [255, 136, 0]);
        assert_eq!(rgb_of("#ff800080"), [255, 128, 0]);
    }

    #[test]
    fn contrast_is_21_to_1_between_black_and_white_in_either_order() {
        close(contrast("#000000", "#ffffff"), 21.0, 2);
        close(contrast("#ffffff", "#000000"), 21.0, 2);
    }

    #[test]
    fn contrast_is_1_to_1_for_one_color() {
        close(contrast("#771133", "#771133"), 1.0, 2);
    }

    #[test]
    fn contrast_matches_a_known_wcag_value() {
        close(contrast("#777777", "#ffffff"), 4.48, 2);
    }

    #[test]
    fn mix_returns_each_end_at_0_and_1() {
        assert_eq!(mix_hex("#102030", "#f0e0d0", 0.0), "#102030");
        assert_eq!(mix_hex("#102030", "#f0e0d0", 1.0), "#f0e0d0");
    }

    #[test]
    fn mixes_channels_in_srgb_and_answers_solid_hex() {
        assert_eq!(mix_hex("#000000", "#ffffff", 0.5), "#808080");
        assert_eq!(mix_hex("#00000080", "#ffffff", 0.5), "#808080");
    }
}
