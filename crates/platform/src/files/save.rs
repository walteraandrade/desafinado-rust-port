use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::decode::{decode_disk, decode_markdown};
use super::handlers::FileSaveResult;
use super::markdown_path::is_markdown_path;
use super::requests::{MAX_FILE_BYTES, SaveRequest};
use super::write_atomic::{AtomicOutcome, write_atomic};
use crate::fs_failure::{describe_failure, is_not_found};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveDecision {
    Write,
    Conflict,
}

pub fn decide_save(disk: Option<&str>, expected: &str) -> SaveDecision {
    match disk {
        Some(disk) if decode_markdown(disk) == expected => SaveDecision::Write,
        _ => SaveDecision::Conflict,
    }
}

fn failed(reason: &str) -> FileSaveResult {
    FileSaveResult::Failed {
        reason: reason.to_owned(),
    }
}

fn too_large(text: &str) -> bool {
    text.len() > MAX_FILE_BYTES
}

fn saved_at(real: &Path) -> FileSaveResult {
    FileSaveResult::Saved {
        path: real.to_string_lossy().into_owned(),
        name: real
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

fn disk_text(real: &Path) -> io::Result<Option<String>> {
    let info = fs::metadata(real)?;
    if info.len() > MAX_FILE_BYTES as u64 {
        return Ok(None);
    }
    Ok(decode_disk(&fs::read(real)?))
}

fn disk_text_or_gone(real: &Path) -> io::Result<Option<String>> {
    match disk_text(real) {
        Err(error) if is_not_found(&error) => Ok(None),
        other => other,
    }
}

fn resolve_existing(path: &Path) -> io::Result<Option<PathBuf>> {
    match fs::canonicalize(path) {
        Ok(real) => Ok(Some(real)),
        Err(error) if is_not_found(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub fn save_markdown(request: &SaveRequest) -> FileSaveResult {
    match save_markdown_inner(request) {
        Ok(result) => result,
        Err(error) => failed(describe_failure(&error)),
    }
}

fn save_markdown_inner(request: &SaveRequest) -> io::Result<FileSaveResult> {
    if too_large(&request.text) {
        return Ok(failed("Too large to save (over 5 MiB)"));
    }
    let Some(real) = resolve_existing(Path::new(&request.path))? else {
        return Ok(FileSaveResult::Conflict);
    };
    if !is_markdown_path(&path_text(&real)) {
        return Ok(failed("Not a Markdown file"));
    }
    let holds = || -> io::Result<bool> {
        let disk = disk_text_or_gone(&real)?;
        Ok(decide_save(disk.as_deref(), &request.expected) == SaveDecision::Write)
    };
    if !holds()? {
        return Ok(FileSaveResult::Conflict);
    }
    Ok(match write_atomic(&real, &request.text, &holds)? {
        AtomicOutcome::Rejected => FileSaveResult::Conflict,
        AtomicOutcome::Written => saved_at(&real),
    })
}

pub fn save_markdown_as(path: &Path, text: &str) -> FileSaveResult {
    match save_markdown_as_inner(path, text) {
        Ok(result) => result,
        Err(error) => failed(describe_failure(&error)),
    }
}

fn save_markdown_as_inner(path: &Path, text: &str) -> io::Result<FileSaveResult> {
    if too_large(text) {
        return Ok(failed("Too large to save (over 5 MiB)"));
    }
    let target = resolve_existing(path)?.unwrap_or_else(|| path.to_path_buf());
    if !is_markdown_path(&path_text(&target)) {
        return Ok(failed("Not a Markdown file"));
    }
    write_atomic(&target, text, &|| Ok(true))?;
    Ok(saved_at(&fs::canonicalize(&target)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn request(path: &Path, text: &str, expected: &str) -> SaveRequest {
        SaveRequest {
            path: path_text(path),
            text: text.into(),
            expected: expected.into(),
        }
    }

    fn real_dir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let real = fs::canonicalize(dir.path()).unwrap();
        (dir, real)
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    fn entries(dir: &Path) -> Vec<String> {
        let mut found: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        found.sort();
        found
    }

    #[test]
    fn decide_save_writes_when_the_disk_holds_the_text_last_read() {
        assert_eq!(decide_save(Some("a"), "a"), SaveDecision::Write);
        assert_eq!(decide_save(Some(""), ""), SaveDecision::Write);
    }

    #[test]
    fn decide_save_stops_on_a_different_or_unreadable_disk() {
        assert_eq!(decide_save(Some("b"), "a"), SaveDecision::Conflict);
        assert_eq!(decide_save(None, "a"), SaveDecision::Conflict);
        assert_eq!(decide_save(Some("a1"), "a2"), SaveDecision::Conflict);
    }

    #[test]
    fn decide_save_does_not_count_crlf_or_a_mark_as_a_change() {
        assert_eq!(
            decide_save(Some("\u{feff}a\r\nb"), "a\nb"),
            SaveDecision::Write
        );
    }

    #[test]
    fn writes_when_the_disk_still_holds_the_text_last_read() {
        let (_dir, real) = real_dir();
        let path = real.join("SKILL.md");
        fs::write(&path, "# A\n").unwrap();
        assert_eq!(
            save_markdown(&request(&path, "# A, edited\n", "# A\n")),
            FileSaveResult::Saved {
                path: path_text(&path),
                name: "SKILL.md".into()
            }
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "# A, edited\n");
    }

    #[test]
    fn never_overwrites_a_file_that_changed_after_it_was_read() {
        let (_dir, real) = real_dir();
        let path = real.join("SKILL.md");
        fs::write(&path, "# A, changed by an agent\n").unwrap();
        assert_eq!(
            save_markdown(&request(&path, "# A, mine\n", "# A\n")),
            FileSaveResult::Conflict
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "# A, changed by an agent\n"
        );
        assert_eq!(entries(&real), vec!["SKILL.md"]);
    }

    #[test]
    fn treats_a_file_that_disappeared_as_a_change() {
        let (_dir, real) = real_dir();
        let path = real.join("SKILL.md");
        assert_eq!(
            save_markdown(&request(&path, "# A\n", "# A\n")),
            FileSaveResult::Conflict
        );
        assert!(entries(&real).is_empty());
    }

    #[test]
    fn treats_a_file_that_stopped_being_text_as_a_change() {
        let (_dir, real) = real_dir();
        let path = real.join("SKILL.md");
        fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(
            save_markdown(&request(&path, "# A\n", "# A\n")),
            FileSaveResult::Conflict
        );
    }

    #[test]
    fn does_not_count_crlf_as_a_change_and_writes_lf() {
        let (_dir, real) = real_dir();
        let path = real.join("SKILL.md");
        fs::write(&path, "# A\r\n\r\nB\r\n").unwrap();
        let result = save_markdown(&request(&path, "# A\n\nB, edited\n", "# A\n\nB\n"));
        assert!(matches!(result, FileSaveResult::Saved { .. }));
        assert_eq!(fs::read_to_string(&path).unwrap(), "# A\n\nB, edited\n");
    }

    #[test]
    fn keeps_the_file_mode() {
        let (_dir, real) = real_dir();
        let path = real.join("SKILL.md");
        fs::write(&path, "# A\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o750)).unwrap();
        save_markdown(&request(&path, "# B\n", "# A\n"));
        assert_eq!(mode(&path), 0o750);
    }

    #[test]
    fn keeps_a_symlink_a_symlink_writes_the_target_and_keeps_its_mode() {
        let (_dir, real) = real_dir();
        fs::create_dir(real.join("agents")).unwrap();
        let target = real.join("agents/SKILL.md");
        fs::write(&target, "# A\n").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
        fs::create_dir(real.join("claude")).unwrap();
        let link = real.join("claude/SKILL.md");
        symlink(&target, &link).unwrap();
        let result = save_markdown(&request(&link, "# B\n", "# A\n"));
        assert_eq!(
            result,
            FileSaveResult::Saved {
                path: path_text(&target),
                name: "SKILL.md".into()
            }
        );
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_link(&link).unwrap(), target);
        assert_eq!(fs::read_to_string(&target).unwrap(), "# B\n");
        assert_eq!(mode(&target), 0o640);
        assert_eq!(entries(&real.join("claude")), vec!["SKILL.md"]);
        assert_eq!(entries(&real.join("agents")), vec!["SKILL.md"]);
    }

    #[test]
    fn keeps_a_symlinked_skill_folder_in_place() {
        let (_dir, real) = real_dir();
        let folder = real.join("agents/skill");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("SKILL.md"), "# A\n").unwrap();
        fs::create_dir(real.join("claude")).unwrap();
        let link = real.join("claude/skill");
        symlink(&folder, &link).unwrap();
        save_markdown(&request(&link.join("SKILL.md"), "# B\n", "# A\n"));
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_to_string(folder.join("SKILL.md")).unwrap(),
            "# B\n"
        );
    }

    #[test]
    fn fails_with_the_old_text_intact_when_the_folder_cannot_be_written() {
        if rustix::process::geteuid().is_root() {
            return;
        }
        let (_dir, real) = real_dir();
        let path = real.join("SKILL.md");
        fs::write(&path, "# A\n").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o500)).unwrap();
        let result = save_markdown(&request(&path, "# B\n", "# A\n"));
        fs::set_permissions(&real, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(result, failed("Permission denied"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "# A\n");
    }

    #[test]
    fn refuses_a_file_that_is_not_markdown() {
        let (_dir, real) = real_dir();
        let path = real.join("run.sh");
        fs::write(&path, "echo hi\n").unwrap();
        assert_eq!(
            save_markdown(&request(&path, "x", "echo hi\n")),
            failed("Not a Markdown file")
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "echo hi\n");
    }

    #[test]
    fn says_not_a_file_when_a_folder_sits_where_the_file_was() {
        let (_dir, real) = real_dir();
        let path = real.join("SKILL.md");
        fs::create_dir(&path).unwrap();
        assert_eq!(
            save_markdown(&request(&path, "x", "x")),
            failed("Not a file")
        );
    }

    #[test]
    fn refuses_text_over_the_byte_limit_and_accepts_exactly_the_limit() {
        let (_dir, real) = real_dir();
        let path = real.join("SKILL.md");
        fs::write(&path, "a").unwrap();
        let over = "é".repeat(MAX_FILE_BYTES / 2 + 1);
        assert_eq!(
            save_markdown(&request(&path, &over, "a")),
            failed("Too large to save (over 5 MiB)")
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "a");
        let exact = "a".repeat(MAX_FILE_BYTES);
        assert!(matches!(
            save_markdown(&request(&path, &exact, "a")),
            FileSaveResult::Saved { .. }
        ));
    }

    #[test]
    fn save_as_creates_a_new_file() {
        let (_dir, real) = real_dir();
        let path = real.join("new.md");
        assert_eq!(
            save_markdown_as(&path, "# N\n"),
            FileSaveResult::Saved {
                path: path_text(&path),
                name: "new.md".into()
            }
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "# N\n");
    }

    #[test]
    fn save_as_replaces_an_existing_file_keeping_its_mode() {
        let (_dir, real) = real_dir();
        let path = real.join("a.md");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        save_markdown_as(&path, "new");
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(mode(&path), 0o600);
    }

    #[test]
    fn save_as_writes_through_a_symlink_and_keeps_it() {
        let (_dir, real) = real_dir();
        let target = real.join("t.md");
        fs::write(&target, "old").unwrap();
        let link = real.join("l.md");
        symlink(&target, &link).unwrap();
        let result = save_markdown_as(&link, "new");
        assert_eq!(
            result,
            FileSaveResult::Saved {
                path: path_text(&target),
                name: "t.md".into()
            }
        );
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
    }

    #[test]
    fn save_as_fails_when_the_folder_does_not_exist_or_the_name_is_not_markdown() {
        let (_dir, real) = real_dir();
        assert_eq!(
            save_markdown_as(&real.join("missing/a.md"), "x"),
            failed("File not found")
        );
        assert_eq!(
            save_markdown_as(&real.join("a.txt"), "x"),
            failed("Not a Markdown file")
        );
        assert!(matches!(
            save_markdown_as(&real.join("x.md"), &"a".repeat(MAX_FILE_BYTES + 1)),
            FileSaveResult::Failed { .. }
        ));
    }

    #[test]
    fn stops_with_a_conflict_when_the_file_changes_between_the_write_and_the_rename() {
        let (_dir, real) = real_dir();
        let path = real.join("a.md");
        fs::write(&path, "old").unwrap();
        let result = write_atomic(&path, "mine", &|| {
            fs::write(&path, "theirs")?;
            Ok(decide_save(disk_text_or_gone(&path)?.as_deref(), "old") == SaveDecision::Write)
        })
        .unwrap();
        assert_eq!(result, AtomicOutcome::Rejected);
        assert_eq!(fs::read_to_string(&path).unwrap(), "theirs");
        assert_eq!(entries(&real), vec!["a.md"]);
    }
}
