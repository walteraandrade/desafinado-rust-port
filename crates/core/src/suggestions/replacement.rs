use crate::text::{js_trim, js_trim_end, js_trim_start};

fn leading_whitespace(text: &str) -> &str {
    &text[..text.len() - js_trim_start(text).len()]
}

fn trailing_whitespace(text: &str) -> &str {
    if js_trim(text).is_empty() {
        ""
    } else {
        &text[js_trim_end(text).len()..]
    }
}

pub fn match_edge_whitespace(original: &str, replacement: &str) -> String {
    format!(
        "{}{}{}",
        leading_whitespace(original),
        js_trim(replacement),
        trailing_whitespace(original)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_the_replacement_untouched_when_the_original_has_no_edge_whitespace() {
        assert_eq!(match_edge_whitespace("Hello", "Hi there"), "Hi there");
    }

    #[test]
    fn restores_the_trailing_whitespace_of_the_original() {
        assert_eq!(
            match_edge_whitespace("Hello world. ", "Hi world."),
            "Hi world. "
        );
        assert_eq!(match_edge_whitespace("Hello\n", "Hi"), "Hi\n");
    }

    #[test]
    fn restores_the_leading_whitespace_of_the_original() {
        assert_eq!(match_edge_whitespace("  Hello", "Hi"), "  Hi");
    }

    #[test]
    fn replaces_the_edge_whitespace_the_model_added_or_changed() {
        assert_eq!(match_edge_whitespace("Hello", "\n\nHi \n"), "Hi");
        assert_eq!(match_edge_whitespace(" Hello ", "Hi\n"), " Hi ");
    }

    #[test]
    fn keeps_inner_whitespace_and_line_breaks() {
        assert_eq!(
            match_edge_whitespace("- a\n- b", "- one\n- two\n"),
            "- one\n- two"
        );
    }

    #[test]
    fn handles_an_original_made_only_of_whitespace() {
        assert_eq!(match_edge_whitespace("  ", "Hi"), "  Hi");
    }

    #[test]
    fn treats_tabs_and_carriage_returns_as_edge_whitespace() {
        assert_eq!(
            match_edge_whitespace("\t\r\nHello \t", "  Hi\n"),
            "\t\r\nHi \t"
        );
    }

    #[test]
    fn handles_an_empty_original_and_an_empty_replacement() {
        assert_eq!(match_edge_whitespace("", " Hi "), "Hi");
        assert_eq!(match_edge_whitespace(" x ", ""), "  ");
    }
}
