use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use desafinado_core::coalescing_writer::CoalescingWriter;
use desafinado_features::history::{
    store::{HistoryStore, MarkSavedInput, RecordInput, StoreError},
    types::{
        AcceptedChange, SuggestionOrigin, VersionCause, VersionId, VersionList, VersionSummary,
    },
    untitled::current_untitled,
};
use desafinado_platform::history::{self as db, HistoryDb, HistoryStore as _};

use crate::document::SCRATCH_KEY;

pub struct DbHistoryStore {
    db: Arc<HistoryDb>,
    now: Box<dyn Fn() -> i64 + Send + Sync>,
    thinned: Mutex<HashSet<String>>,
    /// Writes handed to the background writer. Every use of the store waits for them first,
    /// so a read sees them and a write cannot land before them.
    writer: CoalescingWriter,
}

impl DbHistoryStore {
    pub fn new(
        db: Arc<HistoryDb>,
        now: impl Fn() -> i64 + Send + Sync + 'static,
        writer: CoalescingWriter,
    ) -> Self {
        Self {
            db,
            now: Box::new(now),
            thinned: Mutex::new(HashSet::new()),
            writer,
        }
    }
}

fn failed(error: db::HistoryError) -> StoreError {
    StoreError(error.0)
}

fn origin_of(origin: &str) -> SuggestionOrigin {
    if origin == "review" {
        SuggestionOrigin::Review
    } else {
        SuggestionOrigin::Ask
    }
}

fn origin_name(origin: SuggestionOrigin) -> String {
    match origin {
        SuggestionOrigin::Ask => "ask",
        SuggestionOrigin::Review => "review",
    }
    .to_string()
}

pub fn cause_from_db(cause: db::VersionCause) -> VersionCause {
    match cause {
        db::VersionCause::Started => VersionCause::Started,
        db::VersionCause::Edited => VersionCause::Edited,
        db::VersionCause::External => VersionCause::External,
        db::VersionCause::Opened { name } => VersionCause::Opened { name },
        db::VersionCause::Restored { from } => VersionCause::Restored { from },
        db::VersionCause::Accepted { changes } => VersionCause::Accepted {
            changes: changes
                .into_iter()
                .map(|change| AcceptedChange {
                    origin: origin_of(&change.origin),
                    instruction: change.instruction,
                    rationale: change.rationale,
                })
                .collect(),
        },
    }
}

pub fn cause_to_db(cause: VersionCause) -> db::VersionCause {
    match cause {
        VersionCause::Started => db::VersionCause::Started,
        VersionCause::Edited => db::VersionCause::Edited,
        VersionCause::External => db::VersionCause::External,
        VersionCause::Opened { name } => db::VersionCause::Opened { name },
        VersionCause::Restored { from } => db::VersionCause::Restored { from },
        VersionCause::Accepted { changes } => db::VersionCause::Accepted {
            changes: changes
                .into_iter()
                .map(|change| db::AcceptedChange {
                    origin: origin_name(change.origin),
                    instruction: change.instruction,
                    rationale: change.rationale,
                })
                .collect(),
        },
    }
}

fn count(value: i64) -> usize {
    usize::try_from(value).unwrap_or(0)
}

pub fn summary_from_db(summary: db::VersionSummary) -> VersionSummary {
    VersionSummary {
        id: summary.id,
        at: summary.at,
        cause: cause_from_db(summary.cause),
        words: count(summary.words),
        words_added: count(summary.words_added),
        words_removed: count(summary.words_removed),
        saved: summary.saved,
    }
}

impl HistoryStore for DbHistoryStore {
    fn list(&self, key: &str) -> Result<VersionList, StoreError> {
        self.writer.flush();
        let first = self
            .thinned
            .lock()
            .is_ok_and(|mut thinned| thinned.insert(key.to_string()));
        if first {
            self.db.thin(key, (self.now)()).map_err(failed)?;
        }
        let versions: VersionList = self
            .db
            .list(key)
            .map(|list| list.into_iter().map(summary_from_db).collect())
            .map_err(failed)?;
        Ok(if key == SCRATCH_KEY {
            current_untitled(versions)
        } else {
            versions
        })
    }

    fn text(&self, key: &str, id: VersionId) -> Result<Option<String>, StoreError> {
        self.writer.flush();
        self.db.text(key, id).map_err(failed)
    }

