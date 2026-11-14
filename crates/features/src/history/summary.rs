use super::types::{Version, VersionSummary};
use super::word_delta::{WordDelta, word_delta};
use crate::text::count_words;

fn delta_of(version: &Version, previous: Option<&Version>) -> WordDelta {
    match (version.words_added, version.words_removed) {
        (Some(added), Some(removed)) => WordDelta { added, removed },
        _ => word_delta(previous.map_or("", |p| p.text.as_str()), &version.text),
    }
}

pub fn summarize(version: &Version, previous: Option<&Version>) -> VersionSummary {
    let WordDelta { added, removed } = delta_of(version, previous);
    VersionSummary {
        id: version.id,
        at: version.at,
        cause: version.cause.clone(),
        words: count_words(&version.text),
        words_added: added,
        words_removed: removed,
        saved: version.saved.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::types::VersionCause;

    fn version() -> Version {
        Version {
            id: 4,
            at: 9,
            cause: VersionCause::Edited,
            text: "three small words".into(),
            saved: None,
            words_added: None,
            words_removed: None,
        }
    }

    #[test]
    fn drops_the_text_and_keeps_its_word_count() {
        let summary = summarize(
            &Version {
                words_added: Some(3),
                words_removed: Some(0),
                ..version()
            },
            None,
        );
        assert_eq!(
            summary,
            VersionSummary {
                id: 4,
                at: 9,
                cause: VersionCause::Edited,
                words: 3,
                words_added: 3,
                words_removed: 0,
                saved: None,
            }
        );
    }

    #[test]
    fn keeps_the_saved_file_name() {
        let summary = summarize(
            &Version {
                id: 1,
                cause: VersionCause::Started,
                text: String::new(),
                saved: Some("notes.md".into()),
                words_added: Some(0),
                words_removed: Some(0),
                ..version()
            },
            None,
        );
        assert_eq!(
            summary,
            VersionSummary {
                id: 1,
                at: 9,
                cause: VersionCause::Started,
                words: 0,
                words_added: 0,
                words_removed: 0,
                saved: Some("notes.md".into()),
            }
        );
    }

    #[test]
    fn keeps_the_counts_it_was_stored_with_whatever_the_version_before_says() {
        let previous = Version {
            id: 3,
            text: "three small words".into(),
            ..version()
        };
        let summary = summarize(
            &Version {
                words_added: Some(7),
                words_removed: Some(5),
                ..version()
            },
            Some(&previous),
        );
        assert_eq!((summary.words_added, summary.words_removed), (7, 5));
    }

    #[test]
    fn works_the_counts_out_against_the_version_before_when_none_were_stored() {
        let previous = Version {
            id: 3,
            text: "one two three".into(),
            ..version()
        };
        let summary = summarize(
            &Version {
                text: "one 2 three four".into(),
                ..version()
            },
            Some(&previous),
        );
        assert_eq!((summary.words_added, summary.words_removed), (2, 1));
    }

    #[test]
    fn works_the_counts_out_against_an_empty_text_when_nothing_came_before() {
        let summary = summarize(&version(), None);
        assert_eq!((summary.words_added, summary.words_removed), (3, 0));
    }

    #[test]
    fn does_not_trust_a_count_stored_without_its_pair() {
        let summary = summarize(
            &Version {
                words_added: Some(9),
                ..version()
            },
            None,
        );
        assert_eq!((summary.words_added, summary.words_removed), (3, 0));
    }
}
