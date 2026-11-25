use desafinado_core::log_err::{LogErr, MissingIsRemoved};
use std::path::{Path, PathBuf};

use desafinado_platform::files::write_atomic_unconditionally;

pub const LAST_FILE_NAME: &str = "last-file.txt";

/// The writer key of every save and clear of the record, so the newest replaces the one waiting.
pub const LAST_FILE_JOB: &str = "last-file";

fn file_in(dir: &Path) -> PathBuf {
    dir.join(LAST_FILE_NAME)
}

pub fn load_last_file(dir: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(file_in(dir)).ok()?;
    let path = PathBuf::from(text.trim_end_matches('\n'));
    path.is_absolute().then_some(path)
}

pub fn save_last_file(dir: &Path, path: &Path) {
    write_atomic_unconditionally(&file_in(dir), &path.to_string_lossy()).log_err();
}

pub fn clear_last_file(dir: &Path) {
    std::fs::remove_file(file_in(dir))
        .missing_is_removed()
        .log_err();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_forgets_and_ignores_a_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_last_file(dir.path()), None);
        save_last_file(dir.path(), Path::new("/notes/a b.md"));
        assert_eq!(
            load_last_file(dir.path()),
            Some(PathBuf::from("/notes/a b.md"))
        );
        clear_last_file(dir.path());
        assert_eq!(load_last_file(dir.path()), None);
        std::fs::write(dir.path().join(LAST_FILE_NAME), "relative.md").unwrap();
        assert_eq!(load_last_file(dir.path()), None);
    }

    #[test]
    fn a_clear_queued_after_a_save_wins_and_a_save_queued_after_a_clear_wins() {
        use desafinado_core::coalescing_writer::CoalescingWriter;
        let dir = tempfile::tempdir().unwrap();
        let writer = CoalescingWriter::default();
        let submit = |job: Box<dyn FnOnce() + Send>| {
            let _never_run = writer.submit(LAST_FILE_JOB, job);
        };
        let (save_dir, clear_dir) = (dir.path().to_path_buf(), dir.path().to_path_buf());
        submit(Box::new(move || {
            save_last_file(&save_dir, Path::new("/notes/a.md"));
        }));
        submit(Box::new(move || clear_last_file(&clear_dir)));
        writer.flush();
        assert_eq!(load_last_file(dir.path()), None);
        let save_dir = dir.path().to_path_buf();
        submit(Box::new(move || {
            save_last_file(&save_dir, Path::new("/notes/b.md"));
        }));
        writer.flush();
        assert_eq!(
            load_last_file(dir.path()),
            Some(PathBuf::from("/notes/b.md"))
        );
    }
}
