use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

/// Markdown and web text that holds no prose: code spans, link targets, autolinks, HTML
/// tags, bare URLs and e-mail addresses.
static NOT_PROSE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"`+[^`]*`+|\]\([^)]*\)|<[^>\s]+>|https?://\S+|www\.\S+|[\w.+-]+@[\w-]+\.[\w.]+")
        .expect("constant regex")
});

static WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\p{L}[\p{L}\p{M}]*(?:['’-][\p{L}\p{M}]+)*").expect("constant regex")
});

/// The byte ranges of the prose words in `text`. Words in code, links and addresses, words
/// that touch a digit or an underscore, and acronyms in capitals are left out.
pub fn words(text: &str) -> Vec<Range<usize>> {
    let skipped: Vec<Range<usize>> = NOT_PROSE.find_iter(text).map(|m| m.range()).collect();
    WORD.find_iter(text)
        .map(|m| m.range())
        .filter(|range| {
            !skipped
                .iter()
                .any(|skip| skip.start < range.end && range.start < skip.end)
        })
        .filter(|range| !touches_code_like(text, range))
        .filter(|range| !is_acronym(&text[range.clone()]))
        .collect()
}

fn touches_code_like(text: &str, range: &Range<usize>) -> bool {
    let code_like = |c: char| c.is_ascii_digit() || c == '_' || c == '/' || c == '\\';
    text[..range.start]
        .chars()
        .next_back()
        .is_some_and(code_like)
        || text[range.end..].chars().next().is_some_and(code_like)
}

fn is_acronym(word: &str) -> bool {
    word.chars().count() > 1 && word.chars().all(|c| !c.is_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(text: &str) -> Vec<&str> {
        words(text).into_iter().map(|range| &text[range]).collect()
    }

    #[test]
    fn keeps_accents_apostrophes_and_hyphens_inside_words() {
        assert_eq!(
            listed("Fazê-lo? Don't — guarda-chuva, ação."),
            vec!["Fazê-lo", "Don't", "guarda-chuva", "ação"]
        );
    }

    #[test]
    fn skips_code_links_and_addresses() {
        assert_eq!(
            listed("Use `fmtt` em [texto](http://a.b/cc) ou www.x.com, me@x.com <b>fim</b>"),
            vec!["Use", "em", "texto", "ou", "fim"]
        );
    }

    #[test]
    fn skips_acronyms_and_code_like_words() {
        assert_eq!(
            listed("A NASA usa x86 e foo_bar e a/b"),
            vec!["A", "usa", "e", "e"]
        );
    }
}
