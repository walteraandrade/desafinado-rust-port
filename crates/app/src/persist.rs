//! What the Workspace keeps on disk while the writer types: the unsaved text and the place.
//! Both are snapshots taken on the UI thread, which cost a clone of a Rope or a few small
//! values, and written by one serialized writer that keeps only the newest per key.

use std::{collections::HashMap, sync::Arc};

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
    pub writer: CoalescingWriter,
}

impl UnsavedQueue {
    pub(crate) fn new(writer: CoalescingWriter) -> Self {
        Self {
            snapshots: HashMap::new(),
            task: None,
            writer,
        }
    }
}

pub(crate) fn unsaved_job_key(key: &str) -> String {
    format!("unsaved:{key}")
}

pub(crate) fn place_job_key(key: &str) -> String {
    format!("place:{key}")
}

/// Writes the snapshot, or clears the stored text when it is the saved text again. Run it off
/// the UI thread: it reads the whole text.
pub(crate) fn write_unsaved(db: &HistoryDb, key: &str, snapshot: &UnsavedSnapshot, at: i64) {
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
    if let Err(error) = result {
        log::error!("Unsaved text write failed: {error}");
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
            write_unsaved(&db, "a.md", &snap, 1);
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
