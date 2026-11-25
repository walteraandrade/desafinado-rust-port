use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use super::events::EventSink;
use super::recorder::{AcceptedInput, HistoryRecorder, LazyEdit};
use super::store::{HistoryKey, HistoryStore, HistoryWrite, StoreError, apply_write};
use super::types::{VersionId, VersionList};

pub const TEXT_CACHE_SIZE: usize = 200;

pub struct DocumentHistoryOptions {
    pub store: Arc<dyn HistoryStore>,
    pub key: HistoryKey,
    pub initial: Option<VersionList>,
    pub idle_ms: i64,
    pub on_error: Box<dyn FnMut(&StoreError) + Send>,
    pub on_written: Option<Box<dyn FnMut() + Send>>,
    pub cache_size: usize,
}

impl DocumentHistoryOptions {
    pub fn new(store: Arc<dyn HistoryStore>, key: impl Into<HistoryKey>, idle_ms: i64) -> Self {
        Self {
            store,
            key: key.into(),
            initial: None,
            idle_ms,
            on_error: Box::new(|_| {}),
            on_written: None,
            cache_size: TEXT_CACHE_SIZE,
        }
    }
}

pub struct DocumentHistory {
    store: Arc<dyn HistoryStore>,
    key: HistoryKey,
    recorder: HistoryRecorder,
    versions: VersionList,
    listing_wanted: bool,
    texts: HashMap<VersionId, String>,
    text_order: VecDeque<VersionId>,
    cache_size: usize,
    on_error: Box<dyn FnMut(&StoreError) + Send>,
    on_written: Option<Box<dyn FnMut() + Send>>,
}

fn same_versions(left: &VersionList, right: &VersionList) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(a, b)| a.id == b.id && a.saved == b.saved)
}

/// Writes that came due, taken out of the History to be applied apart from the UI thread.
pub struct DeferredWrites {
    store: Arc<dyn HistoryStore>,
    key: HistoryKey,
    edit: LazyEdit,
}

impl DeferredWrites {
    /// The History key the write is for.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Reads the text and writes the version; returns the error, if it failed. The text is
    /// read here, so run this off the UI thread.
    pub fn apply(self) -> Vec<StoreError> {
        apply_write(self.store.as_ref(), &self.key, self.edit.into_write())
            .err()
            .into_iter()
            .collect()
    }
}

