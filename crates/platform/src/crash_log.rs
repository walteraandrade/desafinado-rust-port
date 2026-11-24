//! The crash log: one entry per panic, in `crash.log` in the user data folder. Started from
//! its desktop entry, the app's stderr reaches no journal, so without this file a crash
//! leaves no trace.

use std::{any::Any, fs::OpenOptions, io::Write, path::Path};

pub const FILE_NAME: &str = "crash.log";

/// Keeps the file from growing without end: past this size it starts over.
const MAX_BYTES: u64 = 1024 * 1024;

pub struct Crash<'a> {
    pub at: &'a str,
    pub version: &'a str,
    pub thread: &'a str,
    pub location: &'a str,
    pub message: &'a str,
    pub backtrace: &'a str,
}

impl Crash<'_> {
    pub fn entry(&self) -> String {
        let Crash {
            at,
            version,
            thread,
            location,
            message,
            backtrace,
        } = self;
        format!(
            "=== {at} desafinado {version}\nthread '{thread}' panicked at {location}:\n{message}\n{backtrace}\n"
        )
    }
}

/// The text a panic was raised with, when it was raised with text.
pub fn panic_message(payload: &dyn Any) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("(no message)")
}

/// Adds an entry to the log, emptying it first once it is over [`MAX_BYTES`].
pub fn append(path: &Path, entry: &str) -> std::io::Result<()> {
    let too_big = std::fs::metadata(path).is_ok_and(|meta| meta.len() > MAX_BYTES);
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(!too_big)
        .truncate(too_big)
        .open(path)?;
    file.write_all(entry.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled(path: &Path, bytes: u64) {
        std::fs::write(path, vec![b'x'; usize::try_from(bytes).unwrap()]).unwrap();
    }

    #[test]
    fn an_entry_puts_each_part_in_its_place() {
        let crash = Crash {
            at: "2026-11-24T18:26:37-03:00",
            version: "0.1.0",
            thread: "main",
            location: "crates/app/src/editor_pane.rs:10:5",
            message: "byte index 5 is not a char boundary",
            backtrace: "0: main",
        };
        assert_eq!(
            crash.entry(),
            "=== 2026-11-24T18:26:37-03:00 desafinado 0.1.0\n\
             thread 'main' panicked at crates/app/src/editor_pane.rs:10:5:\n\
             byte index 5 is not a char boundary\n\
             0: main\n"
        );
    }

    #[test]
    fn reads_the_message_of_text_payloads_only() {
        assert_eq!(panic_message(&"static text"), "static text");
        assert_eq!(panic_message(&String::from("formatted")), "formatted");
        assert_eq!(panic_message(&42_u32), "(no message)");
    }

    #[test]
    fn appends_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        append(&path, "one\n").unwrap();
        append(&path, "two\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");
    }

    #[test]
    fn starts_over_only_past_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        filled(&path, MAX_BYTES);
        append(&path, "kept\n").unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().len(), MAX_BYTES + 5);
        append(&path, "fresh\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "fresh\n");
    }

    #[test]
    fn fails_when_the_folder_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(append(&dir.path().join("gone").join(FILE_NAME), "x").is_err());
    }
}
