use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub fn is_executable_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|info| info.is_file() && info.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

pub fn candidate_paths(path: Option<&str>, home: &Path) -> Vec<PathBuf> {
    path.unwrap_or("")
        .split(':')
        .filter(|directory| !directory.is_empty())
        .map(|directory| Path::new(directory).join("claude"))
        .chain(std::iter::once(home.join(".local/bin/claude")))
        .collect()
}

pub const CLAUDE_CLI_VARIABLE: &str = "DESAFINADO_CLAUDE_CLI";

pub fn find_claude(
    path: Option<&str>,
    home: &Path,
    is_executable: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    candidate_paths(path, home)
        .into_iter()
        .find(|candidate| is_executable(candidate))
}

pub fn find_claude_with_override(
    chosen: Option<&str>,
    path: Option<&str>,
    home: &Path,
    is_executable: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    match chosen.filter(|chosen| !chosen.is_empty()) {
        Some(chosen) => Some(PathBuf::from(chosen)).filter(|chosen| is_executable(chosen)),
        None => find_claude(path, home, is_executable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(paths: &'static [&'static str]) -> impl Fn(&Path) -> bool {
        move |p| paths.iter().any(|candidate| Path::new(candidate) == p)
    }

    #[test]
    fn takes_the_first_match_on_path() {
        let found = find_claude(
            Some("/a:/b"),
            Path::new("/home/u"),
            &only(&["/a/claude", "/b/claude"]),
        );
        assert_eq!(found, Some(PathBuf::from("/a/claude")));
    }

    #[test]
    fn falls_back_to_local_bin() {
        let found = find_claude(
            Some("/a"),
            Path::new("/home/u"),
            &only(&["/home/u/.local/bin/claude"]),
        );
        assert_eq!(found, Some(PathBuf::from("/home/u/.local/bin/claude")));
    }

    #[test]
    fn works_with_no_path_at_all_and_returns_none_when_it_is_nowhere() {
        assert_eq!(
            find_claude(
                None,
                Path::new("/home/u"),
                &only(&["/home/u/.local/bin/claude"])
            ),
            Some(PathBuf::from("/home/u/.local/bin/claude"))
        );
        assert_eq!(
            find_claude(Some("::/a"), Path::new("/home/u"), &only(&[])),
            None
        );
    }

    #[test]
    fn a_chosen_cli_wins_over_path_and_is_never_replaced_when_missing() {
        let check = only(&["/a/claude", "/dev/fake"]);
        assert_eq!(
            find_claude_with_override(Some("/dev/fake"), Some("/a"), Path::new("/h"), &check),
            Some(PathBuf::from("/dev/fake"))
        );
        assert_eq!(
            find_claude_with_override(Some("/gone"), Some("/a"), Path::new("/h"), &check),
            None
        );
        assert_eq!(
            find_claude_with_override(Some(""), Some("/a"), Path::new("/h"), &check),
            Some(PathBuf::from("/a/claude"))
        );
    }

    #[test]
    fn the_real_check_wants_an_executable_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("claude");
        fs::write(&file, "#!/bin/sh\n").unwrap();
        assert!(!is_executable_file(&file));
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(is_executable_file(&file));
        assert!(!is_executable_file(dir.path()));
        assert!(!is_executable_file(&dir.path().join("missing")));
    }
}
