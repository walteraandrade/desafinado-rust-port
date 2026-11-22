use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use crate::files::is_markdown_path;

fn is_file_url(arg: &str) -> bool {
    arg.len() >= 5 && arg[..5].eq_ignore_ascii_case("file:")
}

fn has_url_scheme(arg: &str) -> bool {
    let mut chars = arg.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    for c in chars {
        match c {
            ':' => return true,
            c if c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-') => {}
            _ => return false,
        }
    }
    false
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

pub fn path_from_file_url(url: &str) -> Option<PathBuf> {
    let rest = url.get(5..)?;
    let after = rest.strip_prefix("//")?;
    let slash = after.find('/')?;
    let host = &after[..slash];
    if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
        return None;
    }
    let path = percent_decode(&after[slash..])?;
    (!path.contains('\0')).then(|| PathBuf::from(path))
}

fn path_of(arg: &str, cwd: &Path) -> Option<PathBuf> {
    if is_file_url(arg) {
        return path_from_file_url(arg);
    }
    if arg.starts_with('-') || has_url_scheme(arg) {
        return None;
    }
    let path = Path::new(arg);
    Some(if path.is_absolute() {
        normalize(path)
    } else {
        normalize(&cwd.join(path))
    })
}

fn normalize(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn launch_file_of(argv: &[String], cwd: &Path) -> Option<PathBuf> {
    argv.iter()
        .skip(1)
        .filter_map(|arg| path_of(arg, cwd))
        .find(|path| is_markdown_path(path))
}

#[derive(Debug, Default)]
pub struct LaunchQueue {
    pending: Mutex<Option<PathBuf>>,
}

impl LaunchQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn offer(&self, argv: &[String], cwd: &Path) -> bool {
        let Some(path) = launch_file_of(argv, cwd) else {
            return false;
        };
        let real = std::fs::canonicalize(&path).unwrap_or(path);
        *self.pending.lock().unwrap_or_else(PoisonError::into_inner) = Some(real);
        true
    }

    pub fn take(&self) -> Option<PathBuf> {
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn resolves_a_relative_path_against_the_working_directory() {
        assert_eq!(
            launch_file_of(&argv(&["app", "notes/SKILL.md"]), Path::new("/work")),
            Some(PathBuf::from("/work/notes/SKILL.md"))
        );
    }

    #[test]
    fn accepts_an_absolute_path_and_a_file_url() {
        assert_eq!(
            launch_file_of(&argv(&["app", "/a/b.md"]), Path::new("/w")),
            Some(PathBuf::from("/a/b.md"))
        );
        assert_eq!(
            launch_file_of(&argv(&["app", "file:///a/my%20notes.md"]), Path::new("/w")),
            Some(PathBuf::from("/a/my notes.md"))
        );
    }

    #[test]
    fn skips_the_executable_switches_urls_and_non_markdown_arguments() {
        assert_eq!(
            launch_file_of(
                &argv(&[
                    "/x/first.md",
                    "--flag",
                    "--x=a.md",
                    "https://a/b.md",
                    "c.txt",
                    "d.markdown"
                ]),
                Path::new("/w")
            ),
            Some(PathBuf::from("/w/d.markdown"))
        );
        assert_eq!(launch_file_of(&argv(&["app"]), Path::new("/w")), None);
    }

    #[test]
    fn ignores_a_file_url_for_another_host_or_a_bad_one() {
        assert_eq!(
            launch_file_of(&argv(&["app", "file://host/a.md"]), Path::new("/w")),
            None
        );
        assert_eq!(
            launch_file_of(&argv(&["app", "file:///a/%zz.md"]), Path::new("/w")),
            None
        );
    }

    #[test]
    fn offers_the_real_path_and_hands_it_over_once() {
        let dir = tempfile::tempdir().unwrap();
        let real_dir = fs::canonicalize(dir.path()).unwrap();
        let target = real_dir.join("t.md");
        fs::write(&target, "x").unwrap();
        let link = real_dir.join("l.md");
        symlink(&target, &link).unwrap();
        let queue = LaunchQueue::new();
        assert!(!queue.offer(&argv(&["app", "--x"]), &real_dir));
        assert_eq!(queue.take(), None);
        assert!(queue.offer(&argv(&["app", "l.md"]), &real_dir));
        assert_eq!(queue.take(), Some(target));
        assert_eq!(queue.take(), None);
    }

    #[test]
    fn keeps_the_path_when_it_cannot_be_resolved() {
        let queue = LaunchQueue::new();
        assert!(queue.offer(&argv(&["app", "/nowhere/a.md"]), Path::new("/w")));
        assert_eq!(queue.take(), Some(PathBuf::from("/nowhere/a.md")));
    }

    #[test]
    fn the_latest_offer_wins() {
        let queue = LaunchQueue::new();
        queue.offer(&argv(&["app", "/a/1.md"]), Path::new("/w"));
        queue.offer(&argv(&["app", "/a/2.md"]), Path::new("/w"));
        assert_eq!(queue.take(), Some(PathBuf::from("/a/2.md")));
    }
}
