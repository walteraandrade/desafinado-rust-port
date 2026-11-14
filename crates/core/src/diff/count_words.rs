use crate::text::is_js_whitespace;

pub fn count_words(text: &str) -> usize {
    text.split(is_js_whitespace)
        .filter(|word| !word.is_empty())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_runs_of_non_space_characters() {
        assert_eq!(count_words("one  two\nthree\t four"), 4);
    }

    #[test]
    fn counts_nothing_in_blank_text() {
        assert_eq!(count_words(""), 0);
        assert_eq!(count_words(" \n\t "), 0);
    }
}
