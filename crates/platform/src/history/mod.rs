mod db;
mod types;
mod word_delta;

pub use db::{HistoryDb, HistoryError, HistoryStore, SCHEMA_VERSION};
pub use types::{
    AcceptedChange, MarkSavedInput, RecordInput, Unsaved, VersionCause, VersionId, VersionSummary,
};
pub use word_delta::{WordDelta, count_words, word_delta};
