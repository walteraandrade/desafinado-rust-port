use std::sync::Arc;

use super::recorder::AcceptedInput;
use super::session::DocumentHistory;
use super::store::HistoryKey;
use super::types::{VersionCause, VersionId, VersionList};

type KeyOf<Reference> = dyn Fn(Option<&Reference>) -> HistoryKey + Send + Sync;
type Create = dyn Fn(HistoryKey, Option<VersionList>) -> DocumentHistory + Send + Sync;

pub struct HubOptions<Reference> {
    pub key_of: Box<KeyOf<Reference>>,
    pub create: Box<Create>,
    pub first: Option<Reference>,
    pub first_initial: VersionList,
}

pub struct SwitchableHistory<Reference> {
    key_of: Arc<KeyOf<Reference>>,
    create: Arc<Create>,
    key: HistoryKey,
    current: DocumentHistory,
    retired: Vec<DocumentHistory>,
}

impl<Reference> SwitchableHistory<Reference> {
    pub fn new(options: HubOptions<Reference>) -> Self {
        let key_of: Arc<KeyOf<Reference>> = Arc::from(options.key_of);
        let create: Arc<Create> = Arc::from(options.create);
        let key = key_of(options.first.as_ref());
        let current = create(key.clone(), Some(options.first_initial));
        Self {
            key_of,
            create,
            key,
            current,
            retired: Vec::new(),
        }
    }

    pub fn select(&mut self, reference: Option<&Reference>, now: i64) {
        let next = (self.key_of)(reference);
        if next == self.key {
            return;
        }
        self.current.flush(now);
        self.current.settled();
        let fresh = (self.create)(next.clone(), None);
        self.retired.clear();
        self.key = next;
        self.current = fresh;
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn edited(&mut self, text: impl Into<String>, now: i64) {
        self.current.edited(text, now);
    }

    pub fn edited_with(&mut self, read: impl Fn() -> String + Send + Sync + 'static, now: i64) {
        self.current.edited_with(read, now);
    }

    pub fn accepted(&mut self, input: AcceptedInput, now: i64) {
        self.current.accepted(input, now);
    }

    pub fn replaced(&mut self, before: impl Into<String>, now: i64) {
        self.current.replaced(before, now);
    }

    pub fn opened(&mut self, text: impl Into<String>, name: impl Into<String>, now: i64) {
        self.current.opened(text, name, now);
    }

    pub fn external(&mut self, text: impl Into<String>, now: i64) {
        self.current.external(text, now);
    }

    pub fn restored(&mut self, text: impl Into<String>, from: VersionId, now: i64) {
        self.current.restored(text, from, now);
    }

    pub fn saving(&mut self, text: impl Into<String>, now: i64) {
        self.current.saving(text, now);
    }

    pub fn saved(&mut self, text: impl Into<String>, name: impl Into<String>, now: i64) {
        self.current.saved(text, name, now);
    }

    pub fn recorded_deferred(
        &mut self,
        text: impl Into<String>,
        cause: VersionCause,
        now: i64,
    ) -> super::session::DeferredWrites {
        self.current.recorded_deferred(text, cause, now)
    }

    pub fn saving_deferred(
        &mut self,
        text: impl Into<String>,
        now: i64,
    ) -> super::session::DeferredWrites {
        self.current.saving_deferred(text, now)
    }

    pub fn saved_deferred(
        &mut self,
        text: impl Into<String>,
        name: impl Into<String>,
        now: i64,
    ) -> super::session::DeferredWrites {
        self.current.saved_deferred(text, name, now)
    }

    pub fn flush(&mut self, now: i64) {
        self.current.flush(now);
    }

    pub fn poll(&mut self, now: i64) {
        self.current.poll(now);
    }

    pub fn poll_deferred(&mut self, now: i64) -> Option<super::session::DeferredWrites> {
        self.current.poll_deferred(now)
    }

    /// Takes the result of a deferred write for `key`; false when the Document has changed
    /// since, which the result no longer concerns.
    pub fn wrote(&mut self, key: &str, applied: super::session::Applied) -> bool {
        let current = key == self.key;
        if current {
            self.current.wrote(applied);
        }
        current
    }

    /// Lists the current Document's versions again at the next settle.
    pub fn relist(&mut self) {
        self.current.relist();
    }

    pub fn next_due(&self) -> Option<i64> {
        self.current.next_due()
    }

    pub fn snapshot(&self) -> &VersionList {
        self.current.snapshot()
    }

    pub fn text(&mut self, id: VersionId) -> Result<Option<String>, super::store::StoreError> {
        self.current.text(id)
    }

    pub fn settled(&mut self) -> bool {
        self.current.settled()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::history::memory_store::MemoryHistoryStore;
    use crate::history::session::DocumentHistoryOptions;
    use crate::history::store::{HistoryStore, MarkSavedInput, RecordInput, StoreError};
    use crate::history::types::{VersionCause, VersionSummary};

    const IDLE_MS: i64 = 30_000;

    struct Logged {
        inner: MemoryHistoryStore,
        calls: Mutex<Vec<String>>,
    }

    impl HistoryStore for Logged {
        fn list(&self, key: &str) -> Result<VersionList, StoreError> {
            self.inner.list(key)
        }
        fn text(&self, key: &str, id: VersionId) -> Result<Option<String>, StoreError> {
            self.inner.text(key, id)
        }
        fn record(&self, key: &str, input: RecordInput) -> Result<(), StoreError> {
            let kind = format!("{:?}", input.cause.kind()).to_lowercase();
            self.calls
                .lock()
                .unwrap()
                .push(format!("{key}:{kind}:{}", input.text));
            self.inner.record(key, input)
        }
        fn mark_saved(&self, key: &str, input: MarkSavedInput) -> Result<(), StoreError> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("{key}:saved:{}", input.text));
            self.inner.mark_saved(key, input)
        }
    }

