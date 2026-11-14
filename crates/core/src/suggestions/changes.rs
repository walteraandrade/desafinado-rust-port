use crate::diff::{AdaptiveDiff, DiffHunk};

use super::types::Direction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeRange {
    pub from: usize,
    pub to: usize,
}

fn is_changed(hunk: Option<&DiffHunk>) -> bool {
    hunk.is_some_and(|hunk| !matches!(hunk, DiffHunk::Equal { .. }))
}

fn hunk_start(hunk: &DiffHunk) -> usize {
    match hunk {
        DiffHunk::Inserted { at, .. } => *at,
        DiffHunk::Deleted { from, .. } | DiffHunk::Equal { from, .. } => *from,
    }
}

fn hunk_end(hunk: &DiffHunk) -> usize {
    match hunk {
        DiffHunk::Inserted { at, .. } => *at,
        DiffHunk::Deleted { to, .. } | DiffHunk::Equal { to, .. } => *to,
    }
}

pub fn change_ranges(from: usize, diff: &AdaptiveDiff) -> Vec<ChangeRange> {
    let hunks = &diff.hunks;
    let starts: Vec<&DiffHunk> = hunks
        .iter()
        .enumerate()
        .filter(|(index, hunk)| {
            is_changed(Some(hunk)) && !is_changed(index.checked_sub(1).and_then(|i| hunks.get(i)))
        })
        .map(|(_, hunk)| hunk)
        .collect();
    let ends: Vec<&DiffHunk> = hunks
        .iter()
        .enumerate()
        .filter(|(index, hunk)| is_changed(Some(hunk)) && !is_changed(hunks.get(index + 1)))
        .map(|(_, hunk)| hunk)
        .collect();
    starts
        .iter()
        .enumerate()
        .map(|(index, start)| ChangeRange {
            from: from + hunk_start(start),
            to: from + hunk_end(ends.get(index).unwrap_or(start)),
        })
        .collect()
}

pub fn first_change_at(from: usize, diff: &AdaptiveDiff) -> usize {
    change_ranges(from, diff)
        .first()
        .map_or(from, |range| range.from)
}

pub fn step_change(ranges: &[ChangeRange], cursor: usize, direction: Direction) -> Option<usize> {
    match direction {
        Direction::Forward => ranges
            .iter()
            .find(|range| range.from > cursor)
            .or(ranges.first())
            .map(|range| range.from),
        Direction::Backward => ranges
            .iter()
            .rfind(|range| range.to < cursor)
            .or(ranges.last())
            .map(|range| range.from),
    }
}

pub fn change_label(ranges: &[ChangeRange], cursor: usize) -> String {
    match ranges
        .iter()
        .position(|range| range.from <= cursor && cursor <= range.to)
    {
        Some(index) => format!("Change {} of {}", index + 1, ranges.len()),
        None => format!(
            "{} {}",
            ranges.len(),
            if ranges.len() == 1 {
                "change"
            } else {
                "changes"
            }
        ),
    }
}

pub fn shows_changes(from: usize, original: &str, diff: &AdaptiveDiff) -> bool {
    change_ranges(from, diff).len() >= 2 || original.contains('\n')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::adaptive_diff;

    fn at(original: &str, replacement: &str) -> AdaptiveDiff {
        adaptive_diff(original, replacement)
    }

    fn range(from: usize, to: usize) -> ChangeRange {
        ChangeRange { from, to }
    }

    mod change_ranges_tests {
        use super::*;

        #[test]
        fn finds_where_each_change_sits_in_the_document() {
            let diff = at(
                "The team review their work and it matter a lot.",
                "The team reviews their work and it matters a lot.",
            );
            assert_eq!(
                change_ranges(100, &diff),
                vec![range(109, 115), range(134, 140)]
            );
        }

        #[test]
        fn counts_a_deletion_and_its_insertion_as_one_change() {
            assert_eq!(
                change_ranges(0, &at("one two three", "one 2 three")),
                vec![range(4, 7)]
            );
        }

        #[test]
        fn counts_a_pure_insertion_as_an_empty_range() {
            assert_eq!(
                change_ranges(10, &at("one three", "one two three")),
                vec![range(14, 14)]
            );
        }

        #[test]
        fn has_one_change_over_the_whole_target_for_a_block_rewrite() {
            assert_eq!(
                change_ranges(7, &at("An old sentence here.", "Something else entirely.")),
                vec![range(7, 28)]
            );
        }
    }

    fn sample_ranges() -> Vec<ChangeRange> {
        vec![range(10, 14), range(40, 46), range(90, 90)]
    }

    mod step_change_tests {
        use super::*;

        #[test]
        fn goes_to_the_next_change_after_the_cursor() {
            assert_eq!(
                step_change(&sample_ranges(), 10, Direction::Forward),
                Some(40)
            );
            assert_eq!(
                step_change(&sample_ranges(), 43, Direction::Forward),
                Some(90)
            );
        }

        #[test]
        fn goes_to_the_change_before_the_one_the_cursor_is_in() {
            assert_eq!(
                step_change(&sample_ranges(), 90, Direction::Backward),
                Some(40)
            );
            assert_eq!(
                step_change(&sample_ranges(), 43, Direction::Backward),
                Some(10)
            );
            assert_eq!(
                step_change(&sample_ranges(), 40, Direction::Backward),
                Some(10)
            );
        }

        #[test]
        fn wraps_around_at_both_ends() {
            assert_eq!(
                step_change(&sample_ranges(), 95, Direction::Forward),
                Some(10)
            );
            assert_eq!(
                step_change(&sample_ranges(), 12, Direction::Backward),
                Some(90)
            );
        }

        #[test]
        fn has_nowhere_to_go_without_changes() {
            assert_eq!(step_change(&[], 5, Direction::Forward), None);
        }
    }

    mod change_label_tests {
        use super::*;

        #[test]
        fn counts_the_changes_while_the_cursor_is_elsewhere() {
            assert_eq!(change_label(&sample_ranges(), 50), "3 changes");
            assert_eq!(change_label(&[range(10, 12)], 50), "1 change");
        }

        #[test]
        fn says_which_change_the_cursor_is_in_anywhere_inside_it() {
            assert_eq!(change_label(&sample_ranges(), 40), "Change 2 of 3");
            assert_eq!(change_label(&sample_ranges(), 43), "Change 2 of 3");
            assert_eq!(change_label(&sample_ranges(), 90), "Change 3 of 3");
        }
    }

    mod shows_changes_tests {
        use super::*;

        #[test]
        fn shows_for_two_or_more_changes() {
            let original = "one two three four five six";
            assert!(shows_changes(
                0,
                original,
                &at(original, "one 2 three four five 6")
            ));
        }

        #[test]
        fn shows_for_a_suggestion_over_more_than_one_line() {
            let original = "One.\n\nTwo here.";
            assert!(shows_changes(
                0,
                original,
                &at(original, "One.\n\nTwo there.")
            ));
        }

        #[test]
        fn stays_out_of_the_way_for_one_change_in_one_line() {
            let original = "one two three";
            assert!(!shows_changes(0, original, &at(original, "one 2 three")));
        }
    }

    mod first_change_at_tests {
        use super::*;

        #[test]
        fn points_at_the_first_change_not_the_start_of_the_target() {
            let diff = at(
                "The team review their work and it matter a lot.",
                "The team reviews their work and it matters a lot.",
            );
            assert_eq!(first_change_at(100, &diff), 109);
        }

        #[test]
        fn falls_back_to_the_start_of_the_target_when_nothing_changed() {
            assert_eq!(first_change_at(5, &at("same", "same")), 5);
        }
    }
}
