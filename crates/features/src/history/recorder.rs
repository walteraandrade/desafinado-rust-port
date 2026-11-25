use std::sync::Arc;

use super::store::{HistoryWrite, MarkSavedInput, RecordInput};
use super::types::{AcceptedChange, VersionCause, VersionId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptedInput {
    pub before: String,
    pub text: String,
    pub changes: Vec<AcceptedChange>,
}

/// The text of a pending edit: ready, or read only when the version is written.
#[derive(Clone)]
enum PendingText {
    Ready(String),
    Lazy(Arc<dyn Fn() -> String + Send + Sync>),
}

impl PendingText {
    fn read(self) -> String {
        match self {
            Self::Ready(text) => text,
            Self::Lazy(read) => read(),
        }
    }
}

impl std::fmt::Debug for PendingText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ready(text) => f.debug_tuple("Ready").field(text).finish(),
            Self::Lazy(_) => f.write_str("Lazy"),
        }
    }
}

#[derive(Clone, Debug)]
struct Pending {
    text: PendingText,
    due: i64,
}

#[derive(Clone, Debug)]
pub struct HistoryRecorder {
    idle_ms: i64,
    pending: Option<Pending>,
}

fn record(text: String, cause: VersionCause, now: i64) -> HistoryWrite {
    HistoryWrite::VersionRecorded(RecordInput {
        at: now,
        cause,
        text,
    })
}

impl HistoryRecorder {
    pub fn new(idle_ms: i64) -> Self {
        Self {
            idle_ms,
            pending: None,
        }
    }

    pub fn next_due(&self) -> Option<i64> {
        self.pending.as_ref().map(|pending| pending.due)
    }

    pub fn edited(&mut self, text: String, now: i64) {
        self.pending = Some(Pending {
            text: PendingText::Ready(text),
            due: now + self.idle_ms,
        });
    }

    /// Like [`Self::edited`], but the text is read only when the version is recorded, so
    /// typing does not copy the Document for a version that a later edit replaces.
    pub fn edited_with(&mut self, read: impl Fn() -> String + Send + Sync + 'static, now: i64) {
        self.pending = Some(Pending {
            text: PendingText::Lazy(Arc::new(read)),
            due: now + self.idle_ms,
        });
    }

    pub fn poll(&mut self, now: i64) -> Vec<HistoryWrite> {
        match self.next_due() {
            Some(due) if due <= now => self.flush(now),
            _ => Vec::new(),
        }
    }

    pub fn flush(&mut self, now: i64) -> Vec<HistoryWrite> {
        self.pending
            .take()
            .map(|pending| vec![record(pending.text.read(), VersionCause::Edited, now)])
            .unwrap_or_default()
    }

    fn settle(&mut self, before: String, now: i64) -> HistoryWrite {
        self.pending = None;
        record(before, VersionCause::Edited, now)
    }

    pub fn accepted(&mut self, input: AcceptedInput, now: i64) -> Vec<HistoryWrite> {
        vec![
            self.settle(input.before, now),
            record(
                input.text,
                VersionCause::Accepted {
                    changes: input.changes,
                },
                now,
            ),
        ]
    }

    pub fn replaced(&mut self, before: String, now: i64) -> Vec<HistoryWrite> {
        vec![self.settle(before, now)]
    }

    pub fn opened(&mut self, text: String, name: String, now: i64) -> Vec<HistoryWrite> {
        vec![record(text, VersionCause::Opened { name }, now)]
    }

    pub fn external(&mut self, text: String, now: i64) -> Vec<HistoryWrite> {
        vec![record(text, VersionCause::External, now)]
    }

    pub fn restored(&mut self, text: String, from: VersionId, now: i64) -> Vec<HistoryWrite> {
        vec![record(text, VersionCause::Restored { from }, now)]
    }

    pub fn saving(&mut self, text: String, now: i64) -> Vec<HistoryWrite> {
        vec![self.settle(text, now)]
    }

