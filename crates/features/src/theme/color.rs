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

fn unlinear(value: f64) -> f64 {
    if value <= 0.003_130_8 {
        12.92 * value
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

fn oklch_to_srgb(lightness: f64, chroma: f64, hue: f64) -> [f64; 3] {
    let (a, b) = (
        chroma * hue.to_radians().cos(),
        chroma * hue.to_radians().sin(),
    );
    let l = (lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
    let m = (lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
    let s = (lightness - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
    [
        4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
        -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
        -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701 * s,
    ]
    .map(|channel| {
        if channel > 0.0 {
            unlinear(channel)
        } else {
            channel
        }
    })
}

fn in_gamut(rgb: [f64; 3]) -> bool {
    rgb.iter()
        .all(|channel| (-1e-4..=1.0 + 1e-4).contains(channel))
}

pub fn oklch_hex(lightness: f64, chroma: f64, hue: f64) -> String {
    let chroma = if in_gamut(oklch_to_srgb(lightness, chroma, hue)) {
        chroma
    } else {
        (0..30)
            .fold((0.0, chroma), |(low, high), _| {
                let middle = f64::midpoint(low, high);
                if in_gamut(oklch_to_srgb(lightness, middle, hue)) {
                    (middle, high)
                } else {
                    (low, middle)
                }
            })
            .0
    };
    let [r, g, b] = oklch_to_srgb(lightness, chroma, hue);
    format!(
        "#{}{}{}",
        to_hex(r.clamp(0.0, 1.0) * 255.0),
        to_hex(g.clamp(0.0, 1.0) * 255.0),
        to_hex(b.clamp(0.0, 1.0) * 255.0)
    )
}

pub fn oklch_of(hex: &str) -> [f64; 3] {
    let [r, g, b] = rgb_of(hex).map(linear);
    let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
    let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
    let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
    let lightness = 0.210_454_255_3 * l + 0.793_617_785 * m - 0.004_072_046_8 * s;
    let a = 1.977_998_495_1 * l - 2.428_592_205 * m + 0.450_593_709_9 * s;
    let b = 0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766 * s;
    [
        lightness,
        a.hypot(b),
        (b.atan2(a).to_degrees() + 360.0) % 360.0,
    ]
}

pub fn readable_on(
    chroma: f64,
    hue: f64,
    backgrounds: &[&str],
    target: f64,
    from: f64,
    towards_light: bool,
) -> String {
    let step = if towards_light { 0.005 } else { -0.005 };
    (0..=200)
        .map(|index| (from + step * f64::from(index)).clamp(0.0, 1.0))
        .map(|lightness| oklch_hex(lightness, chroma, hue))
        .find(|candidate| {
            backgrounds
                .iter()
                .all(|background| contrast(candidate, background) >= target)
        })
        .unwrap_or_else(|| if towards_light { "#ffffff" } else { "#000000" }.to_string())
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

    #[test]
    fn oklch_round_trips_through_hex() {
        let [l, c, h] = oklch_of("#7aa2f7");
        assert_eq!(oklch_hex(l, c, h), "#7aa2f7");
    }

    #[test]
    fn oklch_knows_white_black_and_a_red() {
        assert_eq!(oklch_hex(1.0, 0.0, 0.0), "#ffffff");
        assert_eq!(oklch_hex(0.0, 0.0, 0.0), "#000000");
        let [l, c, h] = oklch_of("#ff0000");
        close(l, 0.628, 3);
        close(c, 0.258, 3);
        close(h, 29.23, 1);
    }

    #[test]
    fn oklch_lowers_chroma_to_stay_in_srgb() {
        let wanted = oklch_hex(0.95, 0.4, 260.0);
        let [l, c, _] = oklch_of(&wanted);
        close(l, 0.95, 2);
        assert!(c < 0.1, "{c}");
    }

    #[test]
    fn readable_on_meets_the_contrast_on_every_background() {
        let backgrounds = ["#101018", "#202030"];
        let text = readable_on(0.15, 50.0, &backgrounds, 4.5, 0.4, true);
        for background in backgrounds {
            assert!(contrast(&text, background) >= 4.5);
        }
        let dark = readable_on(0.15, 50.0, &["#fafafa"], 7.0, 0.7, false);
        assert!(contrast(&dark, "#fafafa") >= 7.0);
    }

    #[test]
    fn readable_on_stops_at_the_first_lightness_that_passes() {
        let text = readable_on(0.0, 0.0, &["#000000"], 4.5, 0.0, true);
        let ratio = contrast(&text, "#000000");
        assert!((4.5..4.8).contains(&ratio), "{ratio}");
    }
}
