//! The spell checker: Harper checks English spelling and grammar, Spellbook checks Portuguese
//! spelling with the `LibreOffice` pt-BR dictionary. Each Block is checked on its own, in the
//! language most of its words belong to.

pub mod document;
mod english;
pub mod pass;
mod portuguese;
pub mod settings;
mod words;
pub mod worker;

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

    /// The language of `text`, or `None` when it has no words. Only words one dictionary
    /// knows and the other does not count, plus the short Portuguese words English text never
    /// uses: English knows "o", "do" and "que" as names, and Portuguese prose borrows "app" and
    /// "deploy".
    pub fn language_of(&self, text: &str) -> Option<Language> {
        let found = words(text);
        if found.is_empty() {
            return None;
        }
        let (mut english, mut portuguese) = (0usize, 0usize);
        for range in found {
            let word = &text[range];
            if PORTUGUESE_ONLY.contains(&word.to_lowercase().as_str()) {
                portuguese += 1;
                continue;
            }
            match (self.english.knows(word), self.portuguese.knows(word)) {
                (true, false) => english += 1,
                (false, true) => portuguese += 1,
                _ => {}
            }
        }
        Some(pick_language(english, portuguese, looks_portuguese(text)))
    }

    /// The issues in one Block of Markdown, ordered by where they start.
    pub fn check(&mut self, text: &str) -> Vec<Issue> {
        let mut issues = match self.language_of(text) {
            None => Vec::new(),
            Some(Language::English) => self.english.check(text),
            Some(Language::Portuguese) => {
                let english = &self.english;
                self.portuguese.check(text, |word| english.knows(word))
            }
        };
        issues.sort_by_key(|issue| (issue.range.start, issue.range.end));
        issues
    }
}

/// Short Portuguese words that the English dictionary also lists, but that English prose does
/// not use as words. "a", "as", "do", "no" and "se" are left out: English uses them.
const PORTUGUESE_ONLY: &[&str] = &[
    "o", "os", "da", "das", "dos", "que", "em", "um", "uma", "não", "para", "com", "por", "mas",
    "é", "ao", "na", "nas", "nos", "ele", "ela", "eu", "você",
];

fn pick_language(english_only: usize, portuguese_only: usize, accents: bool) -> Language {
    if portuguese_only > english_only || (portuguese_only == english_only && accents) {
        Language::Portuguese
    } else {
        Language::English
    }
}

fn looks_portuguese(text: &str) -> bool {
    text.chars()
        .any(|c| matches!(c, 'ã' | 'õ' | 'ç' | 'Ã' | 'Õ' | 'Ç'))
}

/// The replacements to offer: each once, in order, at most [`MAX_REPLACEMENTS`].
fn offered(candidates: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut list = Vec::new();
    for item in candidates {
        if list.len() < MAX_REPLACEMENTS && !list.contains(&item) {
            list.push(item);
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{LazyLock, Mutex};

    static SPELLER: LazyLock<Mutex<Speller>> =
        LazyLock::new(|| Mutex::new(Speller::new().expect("dictionaries load")));

    fn check(text: &str) -> Vec<Issue> {
        SPELLER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .check(text)
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
    fn english_ranges_are_bytes_after_accented_text() {
        let text = "The café owner, naïve as ever, had a belive in luck.";
        let issues = check(text);
        let belive = issue_at(&issues, text, "belive");
        assert_eq!(&text[belive.range.clone()], "belive");
    }

    #[test]
    fn the_best_replacement_leads_when_several_rules_hit_a_word() {
        let text = "They has belive the plan.";
        let issues = check(text);
        let belive = issue_at(&issues, text, "belive");
        assert_eq!(
            belive.replacements.first().map(String::as_str),
            Some("believe")
        );
    }

    #[test]
    fn style_advice_is_not_a_mistake() {
        let long = "The committee, which had met every week for the better part of two years in the same cold room above the old bakery on the corner, finally agreed after a great deal of discussion and several false starts that the bridge should be repaired before the winter came.";
        assert_eq!(check(long), vec![]);
    }

    #[test]
    fn issues_come_in_document_order() {
        let issues = check("Teh dog and teh cat were comming home.");
        let starts: Vec<usize> = issues.iter().map(|issue| issue.range.start).collect();
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(starts, sorted);
        assert!(starts.len() >= 3, "{issues:?}");
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
    fn portuguese_with_english_loanwords_stays_portuguese() {
        let text = "Fiz o deploy do app ontem e o build passou.";
        assert_eq!(
            SPELLER
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .language_of(text),
            Some(Language::Portuguese)
        );
        assert_eq!(check(text), vec![]);
        assert_eq!(check("O app quebrou no login de novo."), vec![]);
    }

    #[test]
    fn a_curly_apostrophe_is_an_apostrophe() {
        assert_eq!(check("Um copo d’água e outro d'água."), vec![]);
    }

    #[test]
    fn a_tie_goes_to_portuguese_only_with_its_accents() {
        assert_eq!(pick_language(2, 1, false), Language::English);
        assert_eq!(pick_language(1, 2, false), Language::Portuguese);
        assert_eq!(pick_language(1, 1, true), Language::Portuguese);
        assert_eq!(pick_language(0, 0, false), Language::English);
    }

    #[test]
    fn offered_keeps_order_drops_repeats_and_stops_at_five() {
        let list = offered(["a", "b", "a", "c", "d", "e", "f"].map(String::from));
        assert_eq!(list, ["a", "b", "c", "d", "e"].map(String::from));
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
        let speller = SPELLER
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
