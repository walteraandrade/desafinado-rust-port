//! What the Workspace keeps on disk while the writer types: the unsaved text and the place.
//! Both are snapshots taken on the UI thread, which cost a clone of a Rope or a few small
//! values, and written by one serialized writer that keeps only the newest per key.

use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use desafinado_ai::editor::place::{SavedPlace, place_to_json};
use desafinado_core::{coalescing_writer::CoalescingWriter, log_err::LogErr};
use desafinado_platform::history::{HistoryDb, Unsaved};
use gpui_kit::{Task, base::input::Rope};

/// The text of a Document and the saved text it is measured from, as they were at one edit.
#[derive(Clone)]
pub(crate) struct UnsavedSnapshot {
    pub text: Rope,
    pub base: Arc<str>,
}

/// The unsaved text waiting for its pause, and the writer that every write of the Workspace
/// goes through: the unsaved text, the place and the History's idle version.
pub(crate) struct UnsavedQueue {
    pub snapshots: HashMap<String, UnsavedSnapshot>,
    pub task: Option<Task<()>>,
    /// When the oldest edit not yet handed to the writer was made.
    pub first_unwritten: Option<Instant>,
    /// Set by a write of the unsaved text that failed; the Workspace tells the writer once.
    pub failed: Arc<AtomicBool>,
    pub warned: bool,
    pub writer: CoalescingWriter,
}

impl UnsavedQueue {
    pub(crate) fn new(writer: CoalescingWriter) -> Self {
        Self {
            snapshots: HashMap::new(),
            task: None,
            first_unwritten: None,
            failed: Arc::default(),
            warned: false,
            writer,
        }
    }
}

/// How long the writer pauses before the unsaved text is written.
pub(crate) const UNSAVED_DELAY: Duration = Duration::from_millis(400);
/// The longest an edit waits for that pause: continuous typing writes at least this often.
pub(crate) const UNSAVED_MAX_WAIT: Duration = Duration::from_secs(2);

/// Whether the unsaved text must be written now: the writer paused, or has been typing for
/// longer than [`UNSAVED_MAX_WAIT`] since the oldest edit not yet written.
pub(crate) fn unsaved_due(first_unwritten: Instant, last_edit: Instant, now: Instant) -> bool {
    now.saturating_duration_since(last_edit) >= UNSAVED_DELAY
        || now.saturating_duration_since(first_unwritten) >= UNSAVED_MAX_WAIT
}

pub(crate) fn unsaved_job_key(key: &str) -> String {
    format!("unsaved:{key}")
}

pub(crate) fn place_job_key(key: &str) -> String {
    format!("place:{key}")
}

/// Writes the snapshot, or clears the stored text when it is the saved text again. Run it off
/// the UI thread: it reads the whole text.
/// Returns whether it was written.
pub(crate) fn write_unsaved(
    db: &HistoryDb,
    key: &str,
    snapshot: &UnsavedSnapshot,
    at: i64,
) -> bool {
    let result = if snapshot.text == *snapshot.base {
        db.clear_unsaved(key)
    } else {
        db.save_unsaved(
            key,
            &Unsaved {
                text: snapshot.text.to_string(),
                base: snapshot.base.to_string(),
            },
            at,
        )
    };
    match result {
        Ok(()) => true,
        Err(error) => {
            log::error!("Unsaved text write failed: {error}");
            false
        }
    }
}

/// Everything a place keeps but the text, and the text as a Rope.
#[derive(Clone, PartialEq)]
pub(crate) struct PlaceSnapshot {
    pub text: Rope,
    pub rest: SavedPlace,
}

impl PlaceSnapshot {
    /// The place with its text, as JSON. Run it off the UI thread.
    pub(crate) fn json(&self) -> Option<String> {
        let place = SavedPlace {
            text: self.text.to_string(),
            ..self.rest.clone()
        };
        place_to_json(&place).log_err()
    }
}