impl DocumentHistory {
    pub fn new(options: DocumentHistoryOptions) -> Self {
        let DocumentHistoryOptions {
            store,
            key,
            initial,
            idle_ms,
            on_error,
            on_written,
            cache_size,
        } = options;
        Self {
            store,
            key,
            recorder: HistoryRecorder::new(idle_ms),
            listing_wanted: initial.is_none(),
            versions: initial.unwrap_or_default(),
            texts: HashMap::new(),
            text_order: VecDeque::new(),
            cache_size,
            on_error,
            on_written,
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn snapshot(&self) -> &VersionList {
        &self.versions
    }

    pub fn next_due(&self) -> Option<i64> {
        self.recorder.next_due()
    }

    fn send(&mut self, writes: Vec<HistoryWrite>) {
        for write in writes {
            match apply_write(self.store.as_ref(), &self.key, write) {
                Ok(()) => {
                    if let Some(written) = self.on_written.as_mut() {
                        written();
                    }
                }
                Err(error) => (self.on_error)(&error),
            }
            self.listing_wanted = true;
        }
    }

    pub fn edited(&mut self, text: impl Into<String>, now: i64) {
        self.recorder.edited(text.into(), now);
    }

    pub fn edited_with(&mut self, read: impl Fn() -> String + Send + Sync + 'static, now: i64) {
        self.recorder.edited_with(read, now);
    }

    pub fn accepted(&mut self, input: AcceptedInput, now: i64) {
        let writes = self.recorder.accepted(input, now);
        self.send(writes);
    }

    pub fn replaced(&mut self, before: impl Into<String>, now: i64) {
        let writes = self.recorder.replaced(before.into(), now);
        self.send(writes);
    }

    pub fn opened(&mut self, text: impl Into<String>, name: impl Into<String>, now: i64) {
        let writes = self.recorder.opened(text.into(), name.into(), now);
        self.send(writes);
    }

    pub fn external(&mut self, text: impl Into<String>, now: i64) {
        let writes = self.recorder.external(text.into(), now);
        self.send(writes);
    }

    pub fn restored(&mut self, text: impl Into<String>, from: VersionId, now: i64) {
        let writes = self.recorder.restored(text.into(), from, now);
        self.send(writes);
    }

    pub fn saving(&mut self, text: impl Into<String>, now: i64) {
        let writes = self.recorder.saving(text.into(), now);
        self.send(writes);
    }

    pub fn saved(&mut self, text: impl Into<String>, name: impl Into<String>, now: i64) {
        let writes = self.recorder.saved(text.into(), name.into(), now);
        self.send(writes);
    }

    pub fn flush(&mut self, now: i64) {
        let writes = self.recorder.flush(now);
        self.send(writes);
    }

    pub fn poll(&mut self, now: i64) {
        let writes = self.recorder.poll(now);
        self.send(writes);
    }

    /// Like [`Self::poll`], but hands the writes that came due back instead of applying them,
    /// for the caller to apply off the UI thread and then report with [`Self::wrote`].
    pub fn poll_deferred(&mut self, now: i64) -> Option<DeferredWrites> {
        self.recorder.poll_lazy(now).map(|edit| DeferredWrites {
            store: Arc::clone(&self.store),
            key: self.key.clone(),
            edit,
        })
    }

    /// Takes the result of [`DeferredWrites::apply`].
    pub fn wrote(&mut self, errors: &[StoreError]) {
        for error in errors {
            (self.on_error)(error);
        }
        self.listing_wanted = true;
    }

    pub fn settled(&mut self) -> bool {
        if !self.listing_wanted {
            return false;
        }
        self.listing_wanted = false;
        match self.store.list(&self.key) {
            Ok(listed) if !same_versions(&self.versions, &listed) => {
                self.versions = listed;
                true
            }
            Ok(_) => false,
            Err(error) => {
                (self.on_error)(&error);
                false
            }
        }
    }

    fn remember(&mut self, id: VersionId, text: String) {
        if self.texts.insert(id, text).is_none() {
            self.text_order.push_back(id);
        }
        if self.texts.len() > self.cache_size
            && let Some(oldest) = self.text_order.pop_front()
        {
            self.texts.remove(&oldest);
        }
    }

    pub fn text(&mut self, id: VersionId) -> Result<Option<String>, StoreError> {
        if let Some(known) = self.texts.get(&id) {
            return Ok(Some(known.clone()));
        }
        let loaded = self.store.text(&self.key, id)?;
        if let Some(text) = &loaded {
            self.remember(id, text.clone());
        }
        Ok(loaded)
    }
}

impl EventSink for DocumentHistory {
    fn edited(&mut self, text: String, now: i64) {
        DocumentHistory::edited(self, text, now);
    }

    fn accepted(&mut self, input: AcceptedInput, now: i64) {
        DocumentHistory::accepted(self, input, now);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::history::memory_store::MemoryHistoryStore;
    use crate::history::store::{MarkSavedInput, RecordInput};
    use crate::history::types::{VersionCause, VersionSummary};

    const KEY: &str = "document";
    const IDLE_MS: i64 = 30_000;

    struct Spy {
        inner: MemoryHistoryStore,
        record_log: Mutex<Vec<String>>,
        lists: AtomicUsize,
        texts: AtomicUsize,
        fail_record_for: Option<&'static str>,
        fail_list: bool,
    }

    impl Spy {
        fn new() -> Self {
            Self {
                inner: MemoryHistoryStore::new(),
                record_log: Mutex::new(Vec::new()),
                lists: AtomicUsize::new(0),
                texts: AtomicUsize::new(0),
                fail_record_for: None,
                fail_list: false,
            }
        }
    }

    impl HistoryStore for Spy {
        fn list(&self, key: &str) -> Result<VersionList, StoreError> {
            self.lists.fetch_add(1, Ordering::SeqCst);
            if self.fail_list {
                return Err(StoreError("gone".into()));
            }
            self.inner.list(key)
        }
        fn text(&self, key: &str, id: VersionId) -> Result<Option<String>, StoreError> {
            self.texts.fetch_add(1, Ordering::SeqCst);
            self.inner.text(key, id)
        }
        fn record(&self, key: &str, input: RecordInput) -> Result<(), StoreError> {
            self.record_log.lock().unwrap().push(input.text.clone());
            if self.fail_record_for == Some(input.text.as_str()) {
                return Err(StoreError("disk full".into()));
            }
            self.inner.record(key, input)
        }
        fn mark_saved(&self, key: &str, input: MarkSavedInput) -> Result<(), StoreError> {
            self.inner.mark_saved(key, input)
        }
    }

    struct Made {
        history: DocumentHistory,
        errors: Arc<Mutex<Vec<String>>>,
        spy: Arc<Spy>,
    }

    fn create_with(spy: Spy, initial: Option<VersionList>, cache_size: usize) -> Made {
        let spy = Arc::new(spy);
        let errors = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&errors);
        let mut options = DocumentHistoryOptions::new(Arc::<Spy>::clone(&spy), KEY, IDLE_MS);
        options.initial = initial;
        options.cache_size = cache_size;
        options.on_error = Box::new(move |error| sink.lock().unwrap().push(error.0.clone()));
        Made {
            history: DocumentHistory::new(options),
            errors,
            spy,
        }
    }

    fn create() -> Made {
        create_with(Spy::new(), Some(Vec::new()), TEXT_CACHE_SIZE)
    }

    fn opened(history: &mut DocumentHistory, text: &str, name: &str) {
        history.opened(text, name, 1000);
    }

    #[test]
    fn starts_from_the_versions_it_was_given() {
        let initial = vec![VersionSummary {
            id: 1,
            at: 1,
            cause: VersionCause::Started,
            words: 2,
            words_added: 2,
            words_removed: 0,
            saved: None,
        }];
        let made = create_with(Spy::new(), Some(initial.clone()), TEXT_CACHE_SIZE);
        assert_eq!(made.history.snapshot(), &initial);
    }

    #[test]
    fn lists_a_recorded_version_once_the_store_has_it() {
        let mut made = create();
        opened(&mut made.history, "one two", "a.md");
        made.history.settled();
        let snapshot = made.history.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].cause, VersionCause::Started);
        assert_eq!(snapshot[0].words, 2);
    }

