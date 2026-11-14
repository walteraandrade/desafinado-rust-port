use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::history::story::describe_version;
use crate::history::types::VersionSummary;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RunBaseline {
    None,
    #[serde(rename_all = "camelCase")]
    Version {
        version_id: i64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SummaryBaseline {
    None,
    #[serde(rename_all = "camelCase")]
    Version {
        version_id: i64,
        at: i64,
    },
}

pub const NO_BASELINE: SummaryBaseline = SummaryBaseline::None;

pub const TODAY_LABEL: &str = "today's SKILL.md";

fn integer(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| {
        value
            .as_f64()
            .filter(|number| number.fract() == 0.0 && number.abs() < 9.0e15)
            .map(|number| number as i64)
    })
}

pub fn parse_summary_baseline(text: &str) -> Option<SummaryBaseline> {
    let parsed: Value = serde_json::from_str(text).ok()?;
    let fields = parsed.as_object()?;
    match fields.get("kind")?.as_str()? {
        "none" if fields.len() == 1 => Some(SummaryBaseline::None),
        "version" if fields.len() == 3 => {
            let version_id = integer(fields.get("versionId")?).filter(|id| *id >= 1)?;
            let at = integer(fields.get("at")?).filter(|at| *at >= 0)?;
            Some(SummaryBaseline::Version { version_id, at })
        }
        _ => None,
    }
}

pub fn version_option_label(
    versions: &[VersionSummary],
    version: &VersionSummary,
    format_time: impl Fn(i64) -> String,
    same_as_saved: bool,
) -> String {
    let description = describe_version(versions, version, &format_time);
    format!(
        "{} · {}: {}{}",
        format_time(version.at),
        description.label,
        description.headline,
        if same_as_saved {
            " (same as saved)"
        } else {
            ""
        }
    )
}

pub fn version_label(at: i64, format_time: impl Fn(i64) -> String) -> String {
    format!("the Version from {}", format_time(at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::types::VersionCause;

    fn edited() -> VersionSummary {
        VersionSummary {
            id: 2,
            at: 1000,
            cause: VersionCause::Edited,
            words: 10,
            words_added: 3,
            words_removed: 0,
            saved: None,
        }
    }

    fn time(at: i64) -> String {
        format!("t{at}")
    }

    #[test]
    fn reads_no_baseline_and_a_version_with_its_time() {
        assert_eq!(
            parse_summary_baseline("{\"kind\":\"none\"}"),
            Some(SummaryBaseline::None)
        );
        assert_eq!(
            parse_summary_baseline("{\"kind\":\"version\",\"versionId\":4,\"at\":99}"),
            Some(SummaryBaseline::Version {
                version_id: 4,
                at: 99
            })
        );
    }

    #[test]
    fn refuses_anything_else() {
        assert_eq!(parse_summary_baseline("nope"), None);
        assert_eq!(
            parse_summary_baseline("{\"kind\":\"version\",\"versionId\":4}"),
            None
        );
        assert_eq!(parse_summary_baseline("{\"kind\":\"none\",\"x\":1}"), None);
        assert_eq!(parse_summary_baseline("{\"kind\":\"other\"}"), None);
    }

    #[test]
    fn describes_a_version_as_the_timeline_does_with_its_time() {
        let version = edited();
        assert_eq!(
            version_option_label(std::slice::from_ref(&version), &version, time, false),
            "t1000 · You: You added 3 words"
        );
    }

    #[test]
    fn marks_a_version_whose_text_is_the_saved_one() {
        let version = edited();
        assert_eq!(
            version_option_label(std::slice::from_ref(&version), &version, time, true),
            "t1000 · You: You added 3 words (same as saved)"
        );
    }

    #[test]
    fn names_both_sides_of_the_comparison() {
        assert_eq!(TODAY_LABEL, "today's SKILL.md");
        assert_eq!(version_label(5, time), "the Version from t5");
    }
}
