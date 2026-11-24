//! A request the writer keeps editing around while the model works on it. The writer's edits
//! inside its range are kept as changes over the text the model saw; when the answer comes, the
//! model's changes are laid over the edited text, line by line, and the ones that touch a line
//! the writer changed are dropped.

use crate::change_set::{Assoc, ChangeSet, TextChange, changes_between};

/// The part of `changes` (over the whole Document) that falls inside `from..to`, as changes over
/// that range. An insertion at either edge stays outside, as `map_suggestions` moves the range.
pub fn edits_inside(from: usize, to: usize, changes: &ChangeSet) -> ChangeSet {
    let inside: Vec<TextChange> = changes
        .changes()
        .iter()
        .filter(|change| change.from < to && change.to > from)
        .map(|change| {
            let contained = change.from >= from && change.to <= to;
            TextChange::new(
                change.from.clamp(from, to) - from,
                change.to.clamp(from, to) - from,
                if contained {
                    change.insert.as_str()
                } else {
                    ""
                },
            )
        })
        .collect();
    ChangeSet::of(&inside, to - from)
}

/// The model's answer laid over the writer's edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rebased {
    /// The range's text now, with the writer's edits.
    pub original: String,
    /// That text with the model's changes that did not touch an edit.
    pub replacement: String,
    /// How many of the model's changes touched an edit and were dropped.
    pub dropped: usize,
    /// How many were kept.
    pub kept: usize,
}

fn touches(edit: &TextChange, change: &TextChange) -> bool {
    edit.from <= change.to && change.from <= edit.to
}

/// Lays `replacement`, the model's answer for `original`, over `edits`, the writer's changes to
/// `original` since the request left.
pub fn rebase_replacement(original: &str, edits: &ChangeSet, replacement: &str) -> Rebased {
    let current = edits.apply(original);
    let model = changes_between(original, replacement);
    let (kept, dropped): (Vec<&TextChange>, Vec<&TextChange>) = model
        .changes()
        .iter()
        .partition(|change| !edits.changes().iter().any(|edit| touches(edit, change)));
    let moved: Vec<TextChange> = kept
        .iter()
        .map(|change| {
            TextChange::new(
                edits.map_pos(change.from, Assoc::After),
                edits.map_pos(change.to, Assoc::After),
                change.insert.as_str(),
            )
        })
        .collect();
    Rebased {
        replacement: ChangeSet::of(&moved, current.len()).apply(&current),
        original: current,
        dropped: dropped.len(),
        kept: kept.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(changes: &[TextChange], length: usize) -> ChangeSet {
        ChangeSet::of(changes, length)
    }

    #[test]
    fn keeps_only_the_changes_inside_the_range_shifted_to_it() {
        let changes = set(
            &[
                TextChange::new(2, 3, "x"),
                TextChange::new(12, 14, "yy"),
                TextChange::new(40, 41, "z"),
            ],
            50,
        );
        let inside = edits_inside(10, 30, &changes);
        assert_eq!(inside.changes(), [TextChange::new(2, 4, "yy")]);
        assert_eq!(inside.length(), 20);
    }

    #[test]
    fn leaves_an_insertion_at_either_edge_outside() {
        let changes = set(
            &[TextChange::insert(10, "a"), TextChange::insert(30, "b")],
            50,
        );
        assert!(edits_inside(10, 30, &changes).is_empty());
    }

    #[test]
    fn deletes_the_inside_part_of_a_change_across_an_edge_without_its_text() {
        let changes = set(&[TextChange::new(5, 15, "new")], 50);
        assert_eq!(
            edits_inside(10, 30, &changes).changes(),
            [TextChange::new(0, 5, "")]
        );
    }

    #[test]
    fn without_edits_the_answer_stays_as_it_came() {
        let original = "One.\n\nTwo.\n";
        let rebased = rebase_replacement(original, &ChangeSet::empty(11), "One!\n\nTwo!\n");
        assert_eq!(rebased.original, original);
        assert_eq!(rebased.replacement, "One!\n\nTwo!\n");
        assert_eq!(rebased.dropped, 0);
    }

    #[test]
    fn keeps_the_models_change_to_a_paragraph_the_writer_did_not_touch() {
        let original = "First one.\n\nSecond one.\n";
        let edits = set(&[TextChange::new(6, 9, "two")], original.len());
        let rebased = rebase_replacement(original, &edits, "First one.\n\nSecond, shorter.\n");
        assert_eq!(rebased.original, "First two.\n\nSecond one.\n");
        assert_eq!(rebased.replacement, "First two.\n\nSecond, shorter.\n");
        assert_eq!((rebased.kept, rebased.dropped), (1, 0));
    }

    #[test]
    fn drops_the_models_change_to_a_paragraph_the_writer_edited() {
        let original = "First one.\n\nSecond one.\n";
        let edits = set(&[TextChange::new(6, 9, "two")], original.len());
        let rebased = rebase_replacement(original, &edits, "First, shorter.\n\nSecond one!\n");
        assert_eq!(rebased.replacement, "First two.\n\nSecond one!\n");
        assert_eq!((rebased.kept, rebased.dropped), (1, 1));
    }

    #[test]
    fn keeps_nothing_when_every_change_touches_an_edit() {
        let original = "Only one.\n";
        let edits = set(&[TextChange::insert(4, "y")], original.len());
        let rebased = rebase_replacement(original, &edits, "Just one.\n");
        assert_eq!(rebased.replacement, rebased.original);
        assert_eq!((rebased.kept, rebased.dropped), (0, 1));
    }

    #[test]
    fn places_kept_changes_after_text_the_writer_added_above_them() {
        let original = "A.\n\nB.\n";
        let edits = set(&[TextChange::insert(0, "New line.\n\n")], original.len());
        let rebased = rebase_replacement(original, &edits, "A.\n\nB, longer.\n");
        assert_eq!(rebased.replacement, "New line.\n\nA.\n\nB, longer.\n");
        assert_eq!(rebased.dropped, 0);
    }
}
