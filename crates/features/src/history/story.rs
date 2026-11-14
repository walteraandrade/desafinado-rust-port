use std::collections::HashSet;

use super::types::{AcceptedChange, SuggestionOrigin, VersionCause, VersionSummary};

pub const DEFAULT_REVIEW_FOCUS: &str =
    "Review for clarity, concision, grammar and tone consistency.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionMark {
    Ai,
    You,
    File,
    Restore,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionDescription {
    pub author: VersionMark,
    pub label: String,
    pub headline: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedChange {
    pub key: String,
    pub label: String,
    pub rationale: String,
}

pub fn mark_of(cause: &VersionCause) -> VersionMark {
    match cause {
        VersionCause::Accepted { .. } => VersionMark::Ai,
        VersionCause::Edited => VersionMark::You,
        VersionCause::Restored { .. } => VersionMark::Restore,
        _ => VersionMark::File,
    }
}

pub fn plural(count: usize, word: &str) -> String {
    format!("{count} {word}{}", if count == 1 { "" } else { "s" })
}

pub fn change_label(change: &AcceptedChange) -> String {
    match change.origin {
        SuggestionOrigin::Ask => change.instruction.clone(),
        SuggestionOrigin::Review if change.instruction == DEFAULT_REVIEW_FOCUS => {
            "Review pass".to_string()
        }
        SuggestionOrigin::Review => format!("Review pass · {}", change.instruction),
    }
}

pub fn mixed_labels(changes: &[AcceptedChange]) -> bool {
    changes
        .iter()
        .map(change_label)
        .collect::<HashSet<_>>()
        .len()
        > 1
}

pub fn list_changes(changes: &[AcceptedChange]) -> Vec<ListedChange> {
    changes
        .iter()
        .enumerate()
        .map(|(position, change)| ListedChange {
            key: position.to_string(),
            label: change_label(change),
            rationale: change.rationale.clone(),
        })
        .collect()
}

fn edited_headline(added: usize, removed: usize) -> String {
    match (added > 0, removed > 0) {
        (true, true) => format!("You added {} and removed {removed}", plural(added, "word")),
        (true, false) => format!("You added {}", plural(added, "word")),
        (false, true) => format!("You removed {}", plural(removed, "word")),
        (false, false) => "You changed spacing or formatting".to_string(),
    }
}

fn accepted_description(changes: &[AcceptedChange]) -> VersionDescription {
    let mut labels: Vec<String> = Vec::new();
    for label in changes.iter().map(change_label) {
        if !labels.contains(&label) {
            labels.push(label);
        }
    }
    let label = if labels.len() == 1 {
        labels[0].clone()
    } else {
        "Suggestions".to_string()
    };
    VersionDescription {
        author: VersionMark::Ai,
        label: format!("AI · {label}"),
        headline: if changes.len() == 1 {
            changes[0].rationale.clone()
        } else {
            format!("Accepted {}", plural(changes.len(), "Suggestion"))
        },
    }
}

pub fn describe_version(
    versions: &[VersionSummary],
    version: &VersionSummary,
    format_time: impl Fn(i64) -> String,
) -> VersionDescription {
    let described = |author, label: &str, headline: String| VersionDescription {
        author,
        label: label.to_string(),
        headline,
    };
    match &version.cause {
        VersionCause::Started => described(
            VersionMark::File,
            "Started",
            format!("Started with {}", plural(version.words, "word")),
        ),
        VersionCause::Opened { name } => {
            described(VersionMark::File, "Opened", format!("Opened {name}"))
        }
        VersionCause::Edited => described(
            VersionMark::You,
            "You",
            edited_headline(version.words_added, version.words_removed),
        ),
        VersionCause::External => described(
            VersionMark::File,
            "External",
            "Changed outside Desafinado".to_string(),
        ),
        VersionCause::Accepted { changes } => accepted_description(changes),
        VersionCause::Restored { from } => described(
            VersionMark::Restore,
            "Restored",
            match versions.iter().find(|item| item.id == *from) {
                Some(source) => {
                    format!("Restored the Version from {}", format_time(source.at))
                }
                None => "Restored an earlier Version".to_string(),
            },
        ),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistorySummary {
    pub versions: usize,
    pub from_ai: usize,
    pub by_you: usize,
}

pub fn history_summary(versions: &[VersionSummary]) -> HistorySummary {
    HistorySummary {
        versions: versions.len(),
        from_ai: versions
            .iter()
            .filter(|v| mark_of(&v.cause) == VersionMark::Ai)
            .count(),
        by_you: versions
            .iter()
            .filter(|v| matches!(mark_of(&v.cause), VersionMark::You | VersionMark::Restore))
            .count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time(at: i64) -> String {
        format!("t{at}")
    }

    fn summary(
        id: i64,
        cause: VersionCause,
        words: usize,
        counts: (usize, usize),
    ) -> VersionSummary {
        VersionSummary {
            id,
            at: id,
            cause,
            words,
            words_added: counts.0,
            words_removed: counts.1,
            saved: None,
        }
    }

    fn with_version(
        cause: VersionCause,
        words: usize,
        counts: (usize, usize),
    ) -> Vec<VersionSummary> {
        vec![
            summary(1, VersionCause::Started, 3, (3, 0)),
            summary(2, cause, words, counts),
        ]
    }

    fn describe_last(versions: &[VersionSummary]) -> VersionDescription {
        describe_version(versions, versions.last().expect("empty History"), time)
    }

    fn change() -> AcceptedChange {
        AcceptedChange {
            origin: SuggestionOrigin::Ask,
            instruction: "Shorter".into(),
            rationale: "Drops filler.".into(),
        }
    }

    fn review(instruction: &str) -> AcceptedChange {
        AcceptedChange {
            origin: SuggestionOrigin::Review,
            instruction: instruction.into(),
            rationale: "Drops filler.".into(),
        }
    }

    fn description(author: VersionMark, label: &str, headline: &str) -> VersionDescription {
        VersionDescription {
            author,
            label: label.into(),
            headline: headline.into(),
        }
    }

    #[test]
    fn describes_the_started_version_by_its_length() {
        assert_eq!(
            describe_last(&[summary(1, VersionCause::Started, 3, (0, 0))]),
            description(VersionMark::File, "Started", "Started with 3 words")
        );
    }

    #[test]
    fn names_the_file_of_an_opened_version() {
        assert_eq!(
            describe_last(&with_version(
                VersionCause::Opened {
                    name: "notes.md".into()
                },
                1,
                (0, 0)
            )),
            description(VersionMark::File, "Opened", "Opened notes.md")
        );
    }

    #[test]
    fn says_a_version_came_from_outside_the_app() {
        assert_eq!(
            describe_last(&with_version(VersionCause::External, 1, (0, 0))),
            description(VersionMark::File, "External", "Changed outside Desafinado")
        );
    }

    #[test]
    fn counts_the_words_an_edited_version_added_and_removed() {
        let headline = |words, counts| {
            describe_last(&with_version(VersionCause::Edited, words, counts)).headline
        };
        assert_eq!(
            describe_last(&with_version(VersionCause::Edited, 4, (2, 1))),
            description(VersionMark::You, "You", "You added 2 words and removed 1")
        );
        assert_eq!(headline(4, (1, 0)), "You added 1 word");
        assert_eq!(headline(1, (0, 2)), "You removed 2 words");
        assert_eq!(headline(3, (0, 0)), "You changed spacing or formatting");
    }

    #[test]
    fn reads_the_counts_of_an_edited_version_off_the_version_itself() {
        assert_eq!(
            describe_last(&[summary(9, VersionCause::Edited, 12, (2, 0))]).headline,
            "You added 2 words"
        );
    }

    #[test]
    fn uses_the_rationale_of_a_single_accepted_suggestion() {
        assert_eq!(
            describe_last(&with_version(
                VersionCause::Accepted {
                    changes: vec![change()]
                },
                1,
                (0, 0)
            )),
            description(VersionMark::Ai, "AI · Shorter", "Drops filler.")
        );
    }

    #[test]
    fn labels_a_review_pass_suggestion_with_its_focus() {
        let reviewed = |instruction: &str| {
            describe_last(&with_version(
                VersionCause::Accepted {
                    changes: vec![review(instruction)],
                },
                1,
                (0, 0),
            ))
            .label
        };
        assert_eq!(reviewed("tone"), "AI · Review pass · tone");
        assert_eq!(reviewed(DEFAULT_REVIEW_FOCUS), "AI · Review pass");
    }

    #[test]
    fn counts_the_suggestions_of_an_accept_all() {
        assert_eq!(
            describe_last(&with_version(
                VersionCause::Accepted {
                    changes: vec![review(DEFAULT_REVIEW_FOCUS), review(DEFAULT_REVIEW_FOCUS)]
                },
                1,
                (0, 0)
            )),
            description(
                VersionMark::Ai,
                "AI · Review pass",
                "Accepted 2 Suggestions"
            )
        );
    }

    #[test]
    fn labels_an_accept_all_of_different_requests_as_suggestions() {
        let other = AcceptedChange {
            instruction: "Clearer".into(),
            ..change()
        };
        assert_eq!(
            describe_last(&with_version(
                VersionCause::Accepted {
                    changes: vec![change(), other]
                },
                1,
                (0, 0)
            ))
            .label,
            "AI · Suggestions"
        );
    }

    #[test]
    fn points_a_restored_version_at_the_time_of_its_source() {
        assert_eq!(
            describe_last(&with_version(VersionCause::Restored { from: 1 }, 1, (0, 0))),
            description(
                VersionMark::Restore,
                "Restored",
                "Restored the Version from t1"
            )
        );
        assert_eq!(
            describe_last(&with_version(
                VersionCause::Restored { from: 99 },
                1,
                (0, 0)
            ))
            .headline,
            "Restored an earlier Version"
        );
    }

    #[test]
    fn labels_each_accepted_suggestion_with_a_key_by_position() {
        let second = AcceptedChange {
            origin: SuggestionOrigin::Review,
            instruction: "tone".into(),
            rationale: "Calmer.".into(),
        };
        assert_eq!(
            list_changes(&[change(), second]),
            vec![
                ListedChange {
                    key: "0".into(),
                    label: "Shorter".into(),
                    rationale: "Drops filler.".into()
                },
                ListedChange {
                    key: "1".into(),
                    label: "Review pass · tone".into(),
                    rationale: "Calmer.".into()
                },
            ]
        );
    }

    #[test]
    fn mixed_labels_is_true_only_when_the_suggestions_came_from_different_requests() {
        let clearer = AcceptedChange {
            instruction: "Clearer".into(),
            ..change()
        };
        assert!(!mixed_labels(&[change(), change()]));
        assert!(mixed_labels(&[change(), clearer]));
    }

    #[test]
    fn counts_versions_ai_changes_and_your_changes() {
        let versions = [
            summary(1, VersionCause::Started, 1, (0, 0)),
            summary(2, VersionCause::Edited, 1, (0, 0)),
            summary(
                3,
                VersionCause::Accepted {
                    changes: vec![change()],
                },
                1,
                (0, 0),
            ),
            summary(4, VersionCause::Restored { from: 1 }, 1, (0, 0)),
        ];
        assert_eq!(
            history_summary(&versions),
            HistorySummary {
                versions: 4,
                from_ai: 1,
                by_you: 2
            }
        );
    }

    #[test]
    fn maps_each_cause_to_a_timeline_mark() {
        assert_eq!(mark_of(&VersionCause::Started), VersionMark::File);
        assert_eq!(
            mark_of(&VersionCause::Opened {
                name: "a.md".into()
            }),
            VersionMark::File
        );
        assert_eq!(mark_of(&VersionCause::External), VersionMark::File);
        assert_eq!(mark_of(&VersionCause::Edited), VersionMark::You);
        assert_eq!(
            mark_of(&VersionCause::Accepted { changes: vec![] }),
            VersionMark::Ai
        );
        assert_eq!(
            mark_of(&VersionCause::Restored { from: 1 }),
            VersionMark::Restore
        );
    }
}
