use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::dialogs::FileDialogs;
use super::markdown_path::with_markdown_extension;
use super::read::read_markdown;
use super::requests::{parse_path_request, parse_save_as_request, parse_save_request};
use super::save::{save_markdown, save_markdown_as};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FileReadResult {
    Opened {
        path: String,
        name: String,
        text: String,
    },
    Cancelled,
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FileSaveResult {
    Saved { path: String, name: String },
    Conflict,
    Cancelled,
    Failed { reason: String },
}

fn invalid_read() -> FileReadResult {
    FileReadResult::Failed {
        reason: "Invalid request".into(),
    }
}

fn invalid_save() -> FileSaveResult {
    FileSaveResult::Failed {
        reason: "Invalid request".into(),
    }
}

pub fn open_file(dialogs: &dyn FileDialogs) -> FileReadResult {
    match dialogs.pick_file() {
        None => FileReadResult::Cancelled,
        Some(path) => read_markdown(&path),
    }
}

pub fn read_file_request(path: &str) -> FileReadResult {
    match parse_path_request(path) {
        None => invalid_read(),
        Some(path) => read_markdown(Path::new(&path)),
    }
}

pub fn save_file_request(path: &str, text: &str, expected: &str) -> FileSaveResult {
    match parse_save_request(path, text, expected) {
        None => invalid_save(),
        Some(request) => save_markdown(&request),
    }
}

pub fn save_file_as_request(
    text: &str,
    suggested_name: &str,
    dialogs: &dyn FileDialogs,
) -> FileSaveResult {
    let Some(request) = parse_save_as_request(text, suggested_name) else {
        return invalid_save();
    };
    let Some(chosen) = dialogs.pick_save_path(&request.suggested_name) else {
        return FileSaveResult::Cancelled;
    };
    let chosen_text = chosen.to_string_lossy().into_owned();
    let target = with_markdown_extension(&chosen_text);
    if target != chosen_text && fs::symlink_metadata(&target).is_ok() {
        let name = Path::new(&target)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        return FileSaveResult::Failed {
            reason: format!("{name} already exists"),
        };
    }
    save_markdown_as(Path::new(&target), &request.text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    struct Fake {
        file: Option<PathBuf>,
        asked: Cell<u32>,
        suggested: std::cell::RefCell<Vec<String>>,
    }

    unsafe impl Sync for Fake {}

    impl Fake {
        fn new(file: Option<PathBuf>) -> Self {
            Self {
                file,
                asked: Cell::new(0),
                suggested: Default::default(),
            }
        }
    }

    impl FileDialogs for Fake {
        fn pick_file(&self) -> Option<PathBuf> {
            self.asked.set(self.asked.get() + 1);
            self.file.clone()
        }
        fn pick_save_path(&self, suggested_name: &str) -> Option<PathBuf> {
            self.asked.set(self.asked.get() + 1);
            self.suggested.borrow_mut().push(suggested_name.to_owned());
            self.file.clone()
        }
        fn pick_folder(&self) -> Option<PathBuf> {
            self.file.clone()
        }
    }

    fn real_dir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let real = fs::canonicalize(dir.path()).unwrap();
        (dir, real)
    }

    #[test]
    fn open_reads_the_file_the_dialog_returns() {
        let (_d, dir) = real_dir();
        let path = dir.join("a.md");
        fs::write(&path, "# A\n").unwrap();
        let result = open_file(&Fake::new(Some(path.clone())));
        assert_eq!(
            result,
            FileReadResult::Opened {
                path: path.to_string_lossy().into_owned(),
                name: "a.md".into(),
                text: "# A\n".into()
            }
        );
    }

    #[test]
    fn open_is_cancelled_when_the_dialog_is() {
        assert_eq!(open_file(&Fake::new(None)), FileReadResult::Cancelled);
    }

    #[test]
    fn read_request_reads_an_absolute_path_and_refuses_the_rest() {
        let (_d, dir) = real_dir();
        let path = dir.join("a.md");
        fs::write(&path, "x").unwrap();
        assert!(matches!(
            read_file_request(&path.to_string_lossy()),
            FileReadResult::Opened { .. }
        ));
        assert_eq!(read_file_request("a.md"), invalid_read());
    }

    #[test]
    fn save_request_saves_or_returns_the_conflict_as_a_value() {
        let (_d, dir) = real_dir();
        let path = dir.join("a.md");
        fs::write(&path, "old").unwrap();
        let p = path.to_string_lossy().into_owned();
        assert!(matches!(
            save_file_request(&p, "new", "old"),
            FileSaveResult::Saved { .. }
        ));
        assert_eq!(save_file_request(&p, "x", "old"), FileSaveResult::Conflict);
        assert_eq!(save_file_request("rel", "x", "y"), invalid_save());
    }

    #[test]
    fn save_as_asks_for_a_place_with_the_suggested_name_and_writes_there() {
        let (_d, dir) = real_dir();
        let path = dir.join("n.md");
        let dialogs = Fake::new(Some(path.clone()));
        let result = save_file_as_request("# N\n", "dir/notes.md", &dialogs);
        assert!(matches!(result, FileSaveResult::Saved { .. }));
        assert_eq!(dialogs.suggested.borrow().as_slice(), ["notes.md"]);
        assert_eq!(fs::read_to_string(&path).unwrap(), "# N\n");
    }

    #[test]
    fn save_as_adds_md_when_the_dialog_returns_a_name_without_it() {
        let (_d, dir) = real_dir();
        let result = save_file_as_request("x", "n.md", &Fake::new(Some(dir.join("notes"))));
        assert!(matches!(result, FileSaveResult::Saved { .. }));
        assert_eq!(fs::read_to_string(dir.join("notes.md")).unwrap(), "x");
    }

    #[test]
    fn save_as_refuses_to_replace_the_file_adding_md_would_reach() {
        let (_d, dir) = real_dir();
        fs::write(dir.join("notes.md"), "keep").unwrap();
        let result = save_file_as_request("x", "n.md", &Fake::new(Some(dir.join("notes"))));
        assert_eq!(
            result,
            FileSaveResult::Failed {
                reason: "notes.md already exists".into()
            }
        );
        assert_eq!(fs::read_to_string(dir.join("notes.md")).unwrap(), "keep");
    }

    #[test]
    fn save_as_counts_a_dangling_symlink_as_existing() {
        let (_d, dir) = real_dir();
        symlink(dir.join("nowhere"), dir.join("notes.md")).unwrap();
        let result = save_file_as_request("x", "n.md", &Fake::new(Some(dir.join("notes"))));
        assert!(matches!(result, FileSaveResult::Failed { .. }));
    }

    #[test]
    fn save_as_still_replaces_a_file_the_dialog_confirmed() {
        let (_d, dir) = real_dir();
        fs::write(dir.join("notes.md"), "old").unwrap();
        let result = save_file_as_request("new", "n.md", &Fake::new(Some(dir.join("notes.md"))));
        assert!(matches!(result, FileSaveResult::Saved { .. }));
        assert_eq!(fs::read_to_string(dir.join("notes.md")).unwrap(), "new");
    }

    #[test]
    fn save_as_is_cancelled_when_the_dialog_is_and_refuses_an_invalid_request_first() {
        let fake = Fake::new(None);
        assert_eq!(
            save_file_as_request("x", "n.md", &fake),
            FileSaveResult::Cancelled
        );
        let fake = Fake::new(None);
        assert_eq!(save_file_as_request("x", "dir/", &fake), invalid_save());
        assert_eq!(fake.asked.get(), 0);
    }
}