    #[test]
    fn keeps_the_snapshot_and_stays_quiet_when_the_store_skipped_a_write() {
        let mut made = create();
        opened(&mut made.history, "same", "a.md");
        made.history.settled();
        let before = made.history.snapshot().clone();
        opened(&mut made.history, "same", "a.md");
        assert!(!made.history.settled());
        assert_eq!(made.history.snapshot(), &before);
    }

    #[test]
    fn shows_a_saved_mark_in_the_snapshot() {
        let mut made = create();
        opened(&mut made.history, "text", "a.md");
        made.history.saving("text", 1000);
        made.history.saved("text", "a.md", 1000);
        made.history.settled();
        let marks: Vec<_> = made
            .history
            .snapshot()
            .iter()
            .map(|v| v.saved.clone())
            .collect();
        assert_eq!(marks, vec![Some("a.md".to_string())]);
    }

    #[test]
    fn sends_every_write_to_the_store_before_the_call_returns() {
        let mut made = create();
        opened(&mut made.history, "first", "a.md");
        opened(&mut made.history, "second", "b.md");
        assert_eq!(
            *made.spy.record_log.lock().unwrap(),
            vec!["first", "second"]
        );
    }

    #[test]
    fn sends_a_flush_right_after_an_accept_to_the_store_at_once() {
        let mut made = create();
        made.history.accepted(
            AcceptedInput {
                before: "a".into(),
                text: "b".into(),
                changes: vec![],
            },
            1000,
        );
        made.history.edited("typed after", 1000);
        made.history.flush(1000);
        assert_eq!(
            *made.spy.record_log.lock().unwrap(),
            vec!["a", "b", "typed after"]
        );
    }

