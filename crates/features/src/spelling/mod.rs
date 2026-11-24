//! The spell checker: Harper checks English spelling and grammar, Spellbook checks Portuguese
//! spelling with the `LibreOffice` pt-BR dictionary. Each Block is checked on its own, in the
//! language most of its words belong to.

pub mod document;
mod english;
mod portuguese;
mod words;

use std::ops::Range;

pub use words::words;

const MAX_REPLACEMENTS: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Language {
    English,
    Portuguese,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IssueKind {
    Spelling,
    Grammar,
}

/// A problem in a piece of text, with the replacements that would fix it. An empty
/// replacement removes the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    /// Byte range in the text that was checked.
    pub range: Range<usize>,
    pub kind: IssueKind,
    pub message: String,
    pub replacements: Vec<String>,
}

impl Issue {
    #[must_use]
    pub fn shifted(&self, by: usize) -> Self {
        Self {
            range: self.range.start + by..self.range.end + by,
            ..self.clone()
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SpellerError {
    #[error("the Portuguese dictionary could not be read: {0}")]
    Dictionary(String),
}

pub struct Speller {
    english: english::English,
    portuguese: portuguese::Portuguese,
}

impl Speller {
    /// Loads both languages. It takes most of a second, so call it off the UI thread.
    pub fn new() -> Result<Self, SpellerError> {
        Ok(Self {
            english: english::English::new(),
            portuguese: portuguese::Portuguese::new()?,
        })
    }

    /// The language most words of `text` belong to, or `None` when it has no words.
    pub fn language_of(&self, text: &str) -> Option<Language> {
        let found = words(text);
        if found.is_empty() {
            return None;
        }
        let (mut english, mut portuguese) = (0usize, 0usize);
        for range in found {
            let word = &text[range];
            english += usize::from(self.english.knows(word));
            portuguese += usize::from(self.portuguese.knows(word));
        }
        let portuguese_wins =
            portuguese > english || (portuguese == english && looks_portuguese(text));
        Some(if portuguese_wins {
            Language::Portuguese
        } else {
            Language::English
        })
    }

    /// The issues in one Block of Markdown, ordered by where they start.
    pub fn check(&mut self, text: &str) -> Vec<Issue> {
        let mut issues = match self.language_of(text) {
            None => Vec::new(),
            Some(Language::English) => self.english.check(text),
            Some(Language::Portuguese) => self.portuguese.check(text),
        };
        issues.sort_by_key(|issue| (issue.range.start, issue.range.end));
        issues
    }
}

fn looks_portuguese(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(c, 'ã' | 'õ' | 'ç' | 'Ã' | 'Õ' | 'Ç'))
}

fn push_unique(list: &mut Vec<String>, item: String) {
    if list.len() < MAX_REPLACEMENTS && !list.contains(&item) {
        list.push(item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{LazyLock, Mutex};

    static SPELLER: LazyLock<Mutex<Speller>> =
        LazyLock::new(|| Mutex::new(Speller::new().expect("dictionaries load")));

    fn check(text: &str) -> Vec<Issue> {
        SPELLER.lock().expect("speller lock").check(text)
    }

    fn issue_at<'a>(issues: &'a [Issue], text: &str, word: &str) -> &'a Issue {
        let start = text.find(word).expect("word in text");
        issues
            .iter()
            .find(|issue| issue.range == (start..start + word.len()))
            .unwrap_or_else(|| panic!("no issue on {word:?} in {issues:?}"))
    }

    #[test]
    fn finds_english_typos_with_the_right_replacement() {
        let text = "I belive that the comming years will show he was a genious.";
        let issues = check(text);
        for (word, fix) in [
            ("belive", "believe"),
            ("comming", "coming"),
            ("genious", "genius"),
        ] {
            let issue = issue_at(&issues, text, word);
            assert_eq!(issue.kind, IssueKind::Spelling);
            assert_eq!(
                issue.replacements.first().map(String::as_str),
                Some(fix),
                "{word}: {:?}",
                issue.replacements
            );
        }
    }

    #[test]
    fn finds_english_grammar() {
        let text = "This idea could of worked, and they has the money.";
        let issues = check(text);
        let could = issue_at(&issues, text, "could of");
        assert_eq!(could.kind, IssueKind::Grammar);
        assert_eq!(
            could.replacements.first().map(String::as_str),
            Some("could have")
        );
        assert_eq!(issue_at(&issues, text, "has").kind, IssueKind::Grammar);
    }

    #[test]
    fn one_issue_per_range_with_the_replacements_of_every_rule() {
        let text = "They have alot of money.";
        let issues = check(text);
        let start = text.find("alot").expect("alot");
        let on_alot: Vec<_> = issues
            .iter()
            .filter(|issue| issue.range == (start..start + 4))
            .collect();
        assert_eq!(on_alot.len(), 1, "{issues:?}");
        assert!(on_alot[0].replacements.iter().any(|r| r == "a lot"));
    }

    #[test]
    fn headings_keep_their_own_capitalization() {
        assert_eq!(check("# On loyalty"), vec![]);
    }

    #[test]
    fn correct_english_has_no_issues() {
        assert_eq!(check("The friends who stay are few."), vec![]);
    }

    #[test]
    fn finds_portuguese_typos_with_the_right_replacement() {
        let text = "Ele não sabia que a excessão era tão grande, mas concerteza vai dar certo.";
        let issues = check(text);
        let excessao = issue_at(&issues, text, "excessão");
        assert_eq!(excessao.kind, IssueKind::Spelling);
        assert!(excessao.replacements.iter().any(|r| r == "exceção"));
        let certeza = issue_at(&issues, text, "concerteza");
        assert!(certeza.replacements.iter().any(|r| r == "com certeza"));
        assert!(
            excessao.replacements.iter().all(|r| !r.starts_with("e ")),
            "{:?}",
            excessao.replacements
        );
        assert_eq!(issues.len(), 2, "{issues:?}");
    }

    #[test]
    fn portuguese_ranges_are_bytes_after_accented_text() {
        let text = "Ação e emoção nao faltam.";
        let issues = check(text);
        let nao = issue_at(&issues, text, "nao");
        assert_eq!(&text[nao.range.clone()], "nao");
        assert!(nao.replacements.iter().any(|r| r == "não"));
    }

    #[test]
    fn correct_portuguese_has_no_issues() {
        assert_eq!(
            check("Os meninos foram para a escola e encontraram o professor."),
            vec![]
        );
    }

    #[test]
    fn code_links_and_acronyms_are_not_words() {
        let text = "Rode `cargo fmtt` e veja https://exemplo.com/caminhoo, ou [aqui](http://xpto.br/abcd). A NASA e o IBGE sabem.";
        assert_eq!(check(text), vec![]);
    }

    #[test]
    fn english_code_spans_are_skipped() {
        assert_eq!(
            check("Run `cargo fmtt` before you push the branch."),
            vec![]
        );
    }

    #[test]
    fn tells_english_from_portuguese() {
        let speller = SPELLER.lock().expect("speller lock");
        assert_eq!(
            speller.language_of("The years will show he was right."),
            Some(Language::English)
        );
        assert_eq!(
            speller.language_of("Os anos vão mostrar que ele tinha razão."),
            Some(Language::Portuguese)
        );
        assert_eq!(speller.language_of("--- ** ---"), None);
    }

    #[test]
    fn shifted_moves_the_range_only() {
        let issue = Issue {
            range: 2..5,
            kind: IssueKind::Spelling,
            message: "m".into(),
            replacements: vec!["x".into()],
        };
        let moved = issue.shifted(10);
        assert_eq!(moved.range, 12..15);
        assert_eq!(moved.replacements, issue.replacements);
    }
}
