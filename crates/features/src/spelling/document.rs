//! The spell checker over a whole Document: issues found per Block, kept by the Block's text
//! so an edit rechecks only the Blocks it changed, and the writer's own words.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::ops::Range;

use super::{Issue, IssueKind};

pub const WORDBOOK_FILE_NAME: &str = "dictionary.txt";

/// The issues of each Block checked so far, keyed by the Block's text.
pub type Checked = HashMap<String, Vec<Issue>>;

/// The Blocks whose text has not been checked yet.
pub fn unchecked<'a>(blocks: &'a [(usize, String)], checked: &Checked) -> Vec<&'a str> {
    let mut seen = HashSet::new();
    blocks
        .iter()
        .map(|(_, text)| text.as_str())
        .filter(|text| !checked.contains_key(*text) && seen.insert(*text))
        .collect()
}

/// The Blocks whose text has not been checked, each text once at its first place, in
/// Document order.
pub fn unchecked_at(blocks: &[(usize, String)], checked: &Checked) -> Vec<(usize, String)> {
    let mut seen = HashSet::new();
    blocks
        .iter()
        .filter(|(_, text)| !checked.contains_key(text) && seen.insert(text.as_str()))
        .cloned()
        .collect()
}

/// Takes up to `count` Blocks from `todo`, starting with the first one at or after `anchor`
/// (or the first of all when none is), and returns their texts. What the writer sees is
/// checked first.
pub fn take_chunk(todo: &mut Vec<(usize, String)>, anchor: usize, count: usize) -> Vec<String> {
    let start = todo.partition_point(|(from, _)| *from < anchor);
    let start = if start < todo.len() { start } else { 0 };
    let end = (start + count).min(todo.len());
    todo.drain(start..end).map(|(_, text)| text).collect()
}

/// Where the Blocks that have no check yet sit in the Document, in Document order.
pub fn pending_ranges(blocks: &[(usize, String)], checked: &Checked) -> Vec<Range<usize>> {
    blocks
        .iter()
        .filter(|(_, text)| !checked.contains_key(text))
        .map(|(from, text)| *from..*from + text.len())
        .collect()
}

/// The marks to show while a check is under way: the ones found so far, plus the marks already
/// shown inside Blocks still waiting for their check, so they do not blink out and back.
pub fn merge_progress(
    previous: &[Issue],
    pending: &[Range<usize>],
    mut found: Vec<Issue>,
) -> Vec<Issue> {
    if pending.is_empty() {
        return found;
    }
    found.extend(
        previous
            .iter()
            .filter(|issue| {
                let at = pending.partition_point(|range| range.end < issue.range.start);
                pending.get(at).is_some_and(|range| {
                    range.start <= issue.range.start && issue.range.end <= range.end
                })
            })
            .cloned(),
    );
    found.sort_by_key(|issue| (issue.range.start, issue.range.end));
    found
}

/// The issues of every Block, placed at the Block's offset in the Document.
pub fn place(blocks: &[(usize, String)], checked: &Checked) -> Vec<Issue> {
    blocks
        .iter()
        .filter_map(|(from, text)| checked.get(text).map(|issues| (*from, issues)))
        .flat_map(|(from, issues)| issues.iter().map(move |issue| issue.shifted(from)))
        .collect()
}

/// Forgets the Blocks that are no longer in the Document.
pub fn keep_only(checked: &mut Checked, blocks: &[(usize, String)]) {
    let current: HashSet<&str> = blocks.iter().map(|(_, text)| text.as_str()).collect();
    checked.retain(|text, _| current.contains(text.as_str()));
}

/// Issues after an edit that replaced `start..old_end` with text ending at `new_end`: the ones
/// the edit touched are gone until the next check, the ones after it move.
pub fn map_issues(issues: &[Issue], start: usize, old_end: usize, new_end: usize) -> Vec<Issue> {
    issues
        .iter()
        .filter(|issue| issue.range.end < start || issue.range.start > old_end)
        .map(|issue| {
            if issue.range.start > old_end {
                Issue {
                    range: issue.range.start + new_end - old_end
                        ..issue.range.end + new_end - old_end,
                    ..issue.clone()
                }
            } else {
                issue.clone()
            }
        })
        .collect()
}

