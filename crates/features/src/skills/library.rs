use serde::{Deserialize, Serialize};

use super::scan_skills::{RootScan, Skill};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryRoot {
    pub root: String,
    pub readable: bool,
    pub skills: Vec<Skill>,
    pub label: String,
    pub removable: bool,
}

impl LibraryRoot {
    pub fn from_scan(scan: RootScan, label: impl Into<String>, removable: bool) -> Self {
        Self {
            root: scan.root,
            readable: scan.readable,
            skills: scan.skills,
            label: label.into(),
            removable,
        }
    }
}

pub type LibraryListing = Vec<LibraryRoot>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySnapshot {
    pub roots: LibraryListing,
    pub problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AddRootResult {
    Added { label: String },
    Duplicate { label: String },
    Cancelled,
    Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RemoveRootResult {
    Removed,
    Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSkillRequest {
    pub root: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CreateSkillResult {
    Created { path: String, name: String },
    Failed { reason: String },
}
