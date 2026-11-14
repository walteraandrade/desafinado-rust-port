use std::collections::HashMap;

use super::types::{CauseKind, VersionId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThinCandidate {
    pub id: VersionId,
    pub at: i64,
    pub kind: CauseKind,
    pub saved: bool,
}

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 24 * HOUR_MS;
const HOURLY_AFTER_MS: i64 = 7 * DAY_MS;
const DAILY_AFTER_MS: i64 = 90 * DAY_MS;

fn bucket_of(age: i64, at: i64) -> Option<i64> {
    if age >= DAILY_AFTER_MS {
        Some(at.div_euclid(DAY_MS) * 2)
    } else if age >= HOURLY_AFTER_MS {
        Some(at.div_euclid(HOUR_MS) * 2 + 1)
    } else {
        None
    }
}

pub fn ids_to_thin(versions: &[ThinCandidate], now: i64) -> Vec<VersionId> {
    let newest = versions.iter().map(|v| v.id).fold(0, VersionId::max);
    let thinnable: Vec<&ThinCandidate> = versions
        .iter()
        .filter(|v| v.kind == CauseKind::Edited && !v.saved)
        .collect();
    let last_in_bucket = thinnable.iter().fold(HashMap::new(), |mut kept, v| {
        if let Some(bucket) = bucket_of(now - v.at, v.at) {
            let entry = kept.entry(bucket).or_insert(0);
            *entry = (*entry).max(v.id);
        }
        kept
    });
    thinnable
        .into_iter()
        .filter(|v| {
            v.id != newest
                && bucket_of(now - v.at, v.at)
                    .is_some_and(|bucket| last_in_bucket.get(&bucket) != Some(&v.id))
        })
        .map(|v| v.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600_000;
    const DAY: i64 = 24 * HOUR;
    const NOW: i64 = 200 * DAY;

    fn edited(id: i64, at: i64) -> ThinCandidate {
        ThinCandidate {
            id,
            at,
            kind: CauseKind::Edited,
            saved: false,
        }
    }

    #[test]
    fn keeps_every_version_younger_than_7_days() {
        let versions: Vec<_> = (1..=4)
            .map(|id| edited(id, NOW - DAY - id * 60_000))
            .collect();
        assert_eq!(ids_to_thin(&versions, NOW), Vec::<i64>::new());
    }

    #[test]
    fn keeps_the_last_edited_version_of_each_hour_between_7_and_90_days() {
        let base = NOW - 10 * DAY;
        let versions = [
            edited(1, base),
            edited(2, base + 10 * 60_000),
            edited(3, base + 20 * 60_000),
            edited(4, base + HOUR),
        ];
        assert_eq!(ids_to_thin(&versions, NOW), vec![1, 2]);
    }

    #[test]
    fn keeps_the_last_edited_version_of_each_day_after_90_days() {
        let base = NOW - 120 * DAY;
        let versions = [
            edited(1, base),
            edited(2, base + 3 * HOUR),
            edited(3, base + 5 * HOUR),
            edited(4, base + DAY),
        ];
        assert_eq!(ids_to_thin(&versions, NOW), vec![1, 2]);
    }

    #[test]
    fn never_thins_a_version_that_is_not_edited_or_that_was_saved() {
        let base = NOW - 10 * DAY;
        let candidate = |id, at, kind, saved| ThinCandidate {
            id,
            at,
            kind,
            saved,
        };
        let versions = [
            candidate(1, base, CauseKind::Accepted, false),
            candidate(2, base + 1000, CauseKind::External, false),
            candidate(3, base + 2000, CauseKind::Edited, true),
            candidate(4, base + 3000, CauseKind::Restored, false),
            candidate(5, base + 4000, CauseKind::Opened, false),
            edited(6, base + 5000),
        ];
        assert_eq!(ids_to_thin(&versions, NOW), Vec::<i64>::new());
    }

    #[test]
    fn never_thins_the_newest_version() {
        let base = NOW - 10 * DAY;
        assert_eq!(
            ids_to_thin(&[edited(1, base), edited(2, base + 1000)], NOW),
            vec![1]
        );
        assert_eq!(
            ids_to_thin(&[edited(1, base), edited(2, base + HOUR)], NOW),
            Vec::<i64>::new()
        );
    }

    #[test]
    fn thins_around_a_saved_version_and_keeps_the_saved_one() {
        let base = NOW - 10 * DAY;
        let versions = [
            edited(1, base),
            ThinCandidate {
                id: 2,
                at: base + 1000,
                kind: CauseKind::Edited,
                saved: true,
            },
            edited(3, base + 2000),
            edited(4, base + 3000),
        ];
        assert_eq!(ids_to_thin(&versions, NOW), vec![1, 3]);
    }
}
