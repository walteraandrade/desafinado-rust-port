use serde::{Deserialize, Serialize};

pub type VersionId = i64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptedChange {
    pub origin: String,
    pub instruction: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum VersionCause {
    Started,
    Edited,
    Accepted { changes: Vec<AcceptedChange> },
    Opened { name: String },
    Restored { from: VersionId },
    External,
}

impl VersionCause {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Edited => "edited",
            Self::Accepted { .. } => "accepted",
            Self::Opened { .. } => "opened",
            Self::Restored { .. } => "restored",
            Self::External => "external",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionSummary {
    pub id: VersionId,
    pub at: i64,
    pub cause: VersionCause,
    pub words: i64,
    pub words_added: i64,
    pub words_removed: i64,
    pub saved: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordInput {
    pub at: i64,
    pub cause: VersionCause,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkSavedInput {
    pub text: String,
    pub name: String,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unsaved {
    pub text: String,
    pub base: String,
}