/// What "Ignore" remembers: a misspelled word everywhere, a grammar issue only for the same
/// words and the same rule.
pub fn ignore_key(issue: &Issue, words: &str) -> String {
    match issue.kind {
        IssueKind::Spelling => words.to_lowercase(),
        IssueKind::Grammar => format!("{words}\u{0}{}", issue.message),
    }
}

/// The issues to show: not on a word the writer added or ignored, and not inside text an AI
/// Suggestion already covers.
pub fn shown(
    issues: &[Issue],
    word_of: impl Fn(&Range<usize>) -> String,
    wordbook: &Wordbook,
    ignored: &HashSet<String>,
    covered: &[Range<usize>],
) -> Vec<Issue> {
    issues
        .iter()
        .filter(|issue| {
            let words = word_of(&issue.range);
            let added = issue.kind == IssueKind::Spelling && wordbook.contains(&words);
            !added && !ignored.contains(&ignore_key(issue, &words))
        })
        .filter(|issue| {
            !covered
                .iter()
                .any(|range| range.start < issue.range.end && issue.range.start < range.end)
        })
        .cloned()
        .collect()
}

/// The issue under `offset`, the shortest when several hold it.
pub fn issue_at(issues: &[Issue], offset: usize) -> Option<usize> {
    issues
        .iter()
        .enumerate()
        .filter(|(_, issue)| issue.range.start <= offset && offset <= issue.range.end)
        .min_by_key(|(_, issue)| issue.range.len())
        .map(|(index, _)| index)
}

/// The issue whose menu opens with the caret at `selection`: none while text is selected, and
/// with `strictly_inside` only one the caret is inside of, not at an edge. The shortest wins.
pub fn issue_to_open(
    issues: &[Issue],
    selection: &Range<usize>,
    strictly_inside: bool,
) -> Option<usize> {
    if !selection.is_empty() {
        return None;
    }
    let caret = selection.start;
    issues
        .iter()
        .enumerate()
        .filter(|(_, issue)| {
            if strictly_inside {
                issue.range.start < caret && caret < issue.range.end
            } else {
                issue.range.start <= caret && caret <= issue.range.end
            }
        })
        .min_by_key(|(_, issue)| issue.range.len())
        .map(|(index, _)| index)
}

/// The next issue starting after `offset`, or the previous one starting before it, wrapping
/// at the ends. `issues` are in Document order. Stepping from an issue's own start moves on.
pub fn step_issue(issues: &[Issue], offset: usize, backward: bool) -> Option<usize> {
    if issues.is_empty() {
        return None;
    }
    if backward {
        issues
            .iter()
            .rposition(|issue| issue.range.start < offset)
            .or(Some(issues.len() - 1))
    } else {
        issues
            .iter()
            .position(|issue| issue.range.start > offset)
            .or(Some(0))
    }
}

/// The writer's own words, which are never misspelled. Saved one per line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Wordbook {
    words: BTreeSet<String>,
}

