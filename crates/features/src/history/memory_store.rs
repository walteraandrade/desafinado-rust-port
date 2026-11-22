use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use super::history::{VersionInput, mark_saved, record_version};
use super::store::{HistoryStore, MarkSavedInput, RecordInput, StoreError};
use super::summary::summarize;
use super::types::{History, VersionId, VersionList};

#[derive(Default)]
pub struct MemoryHistoryStore {
    histories: Mutex<HashMap<String, History>>,
}

impl MemoryHistoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn with<T>(&self, key: &str, run: impl FnOnce(&History) -> T) -> T {
        let histories = self
            .histories
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        run(histories.get(key).unwrap_or(&Vec::new()))
    }

    fn update(&self, key: &str, run: impl FnOnce(&History) -> History) {
        let mut histories = self
            .histories
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let next = run(histories.get(key).unwrap_or(&Vec::new()));
        histories.insert(key.to_string(), next);
    }
}

impl HistoryStore for MemoryHistoryStore {
    fn list(&self, key: &str) -> Result<VersionList, StoreError> {
        Ok(self.with(key, |history| {
            history
                .iter()
                .enumerate()
                .map(|(index, version)| {
                    summarize(
                        version,
                        index.checked_sub(1).map(|previous| &history[previous]),
                    )
                })
                .collect()
        }))
    }

    fn text(&self, key: &str, id: VersionId) -> Result<Option<String>, StoreError> {
        Ok(self.with(key, |history| {
            history
                .iter()
                .find(|version| version.id == id)
                .map(|version| version.text.clone())
        }))
    }

    fn record(&self, key: &str, input: RecordInput) -> Result<(), StoreError> {
        self.update(key, |history| {
            record_version(
                history,
                VersionInput {
                    text: input.text,
                    cause: input.cause,
                    at: input.at,
                },
            )
        });
        Ok(())
    }

    fn mark_saved(&self, key: &str, input: MarkSavedInput) -> Result<(), StoreError> {
        self.update(key, |history| {
            mark_saved(history, &input.text, &input.name, input.at)
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::history::MAX_VERSIONS;
    use crate::history::store_contract::{StoreContractOptions, run_store_contract};

    #[test]
    fn implements_the_history_store_contract() {
        run_store_contract(
            "the in-memory store",
            &|| Box::new(MemoryHistoryStore::new()),
            StoreContractOptions {
                cap: Some(MAX_VERSIONS),
                per_file: true,
            },
        );
    }
}
