use crate::text::is_js_space;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealPart {
    pub text: String,
    pub delay_ms: Option<u64>,
}

const WORD_STEP_MS: f64 = 34.0;
const WORD_BUDGET_MS: f64 = 560.0;

pub fn reveal_parts(text: &str) -> Vec<RevealPart> {
    let mut words: Vec<&str> = Vec::new();
    let mut gaps: Vec<&str> = Vec::new();
    let mut gap_start = 0;
    let mut word_start: Option<usize> = None;
    for (index, c) in text.char_indices() {
        match (is_js_space(c), word_start) {
            (false, None) => {
                gaps.push(&text[gap_start..index]);
                word_start = Some(index);
            }
            (true, Some(start)) => {
                words.push(&text[start..index]);
                word_start = None;
                gap_start = index;
            }
            _ => {}
        }
    }
    match word_start {
        Some(start) => words.push(&text[start..]),
        None => gaps.push(&text[gap_start..]),
    }
    let step = WORD_STEP_MS.min(WORD_BUDGET_MS / words.len().max(1) as f64);
    let mut parts = Vec::new();
    for (index, gap) in gaps.iter().enumerate() {
        if !gap.is_empty() {
            parts.push(RevealPart {
                text: (*gap).to_owned(),
                delay_ms: None,
            });
        }
        if let Some(word) = words.get(index) {
            parts.push(RevealPart {
                text: (*word).to_owned(),
                delay_ms: Some((index as f64 * step).round() as u64),
            });
        }
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(text: &str, delay: Option<u64>) -> RevealPart {
        RevealPart {
            text: text.to_owned(),
            delay_ms: delay,
        }
    }

    #[test]
    fn staggers_words_34_ms_apart_and_keeps_the_spaces_between_them() {
        assert_eq!(
            reveal_parts("Ship it today"),
            vec![
                part("Ship", Some(0)),
                part(" ", None),
                part("it", Some(34)),
                part(" ", None),
                part("today", Some(68)),
            ]
        );
    }

    #[test]
    fn keeps_leading_trailing_and_line_break_whitespace_as_is() {
        assert_eq!(
            reveal_parts(" a\n\nb "),
            vec![
                part(" ", None),
                part("a", Some(0)),
                part("\n\n", None),
                part("b", Some(34)),
                part(" ", None),
            ]
        );
    }

    #[test]
    fn fits_a_long_replacement_into_the_560_ms_budget() {
        let text = vec!["word"; 100].join(" ");
        let delays: Vec<u64> = reveal_parts(&text)
            .into_iter()
            .filter_map(|p| p.delay_ms)
            .collect();
        assert_eq!(delays.len(), 100);
        assert!(*delays.last().unwrap() <= 560);
        assert!(delays[1] < 34);
    }

    #[test]
    fn has_nothing_to_reveal_in_an_empty_text() {
        assert_eq!(reveal_parts(""), vec![]);
    }
}