    #[test]
    fn lists_the_versions_once_after_a_burst_of_writes() {
        let mut made = create();
        for text in ["a", "b", "c"] {
            opened(&mut made.history, text, &format!("{text}.md"));
        }
        made.history.settled();
        assert_eq!(made.spy.lists.load(Ordering::SeqCst), 1);
        assert_eq!(made.history.snapshot().len(), 3);
    }

    #[test]
    fn lists_the_versions_of_a_history_it_was_not_given_any_for() {
        let spy = Spy::new();
        spy.inner
            .record(
                KEY,
                RecordInput {
                    at: 1,
                    cause: VersionCause::Edited,
                    text: "old".into(),
                },
            )
            .unwrap();
        let mut made = create_with(spy, None, TEXT_CACHE_SIZE);
        assert!(made.history.snapshot().is_empty());
        made.history.settled();
        assert_eq!(made.history.snapshot().len(), 1);
    }

    #[test]
    fn lists_again_for_writes_made_right_after_it_started_listing() {
        let mut made = create_with(Spy::new(), None, TEXT_CACHE_SIZE);
        opened(&mut made.history, "two words", "a.md");
        opened(&mut made.history, "two words more", "a.md");
        made.history.settled();
        assert_eq!(made.history.snapshot().len(), 2);
    }