    fn create_hub(first_initial: VersionList) -> (SwitchableHistory<String>, Arc<Logged>) {
        let store = Arc::new(Logged {
            inner: MemoryHistoryStore::new(),
            calls: Mutex::new(Vec::new()),
        });
        let shared = Arc::clone(&store);
        let hub = SwitchableHistory::new(HubOptions {
            key_of: Box::new(|reference: Option<&String>| {
                reference.cloned().unwrap_or_else(|| "scratch".to_string())
            }),
            create: Box::new(move |key, initial| {
                let mut options =
                    DocumentHistoryOptions::new(Arc::<Logged>::clone(&shared), key, IDLE_MS);
                options.initial = initial;
                DocumentHistory::new(options)
            }),
            first: None,
            first_initial,
        });
        (hub, store)
    }

    fn calls(store: &Logged) -> Vec<String> {
        store.calls.lock().unwrap().clone()
    }

    fn words(hub: &SwitchableHistory<String>) -> Vec<usize> {
        hub.snapshot().iter().map(|v| v.words).collect()
    }

    fn a() -> String {
        "/notes/a.md".to_string()
    }

    fn b() -> String {
        "/notes/b.md".to_string()
    }

    #[test]
    fn writes_to_the_key_of_the_file_it_started_with() {
        let (mut hub, store) = create_hub(vec![]);
        hub.opened("first", "a.md", 1000);
        hub.settled();
        assert_eq!(calls(&store), vec!["scratch:opened:first"]);
    }

    #[test]
    fn writes_an_external_version_under_the_key_of_the_open_file() {
        let (mut hub, store) = create_hub(vec![]);
        hub.select(Some(&a()), 1000);
        hub.external("by an agent", 1000);
        hub.settled();
        assert_eq!(calls(&store), vec!["/notes/a.md:external:by an agent"]);
    }

    #[test]
    fn moves_writes_to_the_key_of_the_file_it_switches_to() {
        let (mut hub, store) = create_hub(vec![]);
        hub.select(Some(&a()), 1000);
        hub.opened("in a", "a.md", 1000);
        hub.settled();
        assert_eq!(calls(&store), vec!["/notes/a.md:opened:in a"]);
    }

    #[test]
    fn records_pending_edits_under_the_old_key_before_the_new_key_gets_anything() {
        let (mut hub, store) = create_hub(vec![]);
        hub.edited("typed in scratch", 1000);
        hub.select(Some(&a()), 1000);
        hub.opened("a text", "a.md", 1000);
        hub.settled();
        assert_eq!(
            calls(&store),
            vec![
                "scratch:edited:typed in scratch",
                "/notes/a.md:opened:a text"
            ]
        );
    }

