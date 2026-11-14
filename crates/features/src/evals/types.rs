use serde::{Deserialize, Serialize};

use super::baseline::RunBaseline;
use super::report::EvalSummary;
use super::risks::Risk;
use super::settings::EvalSettings;
use crate::history::types::VersionList;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalSkillRequest {
    pub skill_path: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalPrepareRequest {
    pub skill_path: String,
    pub baseline: RunBaseline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportPart {
    Today,
    Version,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalOpenReportRequest {
    pub skill_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<ReportPart>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalRunRequest {
    pub skill_path: String,
    pub baseline: RunBaseline,
    #[serde(flatten)]
    pub settings: EvalSettings,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoppedRun {
    pub message: String,
    pub cost_usd: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum EvalStatus {
    #[serde(rename_all = "camelCase")]
    Ready {
        claude_version: String,
        case_count: usize,
        last: Option<EvalSummary>,
        running: bool,
        cancelling: bool,
        running_skill: Option<String>,
        stopped: Option<StoppedRun>,
    },
    NoCli,
    Failed {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedReady {
    pub claude_version: String,
    pub case_count: usize,
    pub risks: Vec<Risk>,
    pub baseline_risks: Option<Vec<Risk>>,
    pub versions: VersionList,
    pub same_as_saved: Vec<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum EvalPrepareResult {
    Ready(PreparedReady),
    NoCli,
    Failed { reason: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum EvalRunResult {
    Completed {
        summary: EvalSummary,
    },
    #[serde(rename_all = "camelCase")]
    NotComplete {
        message: String,
        cost_usd: Option<f64>,
    },
    RisksChanged,
    Busy,
    NoCli,
    Failed {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum EvalCancelResult {
    Cancelling,
    Idle,
    Failed { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum EvalOpenReportResult {
    Opened,
    Failed { reason: String },
}

pub trait EvalsKit {
    fn status(&self, request: &EvalSkillRequest) -> EvalStatus;
    fn prepare(&self, request: &EvalPrepareRequest) -> EvalPrepareResult;
    fn run(&self, request: &EvalRunRequest) -> EvalRunResult;
    fn open_report(&self, request: &EvalOpenReportRequest) -> EvalOpenReportResult;
    fn cancel(&self) -> EvalCancelResult;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_names_match_the_typescript_contract() {
        let result = EvalRunResult::NotComplete {
            message: "Cancelled".into(),
            cost_usd: Some(0.5),
        };
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            serde_json::json!({"kind": "not-complete", "message": "Cancelled", "costUsd": 0.5})
        );
        let status: EvalStatus =
            serde_json::from_value(serde_json::json!({"kind": "no-cli"})).unwrap();
        assert_eq!(status, EvalStatus::NoCli);
        let prepared: EvalPrepareResult = serde_json::from_value(serde_json::json!({
            "kind": "ready", "claudeVersion": "1", "caseCount": 2, "risks": [],
            "baselineRisks": null, "versions": [], "sameAsSaved": []
        }))
        .unwrap();
        assert!(matches!(
            prepared,
            EvalPrepareResult::Ready(PreparedReady { case_count: 2, .. })
        ));
    }
}