impl Wordbook {
    pub fn parse(text: &str) -> Self {
        Self {
            words: text
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_lowercase)
                .collect(),
        }
    }

    pub fn contains(&self, word: &str) -> bool {
        self.words.contains(&word.to_lowercase())
    }

    #[must_use]
    pub fn with(&self, word: &str) -> Self {
        let mut words = self.words.clone();
        words.insert(word.trim().to_lowercase());
        Self { words }
    }

    pub fn to_text(&self) -> String {
        self.words.iter().fold(String::new(), |mut text, word| {
            text.push_str(word);
            text.push('\n');
            text
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(range: Range<usize>, kind: IssueKind) -> Issue {
        Issue {
            range,
            kind,
            message: "m".into(),
            replacements: vec![],
        }
    }

    fn word_in(text: &str) -> impl Fn(&Range<usize>) -> String + '_ {
        move |range| text[range.clone()].to_string()
    }

    fn blocks(list: &[(usize, &str)]) -> Vec<(usize, String)> {
        list.iter()
            .map(|(from, text)| (*from, (*text).to_string()))
            .collect()
    }

    #[test]
    fn only_new_block_texts_need_a_check() {
        let blocks = blocks(&[(0, "one"), (5, "two"), (10, "one")]);
        let mut checked = Checked::new();
        checked.insert("two".into(), vec![]);
        assert_eq!(unchecked(&blocks, &checked), vec!["one"]);
    }

    #[test]
    fn chunks_start_at_the_anchor_and_wrap_to_the_beginning() {
        let blocks = blocks(&[(0, "a"), (10, "b"), (20, "c"), (30, "b"), (40, "d")]);
        let mut checked = Checked::new();
        checked.insert("c".into(), vec![]);
        let mut todo = unchecked_at(&blocks, &checked);
        assert_eq!(todo.len(), 3);
        assert_eq!(take_chunk(&mut todo, 5, 2), vec!["b", "d"]);
        assert_eq!(take_chunk(&mut todo, 45, 2), vec!["a"]);
        assert!(take_chunk(&mut todo, 0, 2).is_empty());
    }

    #[test]
    fn pending_ranges_cover_the_blocks_without_a_check() {
        let blocks = blocks(&[(0, "one"), (10, "two"), (20, "one")]);
        let mut checked = Checked::new();
        checked.insert("one".into(), vec![]);
        assert_eq!(pending_ranges(&blocks, &checked), vec![10..13]);
    }

    #[test]
    fn progress_keeps_shown_marks_inside_blocks_still_pending() {
        let previous = vec![
            issue(2..4, IssueKind::Spelling),
            issue(12..14, IssueKind::Spelling),
            issue(25..27, IssueKind::Spelling),
        ];
        let found = vec![issue(22..24, IssueKind::Grammar)];
        let merged: Vec<_> = merge_progress(&previous, &[10..16, 20..30], found)
            .into_iter()
            .map(|i| i.range)
            .collect();
        assert_eq!(merged, vec![12..14, 22..24, 25..27]);
    }

    #[test]
    fn progress_with_nothing_pending_is_what_was_found() {
        let previous = vec![issue(2..4, IssueKind::Spelling)];
        let found = vec![issue(8..9, IssueKind::Spelling)];
        assert_eq!(merge_progress(&previous, &[], found.clone()), found);
    }

    #[test]
    fn places_block_issues_at_each_block() {
        let blocks = blocks(&[(0, "teh cat"), (20, "teh cat")]);
        let mut checked = Checked::new();
        checked.insert("teh cat".into(), vec![issue(0..3, IssueKind::Spelling)]);
        let placed: Vec<_> = place(&blocks, &checked)
            .into_iter()
            .map(|i| i.range)
            .collect();
        assert_eq!(placed, vec![0..3, 20..23]);
    }

    #[test]
    fn keep_only_forgets_blocks_gone_from_the_document() {
        let mut checked = Checked::new();
        checked.insert("old".into(), vec![]);
        checked.insert("new".into(), vec![]);
        keep_only(&mut checked, &blocks(&[(0, "new")]));
        assert_eq!(checked.keys().collect::<Vec<_>>(), vec!["new"]);
    }

    #[test]
    fn an_edit_drops_touched_issues_and_moves_later_ones() {
        let issues = vec![
            issue(0..3, IssueKind::Spelling),
            issue(10..14, IssueKind::Spelling),
            issue(20..25, IssueKind::Grammar),
        ];
        let mapped: Vec<_> = map_issues(&issues, 12, 13, 16)
            .into_iter()
            .map(|i| i.range)
            .collect();
        assert_eq!(mapped, vec![0..3, 23..28]);
    }

    #[test]
    fn a_deletion_before_issues_moves_them_back() {
        let issues = vec![
            issue(10..14, IssueKind::Spelling),
            issue(20..25, IssueKind::Grammar),
        ];
        let mapped: Vec<_> = map_issues(&issues, 2, 5, 2)
            .into_iter()
            .map(|i| i.range)
            .collect();
        assert_eq!(mapped, vec![7..11, 17..22]);
    }

    #[test]
    fn typing_right_before_a_word_drops_its_issue() {
        let issues = vec![issue(4..7, IssueKind::Spelling)];
        assert_eq!(map_issues(&issues, 4, 4, 5), vec![]);
    }

    #[test]
    fn typing_right_after_a_word_drops_its_issue() {
        let issues = vec![issue(4..7, IssueKind::Spelling)];
        assert_eq!(map_issues(&issues, 7, 7, 8), vec![]);
    }

    #[test]
    fn hides_wordbook_words_ignored_issues_and_covered_text() {
        let text = "teh Lisboa has has";
        let issues = vec![
            issue(0..3, IssueKind::Spelling),
            issue(4..10, IssueKind::Spelling),
            issue(11..14, IssueKind::Grammar),
            issue(15..18, IssueKind::Grammar),
        ];
        let wordbook = Wordbook::parse("lisboa\n");
        let ignored = HashSet::from([ignore_key(&issues[0], "teh")]);
        let shown: Vec<_> = shown(
            &issues,
            word_in(text),
            &wordbook,
            &ignored,
            std::slice::from_ref(&(14..16)),
        )
        .into_iter()
        .map(|i| i.range)
        .collect();
        assert_eq!(shown, vec![11..14]);
    }

    #[test]
    fn the_wordbook_does_not_hide_grammar() {
        let text = "has";
        let issues = vec![issue(0..3, IssueKind::Grammar)];
        let wordbook = Wordbook::parse("has");
        assert_eq!(
            shown(&issues, word_in(text), &wordbook, &HashSet::new(), &[]).len(),
            1
        );
    }

    #[test]
    fn ignore_keys_grammar_by_its_words_and_spelling_by_the_word_in_any_case() {
        let grammar = issue(0..3, IssueKind::Grammar);
        assert_ne!(ignore_key(&grammar, "has"), ignore_key(&grammar, "have"));
        assert_eq!(
            ignore_key(&issue(0..3, IssueKind::Spelling), "Teh"),
            ignore_key(&issue(4..7, IssueKind::Spelling), "teh")
        );
    }

    #[test]
    fn issue_at_picks_the_shortest_issue_holding_the_offset() {
        let issues = vec![
            issue(0..10, IssueKind::Grammar),
            issue(4..7, IssueKind::Spelling),
        ];
        assert_eq!(issue_at(&issues, 5), Some(1));
        assert_eq!(issue_at(&issues, 1), Some(0));
        assert_eq!(issue_at(&issues, 7), Some(1));
        assert_eq!(issue_at(&issues, 11), None);
    }

    #[test]
    fn opens_the_shortest_issue_the_caret_reaches_and_none_over_a_selection() {
        let issues = vec![
            issue(0..10, IssueKind::Grammar),
            issue(4..7, IssueKind::Spelling),
        ];
        assert_eq!(issue_to_open(&issues, &(5..5), true), Some(1));
        assert_eq!(issue_to_open(&issues, &(7..7), false), Some(1));
        assert_eq!(issue_to_open(&issues, &(7..7), true), Some(0));
        assert_eq!(issue_to_open(&issues, &(10..10), true), None);
        assert_eq!(issue_to_open(&issues, &(10..10), false), Some(0));
        assert_eq!(issue_to_open(&issues, &(5..6), false), None);
    }

    #[test]
    fn steps_through_issues_and_wraps() {
        let issues = vec![
            issue(2..4, IssueKind::Spelling),
            issue(10..12, IssueKind::Spelling),
        ];
        assert_eq!(step_issue(&issues, 0, false), Some(0));
        assert_eq!(step_issue(&issues, 3, false), Some(1));
        assert_eq!(step_issue(&issues, 11, false), Some(0));
        assert_eq!(step_issue(&issues, 2, false), Some(1));
        assert_eq!(step_issue(&issues, 10, true), Some(0));
        assert_eq!(step_issue(&issues, 11, true), Some(1));
        assert_eq!(step_issue(&issues, 2, true), Some(1));
        assert_eq!(step_issue(&[], 1, true), None);
    }

    #[test]
    fn the_wordbook_ignores_case_and_saves_one_word_per_line() {
        let book = Wordbook::parse("Desafinado\n\n  GPUI \n").with("Totoro");
        assert!(book.contains("desafinado"));
        assert!(book.contains("TOTORO"));
        assert_eq!(book.to_text(), "desafinado\ngpui\ntotoro\n");
        assert_eq!(Wordbook::parse(&book.to_text()), book);
    }
}