pub(crate) fn write_place(db: &HistoryDb, key: &str, snapshot: &PlaceSnapshot, at: i64) {
    if let Some(json) = snapshot.json() {
        db.save_place(key, &json, at).log_err();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsaved_text_is_due_after_a_pause_or_after_the_max_wait_of_continuous_typing() {
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        assert!(!unsaved_due(at(0), at(100), at(200)));
        assert!(unsaved_due(at(0), at(100), at(500)));
        assert!(!unsaved_due(at(0), at(1_990), at(1_999)));
        assert!(unsaved_due(at(0), at(1_990), at(2_000)));
        assert!(!unsaved_due(at(5_000), at(5_000), at(5_000)));
    }
    use desafinado_core::coalescing_writer::CoalescingWriter;

    fn db() -> (tempfile::TempDir, Arc<HistoryDb>) {
        let dir = tempfile::tempdir().expect("temp dir");
        let db = HistoryDb::open(&dir.path().join("history.sqlite")).expect("open db");
        (dir, Arc::new(db))
    }

    fn snapshot(text: &str, base: &str) -> UnsavedSnapshot {
        UnsavedSnapshot {
            text: Rope::from_str(text),
            base: Arc::from(base),
        }
    }

    fn submit_unsaved(
        writer: &CoalescingWriter,
        db: &Arc<HistoryDb>,
        text: &str,
    ) -> Option<desafinado_core::coalescing_writer::Drain> {
        let db = Arc::clone(db);
        let snap = snapshot(text, "saved");
        writer.submit(unsaved_job_key("a.md"), move || {
            assert!(write_unsaved(&db, "a.md", &snap, 1));
        })
    }

    #[test]
    fn stores_the_text_with_its_saved_base() {
        let (_dir, db) = db();
        write_unsaved(&db, "a.md", &snapshot("edited", "saved"), 1);
        let stored = db.load_unsaved("a.md").expect("load").expect("stored");
        assert_eq!(
            (stored.text.as_str(), stored.base.as_str()),
            ("edited", "saved")
        );
    }

    #[test]
    fn clears_the_stored_text_when_it_is_the_saved_text_again() {
        let (_dir, db) = db();
        write_unsaved(&db, "a.md", &snapshot("edited", "saved"), 1);
        write_unsaved(&db, "a.md", &snapshot("saved", "saved"), 2);
        assert_eq!(db.load_unsaved("a.md").expect("load"), None);
    }

    #[test]
    fn a_burst_of_edits_stores_only_the_newest_text() {
        let (_dir, db) = db();
        let writer = CoalescingWriter::default();
        let drain = submit_unsaved(&writer, &db, "one");
        submit_unsaved(&writer, &db, "two");
        submit_unsaved(&writer, &db, "three");
        drain.expect("drain").run();
        let stored = db.load_unsaved("a.md").expect("load").expect("stored");
        assert_eq!(stored.text, "three");
    }

    #[test]
    fn quitting_writes_the_newest_text_even_when_the_background_write_never_ran() {
        let (_dir, db) = db();
        let writer = CoalescingWriter::default();
        let _never_run = submit_unsaved(&writer, &db, "typed before the quit");
        submit_unsaved(&writer, &db, "typed last");
        assert_eq!(db.load_unsaved("a.md").expect("load"), None);
        writer.flush();
        let stored = db.load_unsaved("a.md").expect("load").expect("stored");
        assert_eq!(stored.text, "typed last");
    }

    #[test]
    fn a_place_is_stored_with_its_text() {
        let (_dir, db) = db();
        let rest =
            desafinado_ai::editor::place::take_place(&desafinado_ai::editor::place::PlaceInput {
                text: "",
                selection: desafinado_ai::editor::place::PlaceSelection::default(),
                scroll_y: 3.,
                scroll_top: None,
                suggestions: &desafinado_core::suggestions::SuggestionsState::default(),
                notes: None,
                note_rewrites: &[],
                mode: None,
            });
        let snap = PlaceSnapshot {
            text: Rope::from_str("the text"),
            rest,
        };
        write_place(&db, "a.md", &snap, 1);
        let json = db.load_place("a.md").expect("load").expect("stored");
        let place = desafinado_ai::editor::place::place_from_json(&json).expect("json");
        assert_eq!((place.text.as_str(), place.scroll_y), ("the text", 3.));
    }
}
