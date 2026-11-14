pub fn is_js_space(c: char) -> bool {
    match c {
        '\u{85}' => false,
        '\u{feff}' => true,
        c => c.is_whitespace(),
    }
}

pub fn count_words(text: &str) -> usize {
    text.split(is_js_space)
        .filter(|word| !word.is_empty())
        .count()
}

pub fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

pub fn char_count(text: &str) -> usize {
    text.chars().count()
}

pub fn to_fixed(value: f64, digits: usize) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    let long = format!("{:.*}", digits + 40, value.abs());
    let (whole, fraction) = long.split_once('.').unwrap_or((&long, ""));
    let (kept, rest) = fraction.split_at(digits.min(fraction.len()));
    let round_up = rest.as_bytes().first().is_some_and(|digit| *digit >= b'5');
    let mut digits_vec: Vec<u8> = format!("{whole}{kept}").into_bytes();
    if round_up {
        let mut index = digits_vec.len();
        loop {
            if index == 0 {
                digits_vec.insert(0, b'1');
                break;
            }
            index -= 1;
            if digits_vec[index] == b'9' {
                digits_vec[index] = b'0';
            } else {
                digits_vec[index] += 1;
                break;
            }
        }
    }
    let all = String::from_utf8(digits_vec).unwrap_or_default();
    let split = all.len() - kept.len();
    let (int_part, frac_part) = all.split_at(split);
    let sign = if value < 0.0 && all.bytes().any(|b| b != b'0') {
        "-"
    } else {
        ""
    };
    if digits == 0 {
        format!("{sign}{int_part}")
    } else {
        format!("{sign}{int_part}.{frac_part}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_words_by_runs_of_non_space() {
        assert_eq!(count_words("one  two\nthree\t four"), 4);
    }

    #[test]
    fn counts_nothing_in_blank_text() {
        assert_eq!(count_words(""), 0);
        assert_eq!(count_words(" \n\t "), 0);
    }

    #[test]
    fn to_fixed_rounds_half_up_like_javascript() {
        assert_eq!(to_fixed(0.125, 2), "0.13");
        assert_eq!(to_fixed(0.00265497, 4), "0.0027");
        assert_eq!(to_fixed(0.5, 2), "0.50");
        assert_eq!(to_fixed(0.3, 4), "0.3000");
        assert_eq!(to_fixed(9.999, 2), "10.00");
        assert_eq!(to_fixed(0.0, 4), "0.0000");
    }
}