    #[test]
    fn tells_when_a_write_went_through_and_not_when_it_failed() {
        let mut spy = Spy::new();
        spy.fail_record_for = Some("bad");
        let spy = Arc::new(spy);
        let written = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&written);
        let mut options = DocumentHistoryOptions::new(spy, KEY, IDLE_MS);
        options.initial = Some(Vec::new());
        options.on_written = Some(Box::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }));
        let mut history = DocumentHistory::new(options);
        opened(&mut history, "bad", "a.md");
        history.settled();
        assert_eq!(written.load(Ordering::SeqCst), 0);
        opened(&mut history, "good", "b.md");
        history.settled();
        assert_eq!(written.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn reports_a_failed_write_and_carries_on_with_the_next_one() {
        let mut spy = Spy::new();
        spy.fail_record_for = Some("bad");
        let mut made = create_with(spy, Some(Vec::new()), TEXT_CACHE_SIZE);
        opened(&mut made.history, "bad", "a.md");
        opened(&mut made.history, "good", "b.md");
        made.history.settled();
        assert_eq!(*made.errors.lock().unwrap(), vec!["disk full"]);
        assert_eq!(made.history.snapshot().len(), 1);
    }

    #[test]
    fn reports_a_failed_listing_and_keeps_the_last_snapshot() {
        let mut spy = Spy::new();
        spy.fail_list = true;
        let mut made = create_with(spy, Some(Vec::new()), TEXT_CACHE_SIZE);
        opened(&mut made.history, "a", "a.md");
        made.history.settled();
        assert_eq!(*made.errors.lock().unwrap(), vec!["gone"]);
        assert!(made.history.snapshot().is_empty());
    }

    #[test]
    fn records_pending_edits_when_flushed() {
        let mut made = create();
        opened(&mut made.history, "a", "a.md");
        made.history.edited("a b", 1000);
        made.history.flush(1000);
        made.history.settled();
        let words: Vec<_> = made.history.snapshot().iter().map(|v| v.words).collect();
        assert_eq!(words, vec![1, 2]);
    }

    #[test]
    fn records_idle_edits_when_polled_after_the_delay() {
        let mut made = create();
        made.history.edited("a b", 1000);
        made.history.poll(1000 + IDLE_MS - 1);
        assert!(made.spy.record_log.lock().unwrap().is_empty());
        made.history.poll(1000 + IDLE_MS);
        assert_eq!(*made.spy.record_log.lock().unwrap(), vec!["a b"]);
    }

    #[test]
    fn loads_a_version_text_through_the_store() {
        let mut made = create();
        opened(&mut made.history, "loaded text", "a.md");
        made.history.settled();
        let id = made.history.snapshot()[0].id;
        assert_eq!(
            made.history.text(id).unwrap().as_deref(),
            Some("loaded text")
        );
    }

    #[test]
    fn loads_a_text_once_however_often_it_is_asked_for() {
        let mut made = create();
        opened(&mut made.history, "a", "a.md");
        made.history.settled();
        let id = made.history.snapshot()[0].id;
        made.history.text(id).unwrap();
        made.history.text(id).unwrap();
        made.history.text(id).unwrap();
        assert_eq!(made.spy.texts.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn does_not_remember_a_version_that_has_no_text() {
        let mut made = create();
        assert_eq!(made.history.text(404).unwrap(), None);
        assert_eq!(made.history.text(404).unwrap(), None);
        assert_eq!(made.spy.texts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn forgets_the_oldest_loaded_text_past_its_cache_size() {
        let mut made = create_with(Spy::new(), Some(Vec::new()), 2);
        for label in ["a", "b", "c"] {
            opened(&mut made.history, label, &format!("{label}.md"));
        }
        made.history.settled();
        let ids: Vec<_> = made.history.snapshot().iter().map(|v| v.id).collect();
        for id in &ids {
            made.history.text(*id).unwrap();
        }
        assert_eq!(made.spy.texts.load(Ordering::SeqCst), 3);
        made.history.text(ids[2]).unwrap();
        made.history.text(ids[0]).unwrap();
        assert_eq!(made.spy.texts.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn a_due_edit_is_handed_back_to_be_written_apart_and_is_recorded_once() {
        let mut made = create();
        made.history.edited("typed", 1000);
        assert!(made.history.poll_deferred(1000 + IDLE_MS - 1).is_none());
        let deferred = made.history.poll_deferred(1000 + IDLE_MS).expect("due");
        assert!(made.spy.record_log.lock().unwrap().is_empty());
        assert!(deferred.apply().is_empty());
        assert_eq!(*made.spy.record_log.lock().unwrap(), vec!["typed"]);
        assert!(made.history.poll_deferred(1000 + 2 * IDLE_MS).is_none());
        assert_eq!(made.history.next_due(), None);
    }

    #[test]
    fn reporting_a_deferred_write_lists_the_versions_again_and_surfaces_its_errors() {
        let mut spy = Spy::new();
        spy.fail_record_for = Some("typed");
        let mut made = create_with(spy, Some(Vec::new()), TEXT_CACHE_SIZE);
        made.history.edited("typed", 1000);
        let deferred = made.history.poll_deferred(1000 + IDLE_MS).expect("due");
        let errors = deferred.apply();
        assert_eq!(errors, vec![StoreError("disk full".into())]);
        made.history.wrote(&errors);
        assert_eq!(*made.errors.lock().unwrap(), vec!["disk full"]);
        made.history.settled();
        assert_eq!(made.spy.lists.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_deferred_write_reads_the_text_only_when_it_is_applied() {
        let reads = Arc::new(AtomicUsize::new(0));
        let mut made = create();
        let counter = Arc::clone(&reads);
        made.history.edited_with(
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                "lazy".to_string()
            },
            1000,
        );
        let deferred = made.history.poll_deferred(1000 + IDLE_MS).expect("due");
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        assert_eq!(deferred.key(), KEY);
        deferred.apply();
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert_eq!(*made.spy.record_log.lock().unwrap(), vec!["lazy"]);
    }
}