    #[test]
    fn does_not_record_pending_edits_of_the_old_file_a_second_time_later() {
        let (mut hub, store) = create_hub(vec![]);
        hub.edited("pending", 1000);
        hub.select(Some(&a()), 1000);
        hub.poll(1000 + IDLE_MS * 2);
        hub.settled();
        assert_eq!(calls(&store), vec!["scratch:edited:pending"]);
    }

    #[test]
    fn shows_the_timeline_of_the_file_it_is_on() {
        let (mut hub, _) = create_hub(vec![]);
        hub.opened("one", "a.md", 1000);
        hub.settled();
        hub.select(Some(&a()), 1000);
        assert!(hub.snapshot().is_empty());
        hub.opened("two words", "a.md", 1000);
        hub.opened("two words more", "a.md", 1000);
        hub.settled();
        assert_eq!(words(&hub), vec![2, 3]);
        hub.select(None, 1000);
        hub.settled();
        assert_eq!(words(&hub), vec![1]);
    }

    #[test]
    fn loads_the_versions_of_a_file_it_comes_back_to() {
        let (mut hub, _) = create_hub(vec![]);
        hub.select(Some(&a()), 1000);
        hub.opened("alpha", "a.md", 1000);
        hub.settled();
        hub.select(Some(&b()), 1000);
        hub.settled();
        assert!(hub.snapshot().is_empty());
        hub.select(Some(&a()), 1000);
        hub.settled();
        assert_eq!(hub.snapshot().len(), 1);
    }

    #[test]
    fn starts_from_the_versions_it_was_given_for_the_first_file() {
        let initial = vec![VersionSummary {
            id: 1,
            at: 1,
            cause: VersionCause::Started,
            words: 1,
            words_added: 1,
            words_removed: 0,
            saved: None,
        }];
        let (hub, _) = create_hub(initial.clone());
        assert_eq!(hub.snapshot(), &initial);
    }

    #[test]
    fn does_nothing_when_it_selects_the_file_it_is_on() {
        let (mut hub, store) = create_hub(vec![]);
        hub.edited("still pending", 1000);
        hub.select(None, 1000);
        assert!(calls(&store).is_empty());
        hub.flush(1000);
        hub.settled();
        assert_eq!(calls(&store), vec!["scratch:edited:still pending"]);
    }

    #[test]
    fn maps_a_file_without_a_ref_to_the_same_key_as_the_one_it_started_with() {
        let (mut hub, store) = create_hub(vec![]);
        hub.select(Some(&a()), 1000);
        hub.select(None, 1000);
        hub.opened("back", "x.md", 1000);
        assert_eq!(calls(&store), vec!["scratch:opened:back"]);
    }

    #[test]
    fn loads_the_text_of_a_version_of_the_file_it_is_on() {
        let (mut hub, _) = create_hub(vec![]);
        hub.select(Some(&a()), 1000);
        hub.opened("the text", "a.md", 1000);
        hub.settled();
        let id = hub.snapshot()[0].id;
        assert_eq!(hub.text(id).unwrap().as_deref(), Some("the text"));
    }

    #[test]
    fn flushes_the_pending_edits_of_the_file_it_is_on() {
        let (mut hub, store) = create_hub(vec![]);
        hub.select(Some(&a()), 1000);
        hub.edited("unsaved typing", 1000);
        hub.flush(1000);
        hub.settled();
        assert_eq!(calls(&store), vec!["/notes/a.md:edited:unsaved typing"]);
    }

    #[test]
    fn a_deferred_result_for_a_document_left_behind_is_ignored() {
        let (mut hub, _store) = create_hub(vec![]);
        hub.select(Some(&a()), 1000);
        let key = hub.key().to_string();
        let first = hub.saving_deferred("text", 1000).apply();
        assert!(hub.wrote(&key, first));
        let second = hub.saving_deferred("text", 1500).apply();
        hub.select(None, 2000);
        assert!(!hub.wrote(&key, second));
    }
}
