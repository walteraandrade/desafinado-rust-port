use std::fs;
use std::path::Path;

use super::decode::{decode_disk, decode_markdown};
use super::handlers::FileReadResult;
use super::markdown_path::is_markdown_path;
use super::requests::MAX_FILE_BYTES;
use crate::fs_failure::describe_failure;

fn failed(reason: &str) -> FileReadResult {
    FileReadResult::Failed {
        reason: reason.to_owned(),
    }
}

pub fn read_markdown(path: &Path) -> FileReadResult {
    read_markdown_limited(path, MAX_FILE_BYTES as u64)
}

pub fn read_markdown_limited(path: &Path, max_bytes: u64) -> FileReadResult {
    let real = match fs::canonicalize(path) {
        Ok(real) => real,
        Err(error) => return failed(describe_failure(&error)),
    };
    if !is_markdown_path(&real.to_string_lossy()) {
        return failed("Not a Markdown file");
    }
    let info = match fs::metadata(&real) {
        Ok(info) => info,
        Err(error) => return failed(describe_failure(&error)),
    };
    if !info.is_file() {
        return failed("Not a file");
    }
    if info.len() > max_bytes {
        return failed("File is too large");
    }
    let bytes = match fs::read(&real) {
        Ok(bytes) => bytes,
        Err(error) => return failed(describe_failure(&error)),
    };
    match decode_disk(&bytes) {
        None => failed("Not valid UTF-8 text"),
        Some(raw) => FileReadResult::Opened {
            name: real
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: real,
            text: decode_markdown(&raw),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn returns_the_real_path_the_name_and_the_text() {
        let dir = tempfile::tempdir().unwrap();
        let real = fs::canonicalize(dir.path()).unwrap();
        let path = real.join("SKILL.md");
        fs::write(&path, "# A\n").unwrap();
        assert_eq!(
            read_markdown(&path),
            FileReadResult::Opened {
                path: path.clone(),
                name: "SKILL.md".into(),
                text: "# A\n".into()
            }
        );
    }

    #[test]
    fn decodes_like_the_editor() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "\u{feff}# A\r\n\r\nB\r\n").unwrap();
        let FileReadResult::Opened { text, .. } = read_markdown(&path) else {
            panic!("not opened")
        };
        assert_eq!(text, "# A\n\nB\n");
    }

    #[test]
    fn resolves_a_symlink_to_the_file_it_points_at() {
        let dir = tempfile::tempdir().unwrap();
        let real_dir = fs::canonicalize(dir.path()).unwrap();
        let target = real_dir.join("target.md");
        fs::write(&target, "x").unwrap();
        let link = real_dir.join("SKILL.md");
        symlink(&target, &link).unwrap();
        let FileReadResult::Opened { path, name, .. } = read_markdown(&link) else {
            panic!("not opened")
        };
        assert_eq!(path, target);
        assert_eq!(name, "target.md");
    }

    #[test]
    fn says_when_the_file_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            read_markdown(&dir.path().join("none.md")),
            failed("File not found")
        );
    }

    #[test]
    fn refuses_a_symlink_that_points_at_a_file_that_is_not_markdown() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("run.sh");
        fs::write(&target, "echo").unwrap();
        let link = dir.path().join("SKILL.md");
        symlink(&target, &link).unwrap();
        assert_eq!(read_markdown(&link), failed("Not a Markdown file"));
    }

    #[test]
    fn refuses_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("notes.md");
        fs::create_dir(&folder).unwrap();
        assert_eq!(read_markdown(&folder), failed("Not a file"));
    }

    #[test]
    fn refuses_a_file_over_the_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "12345").unwrap();
        assert_eq!(read_markdown_limited(&path, 4), failed("File is too large"));
        assert!(matches!(
            read_markdown_limited(&path, 5),
            FileReadResult::Opened { .. }
        ));
    }

    #[test]
    fn refuses_bytes_that_are_not_utf8() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(read_markdown(&path), failed("Not valid UTF-8 text"));
    }
}
