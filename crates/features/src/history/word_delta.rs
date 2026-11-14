use super::line_runs::{LineRun, line_runs};
use crate::diff::{Edit, diff_tokens, significant_tokens};
use crate::text::count_words;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WordDelta {
    pub added: usize,
    pub removed: usize,
}

const MAX_WORD_EDITS: usize = 400;

fn run_delta(removed: &str, added: &str) -> WordDelta {
    let old = significant_tokens(removed);
    let new = significant_tokens(added);
    let parts = diff_tokens(&old, &new);
    let edits: usize = parts
        .iter()
        .filter(|(edit, ..)| *edit != Edit::Equal)
        .map(|(_, _, len)| len)
        .sum();
    if edits > MAX_WORD_EDITS {
        return WordDelta {
            added: count_words(added),
            removed: count_words(removed),
        };
    }
    parts.iter().fold(
        WordDelta {
            added: 0,
            removed: 0,
        },
        |delta, &(edit, index, len)| match edit {
            Edit::Equal => delta,
            Edit::Removed => WordDelta {
                removed: delta.removed
                    + count_words(&removed[old[index].from..old[index + len - 1].to]),
                ..delta
            },
            Edit::Added => WordDelta {
                added: delta.added + count_words(&added[new[index].from..new[index + len - 1].to]),
                ..delta
            },
        },
    )
}

pub fn word_delta(before: &str, after: &str) -> WordDelta {
    line_runs(before, after).iter().fold(
        WordDelta {
            added: 0,
            removed: 0,
        },
        |total, run| match run {
            LineRun::Same { .. } => total,
            LineRun::Changed { removed, added } => {
                let delta = run_delta(removed, added);
                WordDelta {
                    added: total.added + delta.added,
                    removed: total.removed + delta.removed,
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delta(added: usize, removed: usize) -> WordDelta {
        WordDelta { added, removed }
    }

    #[test]
    fn counts_words_added_and_removed() {
        assert_eq!(word_delta("one two three", "one 2 three four"), delta(2, 1));
    }

    #[test]
    fn counts_every_word_as_added_against_an_empty_text() {
        assert_eq!(word_delta("", "one two three"), delta(3, 0));
    }

    #[test]
    fn counts_every_word_as_removed_against_an_empty_text() {
        assert_eq!(word_delta("one two", ""), delta(0, 2));
    }

    #[test]
    fn returns_zeros_when_only_spacing_changed() {
        assert_eq!(word_delta("one two", "one  two\n"), delta(0, 0));
    }

    #[test]
    fn counts_only_the_lines_that_changed() {
        assert_eq!(
            word_delta(
                "Same first line.\n\nOld middle words.\n\nSame last line.",
                "Same first line.\n\nNew middle words.\n\nSame last line.",
            ),
            delta(1, 1)
        );
    }

    #[test]
    fn counts_a_wholesale_replacement_as_every_word_removed_and_added_quickly() {
        let prose = |seed: &str, count: usize| {
            (0..count)
                .map(|index| format!("{seed}{index}a {seed}{index}b {seed}{index}c."))
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        let before = prose("alpha", 600);
        let after = prose("beta", 400);
        let started = std::time::Instant::now();
        let result = word_delta(&before, &after);
        assert!(started.elapsed().as_millis() < 500);
        assert_eq!(
            result,
            delta(
                after.split_whitespace().count(),
                before.split_whitespace().count()
            )
        );
    }
}
