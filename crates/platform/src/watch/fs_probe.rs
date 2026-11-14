use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use super::classify::is_ignored_name;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RootEntries {
    pub folders: Vec<String>,
    pub dangling: Vec<String>,
}

pub fn directory_identity(dir: &str) -> Option<String> {
    let info = fs::metadata(dir).ok()?;
    info.is_dir()
        .then(|| format!("{}:{}:{}", info.dev(), info.ino(), info.ctime()))
}

fn is_folder(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|info| info.is_dir())
}

fn normalize(path: &Path) -> String {
    use std::path::Component;
    let mut out = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out.to_string_lossy().into_owned()
}

pub fn root_entries(root: &str) -> io::Result<RootEntries> {
    let mut entries = RootEntries::default();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = Path::new(root).join(&name);
        let watchable = !is_ignored_name(&name);
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if watchable {
                entries.folders.push(path.to_string_lossy().into_owned());
            }
        } else if kind.is_symlink() {
            match fs::canonicalize(&path) {
                Ok(real) => {
                    if watchable && is_folder(&real) {
                        entries.folders.push(real.to_string_lossy().into_owned());
                    }
                }
                Err(_) => {
                    if let Ok(target) = fs::read_link(&path) {
                        entries
                            .dangling
                            .push(normalize(&Path::new(root).join(target)));
                    }
                }
            }
        }
    }
    entries.folders.sort();
    entries.dangling.sort();
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn identity_is_none_for_a_missing_folder_or_a_file_and_changes_when_recreated() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();
        assert_eq!(
            directory_identity(&base.join("none").to_string_lossy()),
            None
        );
        fs::write(base.join("f"), "x").unwrap();
        assert_eq!(directory_identity(&base.join("f").to_string_lossy()), None);
        let sub = base.join("sub");
        fs::create_dir(&sub).unwrap();
        let first = directory_identity(&sub.to_string_lossy()).unwrap();
        assert_eq!(directory_identity(&sub.to_string_lossy()).unwrap(), first);
        fs::remove_dir(&sub).unwrap();
        fs::create_dir(base.join("filler")).unwrap();
        fs::create_dir(&sub).unwrap();
        assert!(directory_identity(&sub.to_string_lossy()).is_some());
    }

    #[test]
    fn lists_folders_linked_folders_and_dangling_links_skipping_hidden_ones() {
        let dir = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();
        fs::create_dir(base.join("a")).unwrap();
        fs::create_dir(base.join(".hidden")).unwrap();
        fs::create_dir(base.join("node_modules")).unwrap();
        fs::write(base.join("file.md"), "x").unwrap();
        fs::create_dir(base.join("elsewhere")).unwrap();
        symlink(base.join("elsewhere"), base.join("linked")).unwrap();
        symlink("../missing/target", base.join("dangling")).unwrap();
        let entries = root_entries(&base.to_string_lossy()).unwrap();
        let root = base.to_string_lossy();
        assert_eq!(
            entries.folders,
            [
                format!("{root}/a"),
                format!("{root}/elsewhere"),
                format!("{root}/elsewhere")
            ]
        );
        assert_eq!(
            entries.dangling,
            [base
                .parent()
                .unwrap()
                .join("missing/target")
                .to_string_lossy()
                .into_owned()]
        );
    }

    #[test]
    fn fails_for_a_missing_root() {
        assert!(root_entries("/definitely/not/here").is_err());
    }
}
