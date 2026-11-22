use super::line_runs::{LineRun, line_runs};
use crate::diff::{DiffHunk, adaptive_diff};

#[derive(Clone, Debug, PartialEq)]
pub enum LineChange {
    Same { lines: Vec<String> },
    Changed { hunks: Vec<DiffHunk> },
}

fn changed_hunks(removed: &str, added: &str) -> Vec<DiffHunk> {
    if removed.is_empty() {
        vec![DiffHunk::Inserted {
            text: added.to_string(),
            at: 0,
        }]
    } else if added.is_empty() {
        vec![DiffHunk::Deleted {
            text: removed.to_string(),
            from: 0,
            to: removed.len(),
        }]
    } else {
        adaptive_diff(removed, added).hunks
    }
}

pub fn line_changes(before: &str, after: &str) -> Vec<LineChange> {
    line_runs(before, after)
        .into_iter()
        .map(|run| match run {
            LineRun::Same { lines } => LineChange::Same { lines },
            LineRun::Changed { removed, added } => LineChange::Changed {
                hunks: changed_hunks(&removed, &added),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(lines: &[&str]) -> LineChange {
        LineChange::Same {
            lines: lines.iter().map(std::string::ToString::to_string).collect(),
        }
    }

    fn equal(text: &str, from: usize, to: usize) -> DiffHunk {
        DiffHunk::Equal {
            text: text.into(),
            from,
            to,
        }
    }

    fn kinds(changes: &[LineChange]) -> Vec<&'static str> {
        changes
            .iter()
            .map(|change| match change {
                LineChange::Same { .. } => "same",
                LineChange::Changed { .. } => "changed",
            })
            .collect()
    }

    #[test]
    fn returns_one_same_run_for_equal_texts() {
        assert_eq!(
            line_changes("a\n\nb", "a\n\nb"),
            vec![same(&["a", "", "b"])]
        );
    }

    #[test]
    fn keeps_a_word_change_inside_one_line_as_a_word_diff() {
        let changes = line_changes(
            "Intro\n\nYou will recieve a prompt.\n\nEnd",
            "Intro\n\nYou will receive a prompt.\n\nEnd",
        );
        assert_eq!(
            changes,
            vec![
                same(&["Intro", ""]),
                LineChange::Changed {
                    hunks: vec![
                        equal("You will ", 0, 9),
                        DiffHunk::Deleted {
                            text: "recieve".into(),
                            from: 9,
                            to: 16
                        },
                        DiffHunk::Inserted {
                            text: "receive".into(),
                            at: 16
                        },
                        equal(" a prompt.", 16, 26),
                    ]
                },
                same(&["", "End"]),
            ]
        );
    }

    #[test]
    fn touches_only_the_changed_items_of_a_list() {
        let before = "- Tests pass\n- Get a review\n- Write a plan\n- No handover";
        let after = "- Tests pass\n- A review is done\n- Write a plan\n- No handover";
        let changes = line_changes(before, after);
        assert_eq!(kinds(&changes), vec!["same", "changed", "same"]);
        assert_eq!(changes[0], same(&["- Tests pass"]));
        assert_eq!(changes[2], same(&["- Write a plan", "- No handover"]));
    }

    #[test]
    fn shows_added_lines_as_inserted_and_removed_lines_as_deleted() {
        assert_eq!(
            line_changes("a", "a\nb"),
            vec![
                same(&["a"]),
                LineChange::Changed {
                    hunks: vec![DiffHunk::Inserted {
                        text: "b".into(),
                        at: 0
                    }]
                }
            ]
        );
        assert_eq!(
            line_changes("a\nb", "a"),
            vec![
                same(&["a"]),
                LineChange::Changed {
                    hunks: vec![DiffHunk::Deleted {
                        text: "b".into(),
                        from: 0,
                        to: 1
                    }]
                }
            ]
        );
    }

    #[test]
    fn diffs_a_run_of_two_lines_replaced_by_one_as_one_change() {
        let changes = line_changes("a\nold one\nold two\nz", "a\nnew\nz");
        assert_eq!(kinds(&changes), vec!["same", "changed", "same"]);
        let LineChange::Changed { hunks } = &changes[1] else {
            panic!("expected a change");
        };
        let removed: String = hunks
            .iter()
            .filter(|hunk| !matches!(hunk, DiffHunk::Inserted { .. }))
            .map(DiffHunk::text)
            .collect();
        let added: String = hunks
            .iter()
            .filter(|hunk| !matches!(hunk, DiffHunk::Deleted { .. }))
            .map(DiffHunk::text)
            .collect();
        assert_eq!(removed, "old one\nold two");
        assert_eq!(added, "new");
    }

    #[test]
    fn keeps_the_lines_after_a_deleted_block() {
        assert_eq!(
            line_changes("gone\n\nkept\nalso kept", "kept\nalso kept"),
            vec![
                LineChange::Changed {
                    hunks: vec![DiffHunk::Deleted {
                        text: "gone\n".into(),
                        from: 0,
                        to: 5
                    }]
                },
                same(&["kept", "also kept"]),
            ]
        );
    }

    #[test]
    fn treats_a_trailing_newline_difference_as_no_change() {
        assert_eq!(line_changes("a\n", "a"), vec![same(&["a"])]);
    }
}
