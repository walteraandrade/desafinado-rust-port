use std::time::{Duration, Instant};

use similar::{Algorithm, DiffOp, capture_diff_slices_deadline};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordDelta {
    pub added: i64,
    pub removed: i64,
}

const MAX_WORD_EDITS: usize = 400;
const DIFF_BUDGET: Duration = Duration::from_millis(250);

pub fn count_words(text: &str) -> i64 {
    desafinado_core::diff::count_words(text) as i64
}

fn without_trailing_newlines(text: &str) -> &str {
    text.trim_end_matches('\n')
}

fn lines_of(text: &str) -> Vec<&str> {
    without_trailing_newlines(text).split('\n').collect()
}

struct Changed {
    removed: String,
    added: String,
}

fn changed_runs(before: &str, after: &str) -> Vec<Changed> {
    let old = lines_of(before);
    let new = lines_of(after);
    let ops = capture_diff_slices_deadline(
        Algorithm::Myers,
        &old,
        &new,
        Some(Instant::now() + DIFF_BUDGET),
    );
    let mut runs = Vec::new();
    let mut removed: Vec<&str> = Vec::new();
    let mut added: Vec<&str> = Vec::new();
    let mut pending = false;
    let flush = |runs: &mut Vec<Changed>,
                 removed: &mut Vec<&str>,
                 added: &mut Vec<&str>,
                 pending: &mut bool| {
        if *pending {
            runs.push(Changed {
                removed: removed.join("\n"),
                added: added.join("\n"),
            });
            removed.clear();
            added.clear();
            *pending = false;
        }
    };
    for op in ops {
        match op {
            DiffOp::Equal { .. } => flush(&mut runs, &mut removed, &mut added, &mut pending),
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                removed.extend(&old[old_index..old_index + old_len]);
                pending = true;
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                added.extend(&new[new_index..new_index + new_len]);
                pending = true;
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                removed.extend(&old[old_index..old_index + old_len]);
                added.extend(&new[new_index..new_index + new_len]);
                pending = true;
            }
        }
    }
    flush(&mut runs, &mut removed, &mut added, &mut pending);
    runs
}

fn run_delta(removed: &str, added: &str) -> WordDelta {
    let old: Vec<&str> = removed.split_whitespace().collect();
    let new: Vec<&str> = added.split_whitespace().collect();
    let fallback = WordDelta {
        added: new.len() as i64,
        removed: old.len() as i64,
    };
    let ops = capture_diff_slices_deadline(
        Algorithm::Myers,
        &old,
        &new,
        Some(Instant::now() + DIFF_BUDGET),
    );
    let mut delta = WordDelta {
        added: 0,
        removed: 0,
    };
    for op in ops {
        let (deleted, inserted) = match op {
            DiffOp::Equal { .. } => (0, 0),
            DiffOp::Delete { old_len, .. } => (old_len, 0),
            DiffOp::Insert { new_len, .. } => (0, new_len),
            DiffOp::Replace {
                old_len, new_len, ..
            } => (old_len, new_len),
        };
        delta.removed += deleted as i64;
        delta.added += inserted as i64;
    }
    if (delta.added + delta.removed) as usize > MAX_WORD_EDITS {
        fallback
    } else {
        delta
    }
}

pub fn word_delta(before: &str, after: &str) -> WordDelta {
    changed_runs(before, after)
        .iter()
        .map(|run| run_delta(&run.removed, &run.added))
        .fold(
            WordDelta {
                added: 0,
                removed: 0,
            },
            |total, delta| WordDelta {
                added: total.added + delta.added,
                removed: total.removed + delta.removed,
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_words_added_and_removed() {
        assert_eq!(
            word_delta("one two three", "one 2 three four"),
            WordDelta {
                added: 2,
                removed: 1
            }
        );
    }

    #[test]
    fn counts_every_word_as_added_against_an_empty_text() {
        assert_eq!(
            word_delta("", "one two three"),
            WordDelta {
                added: 3,
                removed: 0
            }
        );
    }

    #[test]
    fn counts_every_word_as_removed_against_an_empty_text() {
        assert_eq!(
            word_delta("one two", ""),
            WordDelta {
                added: 0,
                removed: 2
            }
        );
    }

    #[test]
    fn returns_zeros_when_only_spacing_changed() {
        assert_eq!(
            word_delta("one two", "one  two\n"),
            WordDelta {
                added: 0,
                removed: 0
            }
        );
    }

    #[test]
    fn counts_only_the_lines_that_changed() {
        assert_eq!(
            word_delta(
                "Same first line.\n\nOld middle words.\n\nSame last line.",
                "Same first line.\n\nNew middle words.\n\nSame last line."
            ),
            WordDelta {
                added: 1,
                removed: 1
            }
        );
    }

    #[test]
    fn counts_a_wholesale_replacement_as_every_word_removed_and_added_quickly() {
        let prose = |seed: &str, count: usize| {
            (0..count)
                .map(|i| format!("{seed}{i}a {seed}{i}b {seed}{i}c."))
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        let before = prose("alpha", 600);
        let after = prose("beta", 400);
        let started = Instant::now();
        let delta = word_delta(&before, &after);
        assert!(started.elapsed() < Duration::from_millis(1500));
        assert_eq!(
            delta,
            WordDelta {
                added: count_words(&after),
                removed: count_words(&before)
            }
        );
    }

    #[test]
    fn counts_words_like_the_editor() {
        assert_eq!(count_words(""), 0);
        assert_eq!(count_words("one  two\nthree four"), 4);
        assert_eq!(count_words("# Title\n\nSome body."), 4);
    }
}
