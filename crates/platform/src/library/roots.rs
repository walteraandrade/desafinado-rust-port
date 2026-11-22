use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::files::write_atomic_unconditionally;

pub const ROOTS_FILE_NAME: &str = "library-roots.json";

pub fn default_roots(home: &str) -> Vec<String> {
    vec![
        format!("{home}/.claude/skills"),
        format!("{home}/.agents/skills"),
    ]
}

pub fn display_path(path: &str, home: &str) -> String {
    if path == home {
        "~".to_owned()
    } else if let Some(rest) = path.strip_prefix(&format!("{home}/")) {
        format!("~/{rest}")
    } else {
        path.to_owned()
    }
}

fn is_storable(value: &serde_json::Value) -> Option<String> {
    let text = value.as_str()?;
    (!text.is_empty() && !text.contains('\0') && Path::new(text).is_absolute())
        .then(|| text.to_owned())
}

pub fn parse_stored_roots(text: &str) -> Option<Vec<String>> {
    let parsed: serde_json::Value = serde_json::from_str(text).ok()?;
    let roots = parsed.as_object()?.get("projectRoots")?.as_array()?;
    let mut unique: Vec<String> = Vec::new();
    for value in roots {
        let root = is_storable(value)?;
        if !unique.contains(&root) {
            unique.push(root);
        }
    }
    Some(unique)
}

pub fn serialize_roots(project_roots: &[String]) -> String {
    let value = serde_json::json!({ "projectRoots": project_roots });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&value).unwrap_or_default()
    )
}

pub fn with_root(roots: &[String], root: &str) -> Vec<String> {
    let mut next = roots.to_vec();
    if !next.iter().any(|candidate| candidate == root) {
        next.push(root.to_owned());
    }
    next
}

pub fn without_root(roots: &[String], root: &str) -> Vec<String> {
    roots
        .iter()
        .filter(|candidate| *candidate != root)
        .cloned()
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootsLoad {
    Loaded(Vec<String>),
    Unreadable,
}

#[derive(Debug, Clone)]
pub struct RootsStore {
    file: PathBuf,
}

impl RootsStore {
    pub fn new(user_data_dir: &Path) -> Self {
        Self {
            file: user_data_dir.join(ROOTS_FILE_NAME),
        }
    }

    pub fn load(&self) -> RootsLoad {
        match fs::read_to_string(&self.file) {
            Ok(text) => parse_stored_roots(&text).map_or(RootsLoad::Unreadable, RootsLoad::Loaded),
            Err(error) if error.kind() == io::ErrorKind::NotFound => RootsLoad::Loaded(Vec::new()),
            Err(_) => RootsLoad::Unreadable,
        }
    }

    pub fn save(&self, project_roots: &[String]) -> io::Result<()> {
        if let Some(parent) = self.file.parent() {
            fs::create_dir_all(parent)?;
        }
        write_atomic_unconditionally(&self.file, &serialize_roots(project_roots))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn defaults_are_claude_then_agents() {
        assert_eq!(
            default_roots("/home/u"),
            ["/home/u/.claude/skills", "/home/u/.agents/skills"]
        );
    }

    #[test]
    fn labels_a_path_under_home_with_a_tilde() {
        assert_eq!(display_path("/home/u", "/home/u"), "~");
        assert_eq!(display_path("/home/u/code/x", "/home/u"), "~/code/x");
        assert_eq!(display_path("/home/user2/x", "/home/u"), "/home/user2/x");
        assert_eq!(display_path("/srv/x", "/home/u"), "/srv/x");
    }

    #[test]
    fn parses_stored_roots_dropping_duplicates_and_refusing_bad_shapes() {
        assert_eq!(
            parse_stored_roots(r#"{"projectRoots":["/a","/b","/a"]}"#),
            Some(roots(&["/a", "/b"]))
        );
        for bad in [
            "nope",
            "[]",
            r"{}",
            r#"{"projectRoots":"/a"}"#,
            r#"{"projectRoots":["rel"]}"#,
            r#"{"projectRoots":[""]}"#,
            r#"{"projectRoots":[1]}"#,
            r#"{"projectRoots":["/a\u0000b"]}"#,
        ] {
            assert_eq!(parse_stored_roots(bad), None, "{bad}");
        }
    }

    #[test]
    fn serializes_with_two_space_indent_and_a_trailing_newline() {
        assert_eq!(
            serialize_roots(&roots(&["/a"])),
            "{\n  \"projectRoots\": [\n    \"/a\"\n  ]\n}\n"
        );
    }

    #[test]
    fn adds_and_removes_roots() {
        assert_eq!(with_root(&roots(&["/a"]), "/a"), roots(&["/a"]));
        assert_eq!(with_root(&roots(&["/a"]), "/b"), roots(&["/a", "/b"]));
        assert_eq!(without_root(&roots(&["/a", "/b"]), "/a"), roots(&["/b"]));
    }

    #[test]
    fn the_store_loads_nothing_for_a_missing_file_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = RootsStore::new(dir.path());
        assert_eq!(store.load(), RootsLoad::Loaded(Vec::new()));
        store.save(&roots(&["/a", "/b"])).unwrap();
        assert_eq!(store.load(), RootsLoad::Loaded(roots(&["/a", "/b"])));
    }

    #[test]
    fn the_store_reports_a_bad_file_as_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(ROOTS_FILE_NAME), "{bad").unwrap();
        assert_eq!(RootsStore::new(dir.path()).load(), RootsLoad::Unreadable);
    }
}
