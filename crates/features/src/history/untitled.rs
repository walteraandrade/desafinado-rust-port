//! Every Untitled Document shares one History key, so its versions follow each other in one
//! list. Each Untitled Document begins with a `Started` or `Opened` version; the versions before
//! the last one belong to earlier Untitled Documents and stay stored, out of the Timeline.

use super::types::{CauseKind, VersionList};

/// The versions of the current Untitled Document: the last `Started` or `Opened` version and
/// every version after it.
pub fn current_untitled(versions: VersionList) -> VersionList {
    let start = versions
        .iter()
        .rposition(|version| matches!(version.cause.kind(), CauseKind::Started | CauseKind::Opened))
        .unwrap_or(0);
    versions.into_iter().skip(start).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::types::{VersionCause, VersionSummary};

    fn version(id: i64, cause: VersionCause) -> VersionSummary {
        VersionSummary {
            id,
            at: id * 1_000,
            cause,
            words: 0,
            words_added: 0,
            words_removed: 0,
            saved: None,
        }
    }

    fn opened(id: i64) -> VersionSummary {
        version(
            id,
            VersionCause::Opened {
                name: "Untitled".into(),
            },
        )
    }

    fn ids(versions: &VersionList) -> Vec<i64> {
        versions.iter().map(|version| version.id).collect()
    }

    #[test]
    fn keeps_only_the_versions_since_the_last_new_document() {
        let versions = vec![
            version(1, VersionCause::Started),
            version(2, VersionCause::Edited),
            opened(3),
            version(4, VersionCause::Edited),
            opened(5),
            version(6, VersionCause::Edited),
        ];
        assert_eq!(ids(&current_untitled(versions)), vec![5, 6]);
    }

    #[test]
    fn a_new_document_just_opened_shows_one_version() {
        let versions = vec![version(1, VersionCause::Edited), opened(2)];
        assert_eq!(ids(&current_untitled(versions)), vec![2]);
    }

    #[test]
    fn a_launch_without_unsaved_text_starts_a_new_document() {
        let versions = vec![
            opened(1),
            version(2, VersionCause::Edited),
            version(3, VersionCause::Started),
        ];
        assert_eq!(ids(&current_untitled(versions)), vec![3]);
    }

    #[test]
    fn accepted_and_restored_versions_stay_in_the_same_document() {
        let versions = vec![
            opened(1),
            version(2, VersionCause::Accepted { changes: vec![] }),
            version(3, VersionCause::Restored { from: 1 }),
        ];
        assert_eq!(ids(&current_untitled(versions)), vec![1, 2, 3]);
    }

    #[test]
    fn keeps_every_version_when_none_starts_a_document() {
        let versions = vec![
            version(1, VersionCause::Edited),
            version(2, VersionCause::Edited),
        ];
        assert_eq!(ids(&current_untitled(versions)), vec![1, 2]);
    }
}
