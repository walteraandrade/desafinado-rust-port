use std::collections::HashMap;

use super::types::VersionId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinCandidate {
    pub id: VersionId,
    pub at: i64,
    pub kind: String,
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
    let newest = versions.iter().map(|v| v.id).max().unwrap_or(0).max(0);
    let thinnable: Vec<&ThinCandidate> = versions
        .iter()
        .filter(|v| v.kind == "edited" && !v.saved)
        .collect();
    let mut last_in_bucket: HashMap<i64, VersionId> = HashMap::new();
    for version in &thinnable {
        if let Some(bucket) = bucket_of(now - version.at, version.at) {
            let entry = last_in_bucket.entry(bucket).or_insert(0);
            *entry = (*entry).max(version.id);
        }
    }
    thinnable
        .iter()
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
            kind: "edited".into(),
            saved: false,
        }
    }

    fn other(id: i64, at: i64, kind: &str, saved: bool) -> ThinCandidate {
        ThinCandidate {
            id,
            at,
            kind: kind.into(),
            saved,
        }
    }

    #[test]
    fn keeps_every_version_younger_than_7_days() {
        let versions: Vec<_> = (1..=4)
            .map(|id| edited(id, NOW - DAY - id * 60_000))
            .collect();
        assert!(ids_to_thin(&versions, NOW).is_empty());
    }

    #[test]
    fn keeps_the_last_edited_version_of_each_hour_between_7_and_90_days() {
        let base = NOW - 10 * DAY;
        let versions = vec![
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
        let versions = vec![
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
        let versions = vec![
            other(1, base, "accepted", false),
            other(2, base + 1000, "external", false),
            other(3, base + 2000, "edited", true),
            other(4, base + 3000, "restored", false),
            other(5, base + 4000, "opened", false),
            edited(6, base + 5000),
        ];
        assert!(ids_to_thin(&versions, NOW).is_empty());
    }

    #[test]
    fn never_thins_the_newest_version() {
        let base = NOW - 10 * DAY;
        assert_eq!(
            ids_to_thin(&[edited(1, base), edited(2, base + 1000)], NOW),
            vec![1]
        );
        assert!(ids_to_thin(&[edited(1, base), edited(2, base + HOUR)], NOW).is_empty());
    }

    #[test]
    fn thins_around_a_saved_version_and_keeps_the_saved_one() {
        let base = NOW - 10 * DAY;
        let versions = vec![
            edited(1, base),
            other(2, base + 1000, "edited", true),
            edited(3, base + 2000),
            edited(4, base + 3000),
        ];
        assert_eq!(ids_to_thin(&versions, NOW), vec![1, 3]);
    }
}
