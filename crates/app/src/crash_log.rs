//! Writes each panic to `crash.log` in the user data folder. Started from its desktop entry,
//! the app's stderr reaches no journal, so without this file a crash leaves no trace.

use std::{
    backtrace::Backtrace,
    fs::OpenOptions,
    io::Write,
    panic::PanicHookInfo,
    path::{Path, PathBuf},
};

pub const FILE_NAME: &str = "crash.log";

/// Keeps the file from growing without end: past this size it starts over.
const MAX_BYTES: u64 = 1024 * 1024;

fn payload_text<'a>(info: &'a PanicHookInfo<'_>) -> &'a str {
    let payload = info.payload();
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("(no message)")
}

/// One entry of the crash log.
pub fn report(
    at: &str,
    version: &str,
    thread: &str,
    message: &str,
    location: &str,
    backtrace: &str,
) -> String {
    format!(
        "=== {at} desafinado {version}\nthread '{thread}' panicked at {location}:\n{message}\n{backtrace}\n"
    )
}

fn append(path: &Path, entry: &str) -> std::io::Result<()> {
    let too_big = std::fs::metadata(path).is_ok_and(|meta| meta.len() > MAX_BYTES);
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(!too_big)
        .truncate(too_big)
        .open(path)?;
    file.write_all(entry.as_bytes())
}

/// Adds the crash log to the panic hook, keeping the default message on stderr.
pub fn install(dir: &Path) {
    let path: PathBuf = dir.join(FILE_NAME);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let entry = report(
            &chrono::Local::now().to_rfc3339(),
            env!("CARGO_PKG_VERSION"),
            thread.name().unwrap_or("unnamed"),
            payload_text(info),
            &info
                .location()
                .map_or_else(|| "an unknown place".to_string(), ToString::to_string),
            &Backtrace::force_capture().to_string(),
        );
        if let Err(error) = append(&path, &entry) {
            log::error!("Could not write {}: {error}", path.display());
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_names_the_time_the_place_and_the_message() {
        let entry = report(
            "2026-11-24T18:26:37-03:00",
            "0.1.0",
            "main",
            "byte index 5 is not a char boundary",
            "crates/app/src/editor_pane.rs:10:5",
            "0: main",
        );
        assert!(entry.starts_with("=== 2026-11-24T18:26:37-03:00 desafinado 0.1.0\n"));
        assert!(entry.contains("thread 'main' panicked at crates/app/src/editor_pane.rs:10:5:"));
        assert!(entry.contains("byte index 5 is not a char boundary\n0: main"));
    }

    #[test]
    fn appends_entries_and_starts_over_past_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        append(&path, "one\n").unwrap();
        append(&path, "two\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");
        std::fs::write(&path, vec![b'x'; usize::try_from(MAX_BYTES).unwrap() + 1]).unwrap();
        append(&path, "three\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "three\n");
    }
}
