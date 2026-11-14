use serde::{Deserialize, Serialize};

use crate::files::parse_path_request;

pub const MIN_RUNS: u32 = 1;
pub const MAX_RUNS: u32 = 5;
pub const MAX_COST_LIMIT_USD: f64 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EvalBaseline {
    None,
    #[serde(rename_all = "camelCase")]
    Version {
        version_id: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SummaryBaseline {
    None,
    #[serde(rename_all = "camelCase")]
    Version {
        version_id: i64,
        at: i64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunRequest {
    pub skill_path: String,
    pub baseline: EvalBaseline,
    pub runs: u32,
    pub model: String,
    pub max_cost_usd: f64,
}

pub fn is_valid_model(model: &str) -> bool {
    let mut chars = model.chars();
    model.chars().count() <= 100
        && chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

pub fn parse_run_request(
    skill_path: &str,
    baseline: EvalBaseline,
    runs: i64,
    model: &str,
    max_cost_usd: f64,
) -> Option<RunRequest> {
    let skill_path = parse_path_request(skill_path)?;
    let baseline_ok = match baseline {
        EvalBaseline::None => true,
        EvalBaseline::Version { version_id } => version_id >= 1,
    };
    let valid = baseline_ok
        && (i64::from(MIN_RUNS)..=i64::from(MAX_RUNS)).contains(&runs)
        && is_valid_model(model)
        && max_cost_usd.is_finite()
        && max_cost_usd > 0.0
        && max_cost_usd <= MAX_COST_LIMIT_USD;
    valid.then(|| RunRequest {
        skill_path,
        baseline,
        runs: runs as u32,
        model: model.to_owned(),
        max_cost_usd,
    })
}

pub fn parse_summary_baseline(text: &str) -> Option<SummaryBaseline> {
    let parsed: SummaryBaseline = serde_json::from_str(text).ok()?;
    match parsed {
        SummaryBaseline::Version { version_id, at } if version_id < 1 || at < 0 => None,
        other => Some(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(runs: i64, model: &str, cost: f64, baseline: EvalBaseline) -> Option<RunRequest> {
        parse_run_request("/skills/pdf", baseline, runs, model, cost)
    }

    #[test]
    fn accepts_a_valid_request_and_a_version_baseline_by_id() {
        assert!(run(3, "haiku", 1.0, EvalBaseline::None).is_some());
        assert!(
            run(
                1,
                "claude-sonnet-4.5",
                5.0,
                EvalBaseline::Version { version_id: 7 }
            )
            .is_some()
        );
    }

    #[test]
    fn refuses_what_is_out_of_range() {
        assert!(run(0, "haiku", 1.0, EvalBaseline::None).is_none());
        assert!(run(6, "haiku", 1.0, EvalBaseline::None).is_none());
        assert!(run(3, "1haiku", 1.0, EvalBaseline::None).is_none());
        assert!(run(3, "ha iku", 1.0, EvalBaseline::None).is_none());
        assert!(run(3, "", 1.0, EvalBaseline::None).is_none());
        assert!(run(3, &format!("a{}", "b".repeat(100)), 1.0, EvalBaseline::None).is_none());
        assert!(run(3, "haiku", 0.0, EvalBaseline::None).is_none());
        assert!(run(3, "haiku", 5.01, EvalBaseline::None).is_none());
        assert!(run(3, "haiku", f64::NAN, EvalBaseline::None).is_none());
        assert!(run(3, "haiku", 1.0, EvalBaseline::Version { version_id: 0 }).is_none());
        assert!(parse_run_request("rel", EvalBaseline::None, 3, "haiku", 1.0).is_none());
    }

    #[test]
    fn reads_the_summary_baseline_of_a_stored_run() {
        assert_eq!(
            parse_summary_baseline(r#"{"kind":"none"}"#),
            Some(SummaryBaseline::None)
        );
        assert_eq!(
            parse_summary_baseline(r#"{"kind":"version","versionId":3,"at":1700}"#),
            Some(SummaryBaseline::Version {
                version_id: 3,
                at: 1700
            })
        );
        for bad in [
            "nope",
            r#"{"kind":"version","versionId":0,"at":1}"#,
            r#"{"kind":"other"}"#,
            r#"{"kind":"version","versionId":1}"#,
        ] {
            assert_eq!(parse_summary_baseline(bad), None, "{bad}");
        }
    }

    #[test]
    fn serializes_a_baseline_like_the_typescript_app() {
        let json = serde_json::to_string(&SummaryBaseline::Version {
            version_id: 3,
            at: 9,
        })
        .unwrap();
        assert_eq!(json, r#"{"kind":"version","versionId":3,"at":9}"#);
    }
}
