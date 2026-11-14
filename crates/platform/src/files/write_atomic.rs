use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const NEW_FILE_MODE: u32 = 0o644;
const MODE_MASK: u32 = 0o777;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtomicOutcome {
    Written,
    Rejected,
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temporary_for(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let unique = format!(
        "{:x}{:x}{:x}",
        std::process::id(),
        nanos,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    target
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(".{name}.{unique}.tmp"))
}

fn mode_of(target: &Path) -> io::Result<u32> {
    match fs::metadata(target) {
        Ok(info) => Ok(info.permissions().mode() & MODE_MASK),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(NEW_FILE_MODE),
        Err(error) => Err(error),
    }
}

fn write_temporary(temporary: &Path, text: &str, mode: u32) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(temporary)?;
    file.write_all(text.as_bytes())?;
    file.set_permissions(fs::Permissions::from_mode(mode))?;
    file.sync_all()
}

pub fn write_atomic(
    target: &Path,
    text: &str,
    accept: &dyn Fn() -> io::Result<bool>,
) -> io::Result<AtomicOutcome> {
    let mode = mode_of(target)?;
    let temporary = temporary_for(target);
    let attempt = (|| {
        write_temporary(&temporary, text, mode)?;
        if !accept()? {
            return Ok(AtomicOutcome::Rejected);
        }
        fs::rename(&temporary, target)?;
        Ok(AtomicOutcome::Written)
    })();
    if !matches!(attempt, Ok(AtomicOutcome::Written)) {
        let _ = fs::remove_file(&temporary);
    }
    attempt
}

pub fn write_atomic_unconditionally(target: &Path, text: &str) -> io::Result<()> {
    write_atomic(target, text, &|| Ok(true)).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::os::unix::fs::MetadataExt;

    fn names(dir: &Path) -> Vec<String> {
        let mut found: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        found.sort();
        found
    }

    #[test]
    fn replaces_the_text_of_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "old").unwrap();
        assert_eq!(
            write_atomic(&path, "new", &|| Ok(true)).unwrap(),
            AtomicOutcome::Written
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn leaves_no_temporary_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        write_atomic_unconditionally(&path, "x").unwrap();
        assert_eq!(names(dir.path()), vec!["a.md"]);
    }

    #[test]
    fn gives_the_new_file_a_new_inode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "old").unwrap();
        let before = fs::metadata(&path).unwrap().ino();
        write_atomic_unconditionally(&path, "new").unwrap();
        assert_ne!(fs::metadata(&path).unwrap().ino(), before);
    }

    #[test]
    fn creates_a_file_that_does_not_exist_yet_readable_by_others() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        write_atomic_unconditionally(&path, "x").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o644);
    }

    #[test]
    fn keeps_the_mode_of_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o750)).unwrap();
        write_atomic_unconditionally(&path, "new").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o750);
    }

    #[test]
    fn writes_multibyte_text_as_utf8() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        write_atomic_unconditionally(&path, "✓ ação").unwrap();
        assert_eq!(fs::read(&path).unwrap(), "✓ ação".as_bytes());
    }

    #[test]
    fn fails_and_removes_the_temporary_file_when_the_rename_cannot_happen() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a.md");
        fs::create_dir(&target).unwrap();
        assert!(write_atomic_unconditionally(&target, "x").is_err());
        assert_eq!(names(dir.path()), vec!["a.md"]);
    }

    #[test]
    fn fails_when_the_folder_does_not_exist() {
        let dir = tempfile::tempdir().unwrap();
        let error =
            write_atomic_unconditionally(&dir.path().join("missing/a.md"), "x").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn asks_after_writing_the_temporary_file_and_before_the_rename() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "old").unwrap();
        let seen = RefCell::new(Vec::new());
        let outcome = write_atomic(&path, "new", &|| {
            seen.borrow_mut()
                .push((names(dir.path()), fs::read_to_string(&path).unwrap()));
            Ok(true)
        })
        .unwrap();
        assert_eq!(outcome, AtomicOutcome::Written);
        let seen = seen.into_inner();
        assert_eq!(seen[0].0.len(), 2);
        assert_eq!(seen[0].1, "old");
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn removes_the_temporary_file_and_never_renames_when_the_answer_is_no() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "old").unwrap();
        assert_eq!(
            write_atomic(&path, "new", &|| Ok(false)).unwrap(),
            AtomicOutcome::Rejected
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(names(dir.path()), vec!["a.md"]);
    }
}
