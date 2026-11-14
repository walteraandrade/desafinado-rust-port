use super::types::{History, Version, VersionCause, VersionId};
use super::word_delta::word_delta;

pub const MAX_VERSIONS: usize = 50;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionInput {
    pub text: String,
    pub cause: VersionCause,
    pub at: i64,
}

fn delta_fields(before: &str, after: &str) -> (Option<usize>, Option<usize>) {
    let delta = word_delta(before, after);
    (Some(delta.added), Some(delta.removed))
}

pub fn start_history(text: &str, at: i64) -> History {
    let (words_added, words_removed) = delta_fields("", text);
    vec![Version {
        id: 1,
        at,
        cause: VersionCause::Started,
        text: text.to_string(),
        saved: None,
        words_added,
        words_removed,
    }]
}

pub fn record_version(history: &[Version], input: VersionInput) -> History {
    let VersionInput { text, cause, at } = input;
    let Some(last) = history.last() else {
        return start_history(&text, at);
    };
    if last.text == text {
        return history.to_vec();
    }
    let (words_added, words_removed) = delta_fields(&last.text, &text);
    let mut next = history.to_vec();
    next.push(Version {
        id: last.id + 1,
        at,
        cause,
        text,
        saved: None,
        words_added,
        words_removed,
    });
    let overflow = next.len().saturating_sub(MAX_VERSIONS);
    next.drain(..overflow);
    next
}

pub fn mark_saved(history: &[Version], text: &str, name: &str, at: i64) -> History {
    let holding = |versions: &[Version]| versions.iter().rposition(|version| version.text == text);
    let recorded = if holding(history).is_none() {
        record_version(
            history,
            VersionInput {
                text: text.to_string(),
                cause: VersionCause::Edited,
                at,
            },
        )
    } else {
        history.to_vec()
    };
    let index = holding(&recorded);
    recorded
        .into_iter()
        .enumerate()
        .map(|(position, version)| {
            if Some(position) == index {
                Version {
                    saved: Some(name.to_string()),
                    ..version
                }
            } else {
                version
            }
        })
        .collect()
}

pub trait Identified {
    fn id(&self) -> VersionId;
}

impl Identified for Version {
    fn id(&self) -> VersionId {
        self.id
    }
}

impl Identified for super::types::VersionSummary {
    fn id(&self) -> VersionId {
        self.id
    }
}

pub fn previous_version<T: Identified>(versions: &[T], id: VersionId) -> Option<&T> {
    let index = versions.iter().position(|version| version.id() == id)?;
    index.checked_sub(1).map(|previous| &versions[previous])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(text: &str, cause: VersionCause, at: i64) -> VersionInput {
        VersionInput {
            text: text.into(),
            cause,
            at,
        }
    }

    fn counts(history: &[Version]) -> Vec<(Option<usize>, Option<usize>)> {
        history
            .iter()
            .map(|v| (v.words_added, v.words_removed))
            .collect()
    }

    #[test]
    fn starts_with_one_started_version_holding_the_text() {
        assert_eq!(
            start_history("# Notes", 100),
            vec![Version {
                id: 1,
                at: 100,
                cause: VersionCause::Started,
                text: "# Notes".into(),
                saved: None,
                words_added: Some(2),
                words_removed: Some(0),
            }]
        );
    }

    #[test]
    fn appends_a_version_with_the_next_id() {
        let history = record_version(&start_history("a", 1), input("ab", VersionCause::Edited, 2));
        assert_eq!(
            history.last(),
            Some(&Version {
                id: 2,
                at: 2,
                cause: VersionCause::Edited,
                text: "ab".into(),
                saved: None,
                words_added: Some(1),
                words_removed: Some(1),
            })
        );
    }

    #[test]
    fn counts_the_words_against_the_version_before_it() {
        let history = ["one two", "one two three", "one"].iter().enumerate().fold(
            start_history("", 1),
            |current, (index, text)| {
                record_version(
                    &current,
                    input(text, VersionCause::Edited, index as i64 + 2),
                )
            },
        );
        assert_eq!(
            counts(&history),
            vec![
                (Some(0), Some(0)),
                (Some(2), Some(0)),
                (Some(1), Some(0)),
                (Some(0), Some(2)),
            ]
        );
    }

    #[test]
    fn counts_a_restored_old_text_against_the_last_version_not_its_source() {
        let grown = record_version(
            &start_history("one two three", 1),
            input("one two three four", VersionCause::Edited, 2),
        );
        let restored = record_version(
            &grown,
            input("one two three", VersionCause::Restored { from: 1 }, 3),
        );
        let last = restored.last().unwrap();
        assert_eq!((last.words_added, last.words_removed), (Some(0), Some(1)));
    }

    #[test]
    fn skips_a_version_whose_text_equals_the_last_one() {
        let history = start_history("same", 1);
        assert_eq!(
            record_version(&history, input("same", VersionCause::Edited, 2)),
            history
        );
    }

    #[test]
    fn keeps_at_most_max_versions_dropping_the_oldest() {
        let full = (1..MAX_VERSIONS + 5).fold(start_history("v0", 0), |history, index| {
            record_version(
                &history,
                input(&format!("v{index}"), VersionCause::Edited, index as i64),
            )
        });
        assert_eq!(full.len(), MAX_VERSIONS);
        assert_eq!(full[0].text, "v5");
        assert_eq!(full.last().unwrap().id, (MAX_VERSIONS + 5) as i64);
    }

    #[test]
    fn marks_the_last_version_when_it_holds_the_saved_text() {
        let history = mark_saved(&start_history("text", 1), "text", "notes.md", 5);
        assert_eq!(
            history,
            vec![Version {
                id: 1,
                at: 1,
                cause: VersionCause::Started,
                text: "text".into(),
                saved: Some("notes.md".into()),
                words_added: Some(1),
                words_removed: Some(0),
            }]
        );
    }

    #[test]
    fn marks_the_newest_version_holding_the_saved_text_even_if_others_followed() {
        let history = [
            VersionCause::Edited,
            VersionCause::Accepted { changes: vec![] },
        ]
        .into_iter()
        .enumerate()
        .fold(start_history("v0", 1), |current, (index, cause)| {
            record_version(
                &current,
                input(&format!("v{}", index + 1), cause, index as i64 + 2),
            )
        });
        let saved = mark_saved(&history, "v0", "notes.md", 9);
        assert_eq!(saved.len(), 3);
        assert_eq!(
            saved.iter().map(|v| v.saved.clone()).collect::<Vec<_>>(),
            vec![Some("notes.md".to_string()), None, None]
        );
    }

    #[test]
    fn records_an_edited_version_first_when_the_saved_text_is_newer() {
        let history = mark_saved(&start_history("old", 1), "new", "notes.md", 5);
        assert_eq!(
            history.last(),
            Some(&Version {
                id: 2,
                at: 5,
                cause: VersionCause::Edited,
                text: "new".into(),
                saved: Some("notes.md".into()),
                words_added: Some(1),
                words_removed: Some(1),
            })
        );
    }

    #[test]
    fn previous_version_returns_the_version_before_the_given_one() {
        let history = record_version(&start_history("a", 1), input("b", VersionCause::Edited, 2));
        assert_eq!(
            previous_version(&history, 2).map(|v| v.text.as_str()),
            Some("a")
        );
    }

    #[test]
    fn previous_version_returns_none_for_the_first_version_or_an_unknown_id() {
        let history = record_version(&start_history("a", 1), input("b", VersionCause::Edited, 2));
        assert!(previous_version(&history, 1).is_none());
        assert!(previous_version(&history, 99).is_none());
    }
}