    fn record(&self, key: &str, input: RecordInput) -> Result<(), StoreError> {
        self.writer.flush();
        self.db
            .record(
                key,
                &db::RecordInput {
                    at: input.at,
                    cause: cause_to_db(input.cause),
                    text: input.text,
                },
            )
            .map_err(failed)
    }

    fn mark_saved(&self, key: &str, input: MarkSavedInput) -> Result<(), StoreError> {
        self.writer.flush();
        self.db
            .mark_saved(
                key,
                &db::MarkSavedInput {
                    text: input.text,
                    name: input.name,
                    at: input.at,
                },
            )
            .map_err(failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, DbHistoryStore) {
        let (dir, store, _) = store_with_writer();
        (dir, store)
    }

    fn store_with_writer() -> (tempfile::TempDir, DbHistoryStore, CoalescingWriter) {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(&dir.path().join("history.sqlite")).unwrap();
        let writer = CoalescingWriter::default();
        let store = DbHistoryStore::new(Arc::new(db), || 1_000, writer.clone());
        (dir, store, writer)
    }

    #[test]
    fn a_read_sees_the_writes_still_waiting_in_the_background_writer() {
        let (_dir, store, writer) = store_with_writer();
        let store = Arc::new(store);
        let background = Arc::clone(&store);
        let _never_run = writer.submit("history:1", move || {
            background
                .record(
                    "/a.md",
                    RecordInput {
                        at: 1_000,
                        cause: VersionCause::Edited,
                        text: "typed".into(),
                    },
                )
                .unwrap();
        });
        let listed = store.list("/a.md").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].cause, VersionCause::Started);
    }

    fn record(store: &DbHistoryStore, key: &str, cause: VersionCause, text: &str) {
        store
            .record(
                key,
                RecordInput {
                    at: 1_000,
                    cause,
                    text: text.into(),
                },
            )
            .unwrap();
    }

    #[test]
    fn an_untitled_document_lists_only_its_own_versions() {
        let (_dir, store) = store();
        let opened = || VersionCause::Opened {
            name: "Untitled".into(),
        };
        for key in [SCRATCH_KEY, "/a.md"] {
            record(&store, key, VersionCause::Edited, "first draft");
            record(&store, key, opened(), "");
            record(&store, key, VersionCause::Edited, "second draft");
        }
        let untitled = store.list(SCRATCH_KEY).unwrap();
        assert_eq!(untitled.len(), 2);
        assert_eq!(untitled[0].cause, opened());
        assert_eq!(untitled[1].cause, VersionCause::Edited);
        assert_eq!(store.list("/a.md").unwrap().len(), 3);
    }

    #[test]
    fn records_and_lists_versions_per_key() {
        let (_dir, store) = store();
        record(&store, "/a.md", VersionCause::Edited, "one");
        record(&store, "/a.md", VersionCause::External, "one two");
        record(&store, "/b.md", VersionCause::Edited, "b");
        let listed = store.list("/a.md").unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].cause, VersionCause::Started);
        assert_eq!(listed[1].cause, VersionCause::External);
        assert_eq!(listed[1].words, 2);
        assert_eq!(listed[1].words_added, 1);
        assert_eq!(store.list("/b.md").unwrap().len(), 1);
    }

    #[test]
    fn keeps_accepted_changes_with_their_origin() {
        let (_dir, store) = store();
        record(&store, "k", VersionCause::Edited, "a");
        let changes = vec![
            AcceptedChange {
                origin: SuggestionOrigin::Review,
                instruction: "Clarity".into(),
                rationale: "Why.".into(),
            },
            AcceptedChange {
                origin: SuggestionOrigin::Ask,
                instruction: "Shorter".into(),
                rationale: "Less.".into(),
            },
        ];
        record(
            &store,
            "k",
            VersionCause::Accepted {
                changes: changes.clone(),
            },
            "b",
        );
        assert_eq!(
            store.list("k").unwrap()[1].cause,
            VersionCause::Accepted { changes }
        );
    }

    #[test]
    fn loads_the_text_of_a_version_and_marks_it_saved() {
        let (_dir, store) = store();
        record(&store, "k", VersionCause::Edited, "alpha");
        let id = store.list("k").unwrap()[0].id;
        assert_eq!(store.text("k", id).unwrap().as_deref(), Some("alpha"));
        store
            .mark_saved(
                "k",
                MarkSavedInput {
                    text: "alpha".into(),
                    name: "a.md".into(),
                    at: 2_000,
                },
            )
            .unwrap();
        assert_eq!(store.list("k").unwrap()[0].saved.as_deref(), Some("a.md"));
    }

    #[test]
    fn skips_a_version_with_the_same_text_as_the_last() {
        let (_dir, store) = store();
        record(&store, "k", VersionCause::Edited, "same");
        record(&store, "k", VersionCause::Edited, "same");
        assert_eq!(store.list("k").unwrap().len(), 1);
    }

    #[test]
    fn converts_every_cause_both_ways() {
        let causes = [
            VersionCause::Started,
            VersionCause::Edited,
            VersionCause::External,
            VersionCause::Opened {
                name: "a.md".into(),
            },
            VersionCause::Restored { from: 3 },
        ];
        for cause in causes {
            assert_eq!(cause_from_db(cause_to_db(cause.clone())), cause);
        }
    }

    use desafinado_features::history::session::{Applied, DeferredWrites};

    type History = desafinado_features::history::session::DocumentHistory;

    fn history_on(store: DbHistoryStore) -> (History, Arc<DbHistoryStore>) {
        use desafinado_features::history::session::DocumentHistoryOptions;
        let store = Arc::new(store);
        let mut options = DocumentHistoryOptions::new(Arc::clone(&store) as _, "/a.md", 30_000);
        options.initial = Some(Vec::new());
        (History::new(options), store)
    }

    type Results = std::sync::mpsc::Receiver<Applied>;

    fn submit(
        writer: &CoalescingWriter,
        key: &str,
        deferred: DeferredWrites,
        sender: &std::sync::mpsc::Sender<Applied>,
    ) {
        let sender = sender.clone();
        let _never_run = writer.submit(key, move || {
            sender.send(deferred.apply()).unwrap();
        });
    }

    fn shown(
        history: &mut History,
        writer: &CoalescingWriter,
        results: &Results,
    ) -> Vec<(VersionCause, Option<String>)> {
        writer.flush();
        for applied in results.try_iter() {
            history.wrote(applied);
        }
        history
            .snapshot()
            .iter()
            .map(|v| (v.cause.clone(), v.saved.clone()))
            .collect()
    }

    fn save(
        history: &mut History,
        writer: &CoalescingWriter,
        text: &str,
        sender: &std::sync::mpsc::Sender<Applied>,
    ) {
        let saving = history.saving_deferred(text, 2_000);
        submit(writer, "history:1", saving, sender);
        let saved = history.saved_deferred(text, "a.md", 3_000);
        submit(writer, "history:2", saved, sender);
    }

    #[test]
    fn a_save_written_in_the_background_lists_the_edit_before_the_saved_mark() {
        let (_dir, store, writer) = store_with_writer();
        let (mut history, _store) = history_on(store);
        let (sender, results) = std::sync::mpsc::channel();
        history.edited("typed words", 1_000);
        save(&mut history, &writer, "typed words", &sender);
        assert_eq!(
            shown(&mut history, &writer, &results),
            vec![(VersionCause::Started, Some("a.md".to_string()))]
        );
    }

    #[test]
    fn an_edit_after_a_save_is_listed_after_the_saved_version() {
        let (_dir, store, writer) = store_with_writer();
        let (mut history, _store) = history_on(store);
        let (sender, results) = std::sync::mpsc::channel();
        save(&mut history, &writer, "one", &sender);
        history.edited("one two", 4_000);
        let edit = history.poll_deferred(4_000 + 30_000).expect("due");
        submit(&writer, "history:3", edit, &sender);
        assert_eq!(
            shown(&mut history, &writer, &results),
            vec![
                (VersionCause::Started, Some("a.md".to_string())),
                (VersionCause::Edited, None)
            ]
        );
    }

    #[test]
    fn quitting_right_after_a_save_writes_both_versions() {
        let (_dir, store, writer) = store_with_writer();
        let (mut history, store) = history_on(store);
        let (sender, _results) = std::sync::mpsc::channel();
        save(&mut history, &writer, "kept", &sender);
        writer.flush();
        let listed = store.list("/a.md").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].saved.as_deref(), Some("a.md"));
    }
}