    pub fn saved(&mut self, text: String, name: String, now: i64) -> Vec<HistoryWrite> {
        vec![HistoryWrite::VersionMarkedSaved(MarkSavedInput {
            text,
            name,
            at: now,
        })]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::types::SuggestionOrigin;

    const IDLE_MS: i64 = 30_000;

    fn shorter() -> AcceptedChange {
        AcceptedChange {
            origin: SuggestionOrigin::Ask,
            instruction: "Shorter".into(),
            rationale: "Drops filler.".into(),
        }
    }

    struct Harness {
        recorder: HistoryRecorder,
        writes: Vec<HistoryWrite>,
        time: i64,
    }

    impl Harness {
        fn new() -> Self {
            Self {
                recorder: HistoryRecorder::new(IDLE_MS),
                writes: Vec::new(),
                time: 1000,
            }
        }

        fn emit(&mut self, writes: Vec<HistoryWrite>) {
            self.writes.extend(writes);
        }

        fn edited(&mut self, text: &str) {
            self.recorder.edited(text.into(), self.time);
        }

        fn accepted(&mut self, before: &str, text: &str) {
            let writes = self.recorder.accepted(
                AcceptedInput {
                    before: before.into(),
                    text: text.into(),
                    changes: vec![shorter()],
                },
                self.time,
            );
            self.emit(writes);
        }

        fn replaced(&mut self, before: &str) {
            let writes = self.recorder.replaced(before.into(), self.time);
            self.emit(writes);
        }

        fn opened(&mut self, text: &str, name: &str) {
            let writes = self.recorder.opened(text.into(), name.into(), self.time);
            self.emit(writes);
        }

        fn external(&mut self, text: &str) {
            let writes = self.recorder.external(text.into(), self.time);
            self.emit(writes);
        }

        fn saving(&mut self, text: &str) {
            let writes = self.recorder.saving(text.into(), self.time);
            self.emit(writes);
        }

        fn saved(&mut self, text: &str, name: &str) {
            let writes = self.recorder.saved(text.into(), name.into(), self.time);
            self.emit(writes);
        }

        fn flush(&mut self) {
            let writes = self.recorder.flush(self.time);
            self.emit(writes);
        }

        fn tick(&mut self, ms: i64) {
            self.time += ms;
            let writes = self.recorder.poll(self.time);
            self.emit(writes);
        }
    }

    fn recorded(text: &str, cause: VersionCause, at: i64) -> HistoryWrite {
        record(text.into(), cause, at)
    }

    fn summary(writes: &[HistoryWrite]) -> Vec<Vec<String>> {
        writes
            .iter()
            .map(|write| match write {
                HistoryWrite::VersionRecorded(input) => {
                    let kind = match input.cause {
                        VersionCause::Edited => "edited",
                        VersionCause::Accepted { .. } => "accepted",
                        _ => "other",
                    };
                    vec![kind.to_string(), input.text.clone()]
                }
                HistoryWrite::VersionMarkedSaved(input) => {
                    vec![
                        "marked-saved".into(),
                        input.text.clone(),
                        input.name.clone(),
                    ]
                }
            })
            .collect()
    }

    fn rows(rows: &[&[&str]]) -> Vec<Vec<String>> {
        rows.iter()
            .map(|row| row.iter().map(std::string::ToString::to_string).collect())
            .collect()
    }

    #[test]
    fn turns_edits_into_one_edited_version_after_the_idle_delay() {
        let mut h = Harness::new();
        h.edited("start a");
        h.tick(IDLE_MS - 1);
        h.edited("start ab");
        h.tick(IDLE_MS - 1);
        assert_eq!(h.writes, vec![]);
        h.tick(1);
        assert_eq!(
            h.writes,
            vec![recorded(
                "start ab",
                VersionCause::Edited,
                1000 + 2 * IDLE_MS - 1
            )]
        );
    }

    #[test]
    fn records_pending_edits_on_their_own_before_an_accepted_version() {
        let mut h = Harness::new();
        h.edited("start typed");
        h.accepted("start typed", "typed");
        assert_eq!(
            h.writes,
            vec![
                recorded("start typed", VersionCause::Edited, 1000),
                recorded(
                    "typed",
                    VersionCause::Accepted {
                        changes: vec![shorter()]
                    },
                    1000
                ),
            ]
        );
    }

    #[test]
    fn records_the_text_before_an_accept_even_without_a_reported_edit() {
        let mut h = Harness::new();
        h.accepted("unseen", "after");
        assert_eq!(
            summary(&h.writes),
            rows(&[&["edited", "unseen"], &["accepted", "after"]])
        );
    }

    #[test]
    fn cancels_the_idle_timer_once_pending_edits_are_recorded() {
        let mut h = Harness::new();
        h.edited("start typed");
        h.accepted("start typed", "typed");
        h.tick(IDLE_MS);
        assert_eq!(h.writes.len(), 2);
    }

    #[test]
    fn records_the_text_before_a_replace_then_the_opened_version() {
        let mut h = Harness::new();
        h.edited("start edited");
        h.replaced("start edited");
        h.opened("# File", "file.md");
        assert_eq!(
            h.writes,
            vec![
                recorded("start edited", VersionCause::Edited, 1000),
                recorded(
                    "# File",
                    VersionCause::Opened {
                        name: "file.md".into()
                    },
                    1000
                ),
            ]
        );
    }

    #[test]
    fn records_the_text_before_a_replace_then_the_external_version() {
        let mut h = Harness::new();
        h.edited("mine");
        h.replaced("mine");
        h.external("by an agent");
        assert_eq!(
            h.writes,
            vec![
                recorded("mine", VersionCause::Edited, 1000),
                recorded("by an agent", VersionCause::External, 1000),
            ]
        );
    }

    #[test]
    fn records_an_external_version_and_no_saved_mark() {
        let mut h = Harness::new();
        h.external("by an agent");
        assert_eq!(
            h.writes,
            vec![recorded("by an agent", VersionCause::External, 1000)]
        );
    }

    #[test]
    fn records_a_restored_version_pointing_at_its_source() {
        let mut h = Harness::new();
        h.replaced("changed");
        let writes = h.recorder.restored("start".into(), 1, h.time);
        h.emit(writes);
        assert_eq!(
            h.writes.last(),
            Some(&recorded("start", VersionCause::Restored { from: 1 }, 1000))
        );
    }

    #[test]
    fn records_the_text_before_a_replace_even_with_no_edit_waiting() {
        let mut h = Harness::new();
        h.replaced("changed elsewhere");
        assert_eq!(
            h.writes,
            vec![recorded("changed elsewhere", VersionCause::Edited, 1000)]
        );
    }

    #[test]
    fn records_pending_edits_when_save_starts_then_marks_the_saved_text() {
        let mut h = Harness::new();
        h.edited("start more");
        h.saving("start more");
        h.saved("start more", "notes.md");
        assert_eq!(
            h.writes,
            vec![
                recorded("start more", VersionCause::Edited, 1000),
                HistoryWrite::VersionMarkedSaved(MarkSavedInput {
                    text: "start more".into(),
                    name: "notes.md".into(),
                    at: 1000
                }),
            ]
        );
    }

    #[test]
    fn records_a_version_when_save_starts_even_with_no_edit_waiting() {
        let mut h = Harness::new();
        h.saving("start s");
        assert_eq!(
            h.writes,
            vec![recorded("start s", VersionCause::Edited, 1000)]
        );
    }

    #[test]
    fn drops_the_pending_edits_and_their_timer_once_save_starts() {
        let mut h = Harness::new();
        h.edited("start s");
        h.saving("start s");
        h.tick(IDLE_MS);
        assert_eq!(h.writes.len(), 1);
    }

    #[test]
    fn keeps_edits_typed_while_a_save_waits_after_the_saved_version() {
        let mut h = Harness::new();
        h.edited("start a");
        h.saving("start a");
        h.edited("start a b");
        h.saved("start a", "notes.md");
        assert_eq!(
            summary(&h.writes),
            rows(&[
                &["edited", "start a"],
                &["marked-saved", "start a", "notes.md"]
            ])
        );
        h.tick(IDLE_MS);
        assert_eq!(
            summary(&h.writes),
            rows(&[
                &["edited", "start a"],
                &["marked-saved", "start a", "notes.md"],
                &["edited", "start a b"],
            ])
        );
    }

    #[test]
    fn marks_the_saved_text_after_an_accept_that_came_during_the_save() {
        let mut h = Harness::new();
        h.edited("start typed");
        h.saving("start typed");
        h.accepted("start typed", "typed");
        h.saved("start typed", "notes.md");
        assert_eq!(
            summary(&h.writes),
            rows(&[
                &["edited", "start typed"],
                &["edited", "start typed"],
                &["accepted", "typed"],
                &["marked-saved", "start typed", "notes.md"],
            ])
        );
    }

    #[test]
    fn flush_records_pending_edits_right_away_once() {
        let mut h = Harness::new();
        h.edited("start x");
        h.flush();
        h.flush();
        h.tick(IDLE_MS);
        assert_eq!(
            h.writes,
            vec![recorded("start x", VersionCause::Edited, 1000)]
        );
    }

    #[test]
    fn writes_nothing_when_there_is_nothing_pending() {
        let mut h = Harness::new();
        h.flush();
        h.tick(IDLE_MS);
        assert_eq!(h.writes, vec![]);
    }

    #[test]
    fn stamps_writes_with_the_injected_clock() {
        let mut h = Harness::new();
        h.tick(500);
        h.opened("file", "file.md");
        h.saved("file", "file.md");
        let times: Vec<i64> = h
            .writes
            .iter()
            .map(|write| match write {
                HistoryWrite::VersionRecorded(input) => input.at,
                HistoryWrite::VersionMarkedSaved(input) => input.at,
            })
            .collect();
        assert_eq!(times, vec![1500, 1500]);
    }

    #[test]
    fn a_lazy_edit_is_read_only_when_it_is_recorded_and_records_the_same_version() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let reads = Arc::new(AtomicUsize::new(0));
        let mut recorder = HistoryRecorder::new(IDLE_MS);
        for text in ["a", "ab", "abc"] {
            let reads = Arc::clone(&reads);
            recorder.edited_with(
                move || {
                    reads.fetch_add(1, Ordering::SeqCst);
                    text.to_string()
                },
                1000,
            );
        }
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        assert_eq!(recorder.next_due(), Some(1000 + IDLE_MS));
        let writes = recorder.flush(2000);
        assert_eq!(writes, vec![recorded("abc", VersionCause::Edited, 2000)]);
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_lazy_edit_replaced_by_an_accepted_version_is_never_read() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let read = Arc::new(AtomicBool::new(false));
        let mut recorder = HistoryRecorder::new(IDLE_MS);
        let flag = Arc::clone(&read);
        recorder.edited_with(
            move || {
                flag.store(true, Ordering::SeqCst);
                "typed".to_string()
            },
            1000,
        );
        recorder.saving("saved text".into(), 1500);
        assert!(!read.load(Ordering::SeqCst));
        assert_eq!(recorder.next_due(), None);
    }
}
