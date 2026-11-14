use std::fmt;

use super::types::{VersionCause, VersionId, VersionList};

pub type HistoryKey = String;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreError(pub String);

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StoreError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordInput {
    pub at: i64,
    pub cause: VersionCause,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkSavedInput {
    pub text: String,
    pub name: String,
    pub at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryWrite {
    VersionRecorded(RecordInput),
    VersionMarkedSaved(MarkSavedInput),
}

pub trait HistoryStore: Send + Sync {
    fn list(&self, key: &str) -> Result<VersionList, StoreError>;
    fn text(&self, key: &str, id: VersionId) -> Result<Option<String>, StoreError>;
    fn record(&self, key: &str, input: RecordInput) -> Result<(), StoreError>;
    fn mark_saved(&self, key: &str, input: MarkSavedInput) -> Result<(), StoreError>;
}

pub fn apply_write(
    store: &dyn HistoryStore,
    key: &str,
    write: HistoryWrite,
) -> Result<(), StoreError> {
    match write {
        HistoryWrite::VersionRecorded(input) => store.record(key, input),
        HistoryWrite::VersionMarkedSaved(input) => store.mark_saved(key, input),
    }
}
