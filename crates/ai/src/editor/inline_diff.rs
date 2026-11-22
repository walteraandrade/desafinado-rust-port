//! Where a Suggestion's Replacement shows in the text: word-level insertions next to the
//! struck Original, or a block-level Replacement in rows reserved below it (ADR 0003).
use desafinado_core::diff::{AdaptiveDiff, DiffHunk, DiffMode};

/// A line break inside a word-level insertion. The editor cannot break a line inside
/// inserted text, so the break shows as a mark instead.
pub const LINE_BREAK_MARK: &str = "\u{21b5} ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineInsert {
    pub offset: usize,
    pub text: String,
    /// It follows deleted text, so it shows a small gap after the strike.
    pub separated: bool,
}

/// The insertions of a word-level diff, at document offsets. A block-level diff has none:
/// its Replacement shows below the range instead.
pub fn inline_inserts(from: usize, diff: &AdaptiveDiff) -> Vec<InlineInsert> {
    if diff.mode == DiffMode::Block {
        return Vec::new();
    }
    diff.hunks
        .iter()
        .enumerate()
        .filter_map(|(index, hunk)| match hunk {
            DiffHunk::Inserted { text, at } if !text.is_empty() => Some(InlineInsert {
                offset: from + at,
                text: text.replace('\n', LINE_BREAK_MARK),
                separated: index > 0
                    && matches!(diff.hunks.get(index - 1), Some(DiffHunk::Deleted { .. })),
            }),
            _ => None,
        })
        .collect()
}

/// The Replacement a block-level diff shows below the range.
pub fn block_replacement(diff: &AdaptiveDiff) -> Option<&str> {
    if diff.mode != DiffMode::Block {
        return None;
    }
    diff.hunks.iter().find_map(|hunk| match hunk {
        DiffHunk::Inserted { text, .. } if !text.is_empty() => Some(text.as_str()),
        _ => None,
    })
}

/// Zones on one line stack: given each zone's line and rows, in the order the editor
/// reserves them, the rows above each zone within its line's zones.
pub fn rows_before(zones: &[(usize, usize)]) -> Vec<usize> {
    let mut above = 0;
    let mut line = None;
    zones
        .iter()
        .map(|&(zone_line, rows)| {
            if line != Some(zone_line) {
                line = Some(zone_line);
                above = 0;
            }
            let before = above;
            above += rows;
            before
        })
        .collect()
}

/// Measured heights carry float noise; a height a hair over whole rows takes no extra row.
const ROW_SLACK: f32 = 0.01;

/// Whole editor rows that fit an element `height` tall.
pub fn zone_rows(height: f32, line_height: f32) -> usize {
    if line_height <= 0. || height <= 0. {
        return 0;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive height in rows of a positive line height is a small count"
    )]
    let rows = (height / line_height - ROW_SLACK).ceil().max(0.) as usize;
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use desafinado_core::diff::adaptive_diff;

    #[test]
    fn a_word_change_inserts_next_to_the_struck_word() {
        let diff = adaptive_diff("The quick brown fox jumps.", "The quick red fox jumps.");
        assert_eq!(diff.mode, DiffMode::Word);
        let inserts = inline_inserts(100, &diff);
        assert_eq!(
            inserts,
            vec![InlineInsert {
                offset: 100 + "The quick brown".len(),
                text: "red".into(),
                separated: true,
            }]
        );
    }

    #[test]
    fn a_pure_insertion_is_not_separated() {
        let diff = adaptive_diff(
            "The quick fox jumps over it.",
            "The quick brown fox jumps over it.",
        );
        let inserts = inline_inserts(0, &diff);
        assert_eq!(inserts.len(), 1);
        assert!(!inserts[0].separated);
        assert_eq!(inserts[0].text.trim(), "brown");
        let at = inserts[0].offset;
        let original = "The quick fox jumps over it.";
        let shown = format!("{}{}{}", &original[..at], inserts[0].text, &original[at..]);
        assert_eq!(shown, "The quick brown fox jumps over it.");
    }

    #[test]
    fn each_insertion_of_several_changes_sits_after_its_own_deletion() {
        let original = "Hello there my friend, how are you doing today?";
        let replacement = "Hi there my dear friend, how are you feeling today?";
        let diff = adaptive_diff(original, replacement);
        assert_eq!(diff.mode, DiffMode::Word);
        let inserts = inline_inserts(10, &diff);
        assert_eq!(inserts.len(), 3);
        for insert in &inserts {
            let before = &original[..insert.offset - 10];
            let deleted = diff.hunks.iter().find_map(|hunk| match hunk {
                DiffHunk::Deleted { to, .. } if *to == insert.offset - 10 => Some(*to),
                _ => None,
            });
            assert_eq!(insert.separated, deleted.is_some(), "{before:?}");
        }
        assert_eq!(
            inserts.iter().map(|i| i.text.trim()).collect::<Vec<_>>(),
            ["Hi", "dear", "feeling"]
        );
    }

    #[test]
    fn a_block_change_has_no_inline_inserts_and_shows_its_replacement() {
        let diff = adaptive_diff("One two three.", "Completely different words here.");
        assert_eq!(diff.mode, DiffMode::Block);
        assert!(inline_inserts(0, &diff).is_empty());
        assert_eq!(
            block_replacement(&diff),
            Some("Completely different words here.")
        );
    }

    #[test]
    fn a_word_change_has_no_block_replacement() {
        let diff = adaptive_diff("The quick brown fox jumps.", "The quick red fox jumps.");
        assert_eq!(block_replacement(&diff), None);
    }

    #[test]
    fn an_empty_block_insertion_is_no_block_replacement() {
        let diff = AdaptiveDiff {
            mode: DiffMode::Block,
            hunks: vec![
                DiffHunk::Deleted {
                    text: "Delete all of this.".into(),
                    from: 0,
                    to: 19,
                },
                DiffHunk::Inserted {
                    text: String::new(),
                    at: 19,
                },
            ],
            changed_share: 1.,
        };
        assert_eq!(block_replacement(&diff), None);
    }

    #[test]
    fn line_breaks_in_an_insertion_show_as_a_mark() {
        let diff = AdaptiveDiff {
            mode: DiffMode::Word,
            hunks: vec![DiffHunk::Inserted {
                text: "a\nb".into(),
                at: 2,
            }],
            changed_share: 0.1,
        };
        assert_eq!(inline_inserts(0, &diff)[0].text, "a\u{21b5} b");
    }

    #[test]
    fn zones_on_one_line_stack_and_a_new_line_starts_over() {
        assert_eq!(
            rows_before(&[(2, 3), (2, 2), (2, 1), (5, 4), (7, 2), (7, 1)]),
            [0, 3, 5, 0, 0, 2]
        );
        assert!(rows_before(&[]).is_empty());
    }

    #[test]
    fn zone_rows_round_up_to_whole_rows() {
        assert_eq!(zone_rows(0., 20.), 0);
        assert_eq!(zone_rows(20., 20.), 1);
        assert_eq!(zone_rows(21., 20.), 2);
        assert_eq!(zone_rows(40.0001, 20.), 2);
        assert_eq!(zone_rows(50., 0.), 0);
    }
}
