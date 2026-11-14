use serde::{Deserialize, Serialize};

use super::count_words::count_words;
use super::word_diff::{Part, diff_words_with_space};
use crate::text::is_js_whitespace;

pub const BLOCK_LEVEL_THRESHOLD: f64 = 0.4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum DiffHunk {
    Equal {
        text: String,
        from: usize,
        to: usize,
    },
    Deleted {
        text: String,
        from: usize,
        to: usize,
    },
    Inserted {
        text: String,
        at: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffMode {
    Word,
    Block,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdaptiveDiff {
    pub mode: DiffMode,
    pub hunks: Vec<DiffHunk>,
    pub changed_share: f64,
}

#[derive(Debug, Clone)]
enum Chunk {
    Equal(String),
    Change { deleted: String, inserted: String },
}

fn is_blank(text: &str) -> bool {
    text.chars().all(is_js_whitespace)
}

fn to_chunk(part: Part) -> Chunk {
    if part.added {
        Chunk::Change {
            deleted: String::new(),
            inserted: part.value,
        }
    } else if part.removed {
        Chunk::Change {
            deleted: part.value,
            inserted: String::new(),
        }
    } else {
        Chunk::Equal(part.value)
    }
}

fn join_side(previous: &str, separator: &str, next: &str) -> String {
    let separator = if previous.is_empty() { "" } else { separator };
    format!("{previous}{separator}{next}")
}

fn join_changes(previous: (&str, &str), separator: &str, next: (&str, &str)) -> Chunk {
    Chunk::Change {
        deleted: join_side(previous.0, separator, next.0),
        inserted: join_side(previous.1, separator, next.1),
    }
}

fn push_chunk(mut chunks: Vec<Chunk>, chunk: Chunk) -> Vec<Chunk> {
    let Chunk::Change { deleted, inserted } = &chunk else {
        chunks.push(chunk);
        return chunks;
    };
    let next = (deleted.as_str(), inserted.as_str());
    if let Some(Chunk::Change {
        deleted: last_deleted,
        inserted: last_inserted,
    }) = chunks.last()
    {
        let joined = join_changes((last_deleted, last_inserted), "", next);
        chunks.pop();
        chunks.push(joined);
        return chunks;
    }
    let length = chunks.len();
    if length >= 2
        && let (
            Chunk::Change {
                deleted: before_deleted,
                inserted: before_inserted,
            },
            Chunk::Equal(blank),
        ) = (&chunks[length - 2], &chunks[length - 1])
        && is_blank(blank)
    {
        let joined = join_changes((before_deleted, before_inserted), blank, next);
        chunks.truncate(length - 2);
        chunks.push(joined);
        return chunks;
    }
    chunks.push(chunk);
    chunks
}

fn to_hunks(chunks: &[Chunk]) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut offset = 0;
    for chunk in chunks {
        match chunk {
            Chunk::Equal(text) => {
                let to = offset + text.len();
                hunks.push(DiffHunk::Equal {
                    text: text.clone(),
                    from: offset,
                    to,
                });
                offset = to;
            }
            Chunk::Change { deleted, inserted } => {
                let to = offset + deleted.len();
                if !deleted.is_empty() {
                    hunks.push(DiffHunk::Deleted {
                        text: deleted.clone(),
                        from: offset,
                        to,
                    });
                }
                if !inserted.is_empty() {
                    hunks.push(DiffHunk::Inserted {
                        text: inserted.clone(),
                        at: to,
                    });
                }
                offset = to;
            }
        }
    }
    hunks
}

fn sum_words(hunks: &[DiffHunk], select: fn(&DiffHunk) -> Option<&str>) -> usize {
    hunks.iter().filter_map(select).map(count_words).sum()
}

pub fn changed_share(hunks: &[DiffHunk]) -> f64 {
    let deleted = sum_words(hunks, |hunk| match hunk {
        DiffHunk::Deleted { text, .. } => Some(text),
        _ => None,
    });
    let inserted = sum_words(hunks, |hunk| match hunk {
        DiffHunk::Inserted { text, .. } => Some(text),
        _ => None,
    });
    let equal = sum_words(hunks, |hunk| match hunk {
        DiffHunk::Equal { text, .. } => Some(text),
        _ => None,
    });
    let changed = deleted + inserted;
    let total = changed + 2 * equal;
    if total == 0 {
        0.0
    } else {
        changed as f64 / total as f64
    }
}

fn block_hunks(original: &str, replacement: &str) -> Vec<DiffHunk> {
    let deleted = (!original.is_empty()).then(|| DiffHunk::Deleted {
        text: original.to_string(),
        from: 0,
        to: original.len(),
    });
    let inserted = (!replacement.is_empty()).then(|| DiffHunk::Inserted {
        text: replacement.to_string(),
        at: original.len(),
    });
    deleted.into_iter().chain(inserted).collect()
}

pub fn adaptive_diff(original: &str, replacement: &str) -> AdaptiveDiff {
    let chunks = diff_words_with_space(original, replacement)
        .into_iter()
        .map(to_chunk)
        .fold(Vec::new(), push_chunk);
    let hunks = to_hunks(&chunks);
    let share = changed_share(&hunks);
    if share > BLOCK_LEVEL_THRESHOLD {
        AdaptiveDiff {
            mode: DiffMode::Block,
            hunks: block_hunks(original, replacement),
            changed_share: share,
        }
    } else {
        AdaptiveDiff {
            mode: DiffMode::Word,
            hunks,
            changed_share: share,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn original_of(hunks: &[DiffHunk]) -> String {
        hunks
            .iter()
            .filter_map(|hunk| match hunk {
                DiffHunk::Equal { text, .. } | DiffHunk::Deleted { text, .. } => {
                    Some(text.as_str())
                }
                DiffHunk::Inserted { .. } => None,
            })
            .collect()
    }

    fn replacement_of(hunks: &[DiffHunk]) -> String {
        hunks
            .iter()
            .filter_map(|hunk| match hunk {
                DiffHunk::Equal { text, .. } | DiffHunk::Inserted { text, .. } => {
                    Some(text.as_str())
                }
                DiffHunk::Deleted { .. } => None,
            })
            .collect()
    }

    fn words(count: usize, prefix: &str) -> String {
        (0..count)
            .map(|index| format!("{prefix}{index}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn with_changed_words(total: usize, changed: usize) -> (String, String) {
        (
            format!(
                "{} {}",
                words(total - changed, "keep"),
                words(changed, "old")
            ),
            format!(
                "{} {}",
                words(total - changed, "keep"),
                words(changed, "new")
            ),
        )
    }

    fn deleted(hunks: &[DiffHunk]) -> Vec<&DiffHunk> {
        hunks
            .iter()
            .filter(|hunk| matches!(hunk, DiffHunk::Deleted { .. }))
            .collect()
    }

    fn inserted(hunks: &[DiffHunk]) -> Vec<&DiffHunk> {
        hunks
            .iter()
            .filter(|hunk| matches!(hunk, DiffHunk::Inserted { .. }))
            .collect()
    }

    fn text_of(hunk: &DiffHunk) -> &str {
        match hunk {
            DiffHunk::Equal { text, .. }
            | DiffHunk::Deleted { text, .. }
            | DiffHunk::Inserted { text, .. } => text,
        }
    }

    fn equal(text: &str, from: usize, to: usize) -> DiffHunk {
        DiffHunk::Equal {
            text: text.into(),
            from,
            to,
        }
    }

    fn del(text: &str, from: usize, to: usize) -> DiffHunk {
        DiffHunk::Deleted {
            text: text.into(),
            from,
            to,
        }
    }

    fn ins(text: &str, at: usize) -> DiffHunk {
        DiffHunk::Inserted {
            text: text.into(),
            at,
        }
    }

    #[test]
    fn exposes_the_threshold_as_a_named_constant() {
        assert_eq!(BLOCK_LEVEL_THRESHOLD, 0.4);
    }

    #[test]
    fn returns_a_single_equal_hunk_for_identical_text() {
        let result = adaptive_diff("Same text.", "Same text.");
        assert_eq!(result.mode, DiffMode::Word);
        assert_eq!(result.changed_share, 0.0);
        assert_eq!(result.hunks, vec![equal("Same text.", 0, 10)]);
    }

    #[test]
    fn marks_a_replaced_word_as_deleted_then_inserted_with_original_offsets() {
        let original = "Once the canary looks healthy, you will recieve a prompt to promote it.";
        let replacement = "Once the canary looks healthy, you will receive a prompt to promote it.";
        let result = adaptive_diff(original, replacement);

        assert_eq!(result.mode, DiffMode::Word);
        let deleted_hunk = result
            .hunks
            .iter()
            .find(|hunk| matches!(hunk, DiffHunk::Deleted { .. }))
            .unwrap();
        let inserted_hunk = result
            .hunks
            .iter()
            .find(|hunk| matches!(hunk, DiffHunk::Inserted { .. }))
            .unwrap();
        assert_eq!(text_of(deleted_hunk), "recieve");
        assert_eq!(text_of(inserted_hunk), "receive");
        let (DiffHunk::Deleted { from, to, .. }, DiffHunk::Inserted { at, .. }) =
            (deleted_hunk, inserted_hunk)
        else {
            panic!("expected a deleted and an inserted hunk");
        };
        assert_eq!(&original[*from..*to], "recieve");
        assert_eq!(at, to);
        assert_eq!(original_of(&result.hunks), original);
        assert_eq!(replacement_of(&result.hunks), replacement);
    }

    #[test]
    fn reports_a_pure_insertion_with_no_deleted_hunk() {
        let result = adaptive_diff(
            "The quick brown fox jumps over the dog",
            "The quick brown fox jumps over the lazy dog",
        );
        assert_eq!(result.mode, DiffMode::Word);
        assert!(deleted(&result.hunks).is_empty());
        assert_eq!(inserted(&result.hunks), vec![&ins("lazy ", 35)]);
    }

    #[test]
    fn reports_a_pure_deletion_with_no_inserted_hunk() {
        let result = adaptive_diff(
            "The quick brown fox jumps over the lazy dog today",
            "The quick brown fox jumps over the lazy dog",
        );
        assert_eq!(result.mode, DiffMode::Word);
        assert!(inserted(&result.hunks).is_empty());
        assert_eq!(deleted(&result.hunks), vec![&del(" today", 43, 49)]);
    }

    #[test]
    fn keeps_hunks_reversible_in_both_directions() {
        let original = "We deploy on weekdays, and the team reviews every change.";
        let replacement = "We deploy on any weekday, and reviewers check every change.";
        let result = adaptive_diff(original, replacement);
        assert_eq!(original_of(&result.hunks), original);
        assert_eq!(replacement_of(&result.hunks), replacement);
    }

    #[test]
    fn merges_changes_separated_only_by_whitespace_into_one_deleted_and_one_inserted_hunk() {
        let original = format!("{} foo bar {}", words(20, "keep"), words(20, "tail"));
        let replacement = format!("{} alpha beta {}", words(20, "keep"), words(20, "tail"));
        let result = adaptive_diff(&original, &replacement);

        assert_eq!(result.mode, DiffMode::Word);
        assert_eq!(deleted(&result.hunks).len(), 1);
        assert_eq!(inserted(&result.hunks).len(), 1);
        assert_eq!(text_of(deleted(&result.hunks)[0]), "foo bar");
        assert_eq!(text_of(inserted(&result.hunks)[0]), "alpha beta");
    }

    #[test]
    fn always_puts_the_deleted_hunk_before_the_inserted_hunk_of_a_change() {
        let result = adaptive_diff(
            &format!("{} old {}", words(20, "keep"), words(20, "tail")),
            &format!("{} new {}", words(20, "keep"), words(20, "tail")),
        );
        let kinds: Vec<&str> = result
            .hunks
            .iter()
            .map(|hunk| match hunk {
                DiffHunk::Equal { .. } => "equal",
                DiffHunk::Deleted { .. } => "deleted",
                DiffHunk::Inserted { .. } => "inserted",
            })
            .collect();
        assert_eq!(kinds, ["equal", "deleted", "inserted", "equal"]);
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 0.005, "{actual} vs {expected}");
    }

    mod threshold {
        use super::*;

        #[test]
        fn stays_word_level_when_the_changed_share_is_below_the_threshold() {
            let (original, replacement) = with_changed_words(10, 3);
            let result = adaptive_diff(&original, &replacement);
            assert_close(result.changed_share, 0.3);
            assert_eq!(result.mode, DiffMode::Word);
        }

        #[test]
        fn stays_word_level_when_the_changed_share_equals_the_threshold() {
            let (original, replacement) = with_changed_words(10, 4);
            let result = adaptive_diff(&original, &replacement);
            assert_close(result.changed_share, BLOCK_LEVEL_THRESHOLD);
            assert_eq!(result.mode, DiffMode::Word);
        }

        #[test]
        fn switches_to_block_level_when_the_changed_share_exceeds_the_threshold() {
            let (original, replacement) = with_changed_words(10, 5);
            let result = adaptive_diff(&original, &replacement);
            assert_close(result.changed_share, 0.5);
            assert_eq!(result.mode, DiffMode::Block);
        }

        #[test]
        fn strikes_the_whole_original_and_inserts_the_whole_replacement_in_block_mode() {
            let original = "one two three four five";
            let replacement = "alpha beta gamma delta epsilon";
            let result = adaptive_diff(original, replacement);
            assert_eq!(result.mode, DiffMode::Block);
            assert_eq!(result.changed_share, 1.0);
            assert_eq!(
                result.hunks,
                vec![
                    del(original, 0, original.len()),
                    ins(replacement, original.len())
                ]
            );
        }

        #[test]
        fn counts_a_heavy_cut_as_a_large_change() {
            let original = words(20, "w");
            let replacement = words(4, "w");
            let result = adaptive_diff(&original, &replacement);
            assert_close(result.changed_share, 16.0 / 24.0);
            assert_eq!(result.mode, DiffMode::Block);
        }
    }

    mod whitespace_and_markdown {
        use super::*;

        #[test]
        fn keeps_list_markers_equal_when_only_one_item_changes() {
            let original =
                "- Tests are passing\n- Get a review\n- Write a rollback plan\n- Avoid handover";
            let replacement = "- Tests are passing\n- A second engineer reviews\n- Write a rollback plan\n- Avoid handover";
            let result = adaptive_diff(original, replacement);

            assert_eq!(result.mode, DiffMode::Word);
            assert_eq!(original_of(&result.hunks), original);
            assert_eq!(replacement_of(&result.hunks), replacement);
            let deleted_texts: Vec<&str> = deleted(&result.hunks)
                .into_iter()
                .map(|hunk| text_of(hunk).trim())
                .collect();
            assert_eq!(deleted_texts, ["Get a review"]);
        }

        #[test]
        fn treats_emphasis_markers_as_part_of_the_word() {
            let original = format!("{} **bold** {}", words(15, "keep"), words(15, "tail"));
            let replacement = format!("{} __bold__ {}", words(15, "keep"), words(15, "tail"));
            let result = adaptive_diff(&original, &replacement);

            assert_eq!(result.mode, DiffMode::Word);
            assert_eq!(text_of(deleted(&result.hunks)[0]), "**bold**");
            assert_eq!(text_of(inserted(&result.hunks)[0]), "__bold__");
        }

        #[test]
        fn reports_a_removed_trailing_newline_without_counting_it_as_a_word() {
            let result = adaptive_diff("Same text.\n", "Same text.");
            assert_eq!(result.mode, DiffMode::Word);
            assert_eq!(result.changed_share, 0.0);
            assert_eq!(
                result.hunks,
                vec![equal("Same text.", 0, 10), del("\n", 10, 11)]
            );
        }

        #[test]
        fn reports_a_changed_amount_of_whitespace_as_an_actual_change() {
            let result = adaptive_diff("one  two", "one two");
            assert_eq!(original_of(&result.hunks), "one  two");
            assert_eq!(replacement_of(&result.hunks), "one two");
            assert!(
                result
                    .hunks
                    .iter()
                    .any(|hunk| !matches!(hunk, DiffHunk::Equal { .. }))
            );
        }

        #[test]
        fn handles_an_empty_replacement_and_an_empty_original() {
            assert_eq!(adaptive_diff("", "").hunks, vec![]);
            assert_eq!(adaptive_diff("", "").changed_share, 0.0);
            let added = adaptive_diff("", "new text");
            assert_eq!(added.hunks, vec![ins("new text", 0)]);
            let removed = adaptive_diff("old text", "");
            assert_eq!(removed.hunks, vec![del("old text", 0, 8)]);
        }
    }
}
