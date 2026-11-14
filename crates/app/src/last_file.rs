use std::path::{Path, PathBuf};

use desafinado_platform::files::write_atomic_unconditionally;

pub const LAST_FILE_NAME: &str = "last-file.txt";

fn file_in(dir: &Path) -> PathBuf {
    dir.join(LAST_FILE_NAME)
}

pub fn load_last_file(dir: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(file_in(dir)).ok()?;
    let path = PathBuf::from(text.trim_end_matches('\n'));
    path.is_absolute().then_some(path)
}

pub fn save_last_file(dir: &Path, path: &Path) {
    let _ = write_atomic_unconditionally(&file_in(dir), &path.to_string_lossy());
}

pub fn clear_last_file(dir: &Path) {
    let _ = std::fs::remove_file(file_in(dir));
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
}
