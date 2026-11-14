use serde::{Deserialize, Serialize};

pub type VersionId = i64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionOrigin {
    Ask,
    Review,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedChange {
    pub origin: SuggestionOrigin,
    pub instruction: String,
    pub rationale: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum VersionCause {
    Started,
    Edited,
    Accepted { changes: Vec<AcceptedChange> },
    Opened { name: String },
    Restored { from: VersionId },
    External,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CauseKind {
    Started,
    Edited,
    Accepted,
    Opened,
    Restored,
    External,
}

impl VersionCause {
    pub fn kind(&self) -> CauseKind {
        match self {
            VersionCause::Started => CauseKind::Started,
            VersionCause::Edited => CauseKind::Edited,
            VersionCause::Accepted { .. } => CauseKind::Accepted,
            VersionCause::Opened { .. } => CauseKind::Opened,
            VersionCause::Restored { .. } => CauseKind::Restored,
            VersionCause::External => CauseKind::External,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: VersionId,
    pub at: i64,
    pub cause: VersionCause,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words_added: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words_removed: Option<usize>,
}

pub type History = Vec<Version>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionSummary {
    pub id: VersionId,
    pub at: i64,
    pub cause: VersionCause,
    pub words: usize,
    pub words_added: usize,
    pub words_removed: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved: Option<String>,
}

pub type VersionList = Vec<VersionSummary>;
