use std::sync::Arc;

use harper_core::linting::{Lint, LintGroup, LintKind, Linter, Suggestion};
use harper_core::spell::{Dictionary, FstDictionary};
use harper_core::{Dialect, Document};

use super::{Issue, IssueKind, push_unique};

/// Rules that enforce one house style rather than catch a mistake.
const HOUSE_STYLE: &[&str] = &["UseTitleCase"];

pub struct English {
    dictionary: Arc<FstDictionary>,
    linter: LintGroup,
}

impl English {
    pub fn new() -> Self {
        let dictionary = FstDictionary::curated();
        let mut linter = LintGroup::new_curated(Arc::clone(&dictionary), Dialect::American);
        for rule in HOUSE_STYLE {
            linter.config.set_rule_enabled(*rule, false);
        }
        Self { dictionary, linter }
    }

    pub fn knows(&self, word: &str) -> bool {
        self.dictionary.contains_word_str(word)
            || self.dictionary.contains_word_str(&word.to_lowercase())
    }

    /// Harper's own order puts a rare word first now and then ("belive" → "belie"), so
    /// common words go first and vulgar ones last.
    fn rank(&self, candidate: &str) -> u8 {
        match self.dictionary.get_word_metadata_str(candidate) {
            Some(meta) if meta.swear == Some(true) => 2,
            Some(meta) if meta.common => 0,
            _ => 1,
        }
    }

    pub fn check(&mut self, text: &str) -> Vec<Issue> {
        let document = Document::new_markdown_default_curated(text);
        let mut lints: Vec<Lint> = self
            .linter
            .lint(&document)
            .into_iter()
            .filter(|lint| !is_advice(lint.lint_kind))
            .collect();
        lints.sort_by_key(|lint| lint.priority);
        let bytes = char_to_byte(text);
        let mut issues: Vec<Issue> = Vec::new();
        for lint in lints {
            let (Some(&start), Some(&end)) = (bytes.get(lint.span.start), bytes.get(lint.span.end))
            else {
                continue;
            };
            if start >= end {
                continue;
            }
            let original = &text[start..end];
            let kind = kind_of(lint.lint_kind);
            let index = issues
                .iter()
                .position(|issue| issue.range == (start..end))
                .unwrap_or_else(|| {
                    issues.push(Issue {
                        range: start..end,
                        kind,
                        message: lint.message.clone(),
                        replacements: Vec::new(),
                    });
                    issues.len() - 1
                });
            let issue = &mut issues[index];
            if kind == IssueKind::Grammar {
                issue.kind = IssueKind::Grammar;
            }
            let mut replacements: Vec<String> = lint
                .suggestions
                .iter()
                .map(|suggestion| replacement(original, suggestion))
                .collect();
            if kind == IssueKind::Spelling {
                replacements.sort_by_key(|candidate| {
                    (self.rank(candidate), !drops_one_letter(original, candidate))
                });
            }
            for candidate in replacements {
                push_unique(&mut issue.replacements, candidate);
            }
        }
        issues
    }
}

/// Whether `candidate` is `typo` with one letter fewer, the most common slip of the fingers
/// ("comming" → "coming", ahead of "cumming").
fn drops_one_letter(typo: &str, candidate: &str) -> bool {
    let typo: Vec<char> = typo.chars().collect();
    let candidate: Vec<char> = candidate.chars().collect();
    typo.len() == candidate.len() + 1
        && (0..typo.len()).any(|skip| {
            typo.iter()
                .enumerate()
                .filter(|(index, _)| *index != skip)
                .map(|(_, c)| *c)
                .eq(candidate.iter().copied())
        })
}

/// Lints that suggest a better style rather than fix a mistake.
fn is_advice(kind: LintKind) -> bool {
    matches!(
        kind,
        LintKind::Readability | LintKind::Style | LintKind::Enhancement
    )
}

fn kind_of(kind: LintKind) -> IssueKind {
    match kind {
        LintKind::Spelling | LintKind::Typo => IssueKind::Spelling,
        _ => IssueKind::Grammar,
    }
}

fn replacement(original: &str, suggestion: &Suggestion) -> String {
    match suggestion {
        Suggestion::ReplaceWith(chars) => chars.iter().collect(),
        Suggestion::InsertAfter(chars) => original.chars().chain(chars.iter().copied()).collect(),
        Suggestion::Remove => String::new(),
    }
}

/// The byte offset of every char index of `text`, with one past the end.
fn char_to_byte(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(byte, _)| byte)
        .chain(std::iter::once(text.len()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::drops_one_letter;

    #[test]
    fn a_doubled_or_extra_letter_is_one_letter_dropped() {
        assert!(drops_one_letter("comming", "coming"));
        assert!(drops_one_letter("genious", "genius"));
        assert!(!drops_one_letter("comming", "cumming"));
        assert!(!drops_one_letter("belive", "believe"));
    }
}
