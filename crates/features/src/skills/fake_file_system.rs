use std::collections::HashMap;

use super::scan_skills::{DirectoryEntry, EntryKind, FsError, SkillFileSystem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FakeEntry {
    File(String),
    Link(String),
    DeniedFile,
    DeniedDirectory,
    Directory,
    Special,
}

pub type FakeTree = Vec<(String, FakeEntry)>;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    File(String),
    DeniedFile,
    Directory,
    Special,
    DeniedDirectory,
    Symlink(String),
}

struct Resolved {
    real: String,
    node: Node,
}

const MAX_LINK_HOPS: usize = 40;

pub fn link(target: &str) -> FakeEntry {
    FakeEntry::Link(target.to_string())
}

pub fn file(text: &str) -> FakeEntry {
    FakeEntry::File(text.to_string())
}

pub const DENIED_FILE: FakeEntry = FakeEntry::DeniedFile;

pub const DENIED_DIRECTORY: FakeEntry = FakeEntry::DeniedDirectory;

pub const EMPTY_DIRECTORY: FakeEntry = FakeEntry::Directory;

pub const SPECIAL: FakeEntry = FakeEntry::Special;

pub fn fake_tree(entries: &[(&str, FakeEntry)]) -> FakeTree {
    entries
        .iter()
        .map(|(path, entry)| ((*path).to_string(), entry.clone()))
        .collect()
}

fn segments_of(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .fold(Vec::new(), |mut kept, part| {
            if part == ".." {
                kept.pop();
            } else {
                kept.push(part.to_string());
            }
            kept
        })
}

fn parent_of(path: &str) -> String {
    let segments = segments_of(path);
    format!(
        "/{}",
        segments[..segments.len().saturating_sub(1)].join("/")
    )
}

fn name_of(path: &str) -> String {
    segments_of(path).pop().unwrap_or_default()
}

fn node_of(entry: &FakeEntry) -> Node {
    match entry {
        FakeEntry::File(text) => Node::File(text.clone()),
        FakeEntry::Link(target) => Node::Symlink(target.clone()),
        FakeEntry::DeniedFile => Node::DeniedFile,
        FakeEntry::DeniedDirectory => Node::DeniedDirectory,
        FakeEntry::Special => Node::Special,
        FakeEntry::Directory => Node::Directory,
    }
}

fn ancestors_of(path: &str) -> Vec<String> {
    let segments = segments_of(path);
    (0..segments.len().saturating_sub(1))
        .map(|index| format!("/{}", segments[..=index].join("/")))
        .collect()
}

fn kind_of(node: &Node) -> EntryKind {
    match node {
        Node::Symlink(_) => EntryKind::Symlink,
        Node::File(_) | Node::DeniedFile => EntryKind::File,
        Node::Special => EntryKind::Other,
        Node::Directory | Node::DeniedDirectory => EntryKind::Directory,
    }
}

pub struct FakeFileSystem {
    order: Vec<String>,
    nodes: HashMap<String, Node>,
}

pub fn create_fake_file_system(tree: &[(String, FakeEntry)]) -> FakeFileSystem {
    let explicit: Vec<(String, Node)> = tree
        .iter()
        .map(|(path, entry)| (format!("/{}", segments_of(path).join("/")), node_of(entry)))
        .collect();
    let implied: Vec<(String, Node)> = explicit
        .iter()
        .flat_map(|(path, _)| ancestors_of(path))
        .map(|path| (path, Node::Directory))
        .collect();
    let mut order = Vec::new();
    let mut nodes = HashMap::new();
    for (path, node) in std::iter::once(("/".to_string(), Node::Directory))
        .chain(implied)
        .chain(explicit)
    {
        if nodes.insert(path.clone(), node).is_none() {
            order.push(path);
        }
    }
    FakeFileSystem { order, nodes }
}

impl FakeFileSystem {
    fn resolve_from(&self, real: &str, rest: &[String], hops: usize) -> Result<Resolved, FsError> {
        if hops > MAX_LINK_HOPS {
            return Err(FsError::new("ELOOP", real));
        }
        let Some((next, remaining)) = rest.split_first() else {
            return Ok(Resolved {
                real: real.to_string(),
                node: self.nodes.get(real).cloned().unwrap_or(Node::Directory),
            });
        };
        let candidate = format!("{}/{next}", if real == "/" { "" } else { real });
        let node = self
            .nodes
            .get(&candidate)
            .ok_or_else(|| FsError::new("ENOENT", &candidate))?;
        match node {
            Node::Symlink(target) => {
                let base = if target.starts_with('/') { "/" } else { real };
                let mut segments = segments_of(&format!("{base}/{target}"));
                segments.extend(remaining.iter().cloned());
                self.resolve_from("/", &segments, hops + 1)
            }
            _ => self.resolve_from(&candidate, remaining, hops),
        }
    }

    fn resolve(&self, path: &str) -> Result<Resolved, FsError> {
        self.resolve_from("/", &segments_of(path), 0)
    }
}

impl SkillFileSystem for FakeFileSystem {
    fn list_directory(&self, path: &str) -> Result<Vec<DirectoryEntry>, FsError> {
        let Resolved { real, node } = self.resolve(path)?;
        match node {
            Node::DeniedDirectory => Err(FsError::new("EACCES", path)),
            Node::Directory => Ok(self
                .order
                .iter()
                .filter(|child| **child != real && parent_of(child) == real)
                .map(|child| DirectoryEntry {
                    name: name_of(child),
                    kind: kind_of(&self.nodes[child]),
                })
                .rev()
                .collect()),
            _ => Err(FsError::new("ENOTDIR", path)),
        }
    }

    fn real_path(&self, path: &str) -> Result<String, FsError> {
        self.resolve(path).map(|resolved| resolved.real)
    }

    fn read_text(&self, path: &str) -> Result<String, FsError> {
        match self.resolve(path)?.node {
            Node::DeniedFile => Err(FsError::new("EACCES", path)),
            Node::File(text) => Ok(text),
            _ => Err(FsError::new("EISDIR", path)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_system() -> FakeFileSystem {
        create_fake_file_system(&fake_tree(&[
            ("/home/u/skills/pdf/SKILL.md", file("pdf text")),
            ("/home/u/skills/pdf/refs/a.md", file("a")),
            ("/home/u/skills/alias", link("/home/u/other/target")),
            ("/home/u/skills/relative", link("pdf")),
            ("/home/u/skills/up", link("../other/target")),
            ("/home/u/skills/gone", link("/home/u/nowhere")),
            ("/home/u/skills/loop-a", link("/home/u/skills/loop-b")),
            ("/home/u/skills/loop-b", link("/home/u/skills/loop-a")),
            ("/home/u/skills/secret.md", DENIED_FILE),
            ("/home/u/skills/vault", DENIED_DIRECTORY),
            ("/home/u/skills/empty", EMPTY_DIRECTORY),
            ("/home/u/skills/socket", SPECIAL),
            ("/home/u/other/target/SKILL.md", file("target text")),
        ]))
    }

    fn names_in(fs: &FakeFileSystem, path: &str) -> Vec<String> {
        let mut names: Vec<String> = fs
            .list_directory(path)
            .expect("list")
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        names.sort();
        names
    }

    #[test]
    fn lists_entries_with_their_kinds_links_not_followed() {
        let fs = file_system();
        let entries = fs.list_directory("/home/u/skills").expect("list");
        let kind = |name: &str| {
            entries
                .iter()
                .find(|entry| entry.name == name)
                .map(|entry| entry.kind)
        };
        assert_eq!(kind("pdf"), Some(EntryKind::Directory));
        assert_eq!(kind("alias"), Some(EntryKind::Symlink));
        assert_eq!(kind("gone"), Some(EntryKind::Symlink));
        assert_eq!(kind("secret.md"), Some(EntryKind::File));
        assert_eq!(kind("empty"), Some(EntryKind::Directory));
        assert_eq!(kind("socket"), Some(EntryKind::Other));
    }

    #[test]
    fn implies_the_directories_above_a_file() {
        let fs = file_system();
        assert_eq!(names_in(&fs, "/home"), vec!["u"]);
        assert_eq!(
            names_in(&fs, "/home/u/skills/pdf"),
            vec!["SKILL.md", "refs"]
        );
    }

    #[test]
    fn lists_an_empty_directory() {
        assert_eq!(
            names_in(&file_system(), "/home/u/skills/empty"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn resolves_real_paths_through_absolute_and_relative_links() {
        let fs = file_system();
        let real = |path: &str| fs.real_path(path).expect("real path");
        assert_eq!(real("/home/u/skills/alias"), "/home/u/other/target");
        assert_eq!(real("/home/u/skills/relative"), "/home/u/skills/pdf");
        assert_eq!(real("/home/u/skills/up"), "/home/u/other/target");
        assert_eq!(
            real("/home/u/skills/alias/SKILL.md"),
            "/home/u/other/target/SKILL.md"
        );
        assert_eq!(real("/home/u/skills/pdf"), "/home/u/skills/pdf");
    }

    #[test]
    fn lists_and_reads_through_a_link() {
        let fs = file_system();
        assert_eq!(names_in(&fs, "/home/u/skills/alias"), vec!["SKILL.md"]);
        assert_eq!(
            fs.read_text("/home/u/skills/alias/SKILL.md").expect("text"),
            "target text"
        );
    }

    #[test]
    fn fails_on_a_broken_link_a_loop_and_a_missing_path() {
        let fs = file_system();
        assert!(fs.real_path("/home/u/skills/gone").is_err());
        assert!(fs.real_path("/home/u/skills/loop-a").is_err());
        assert!(fs.list_directory("/nope").is_err());
        assert!(fs.read_text("/nope").is_err());
    }

    #[test]
    fn fails_to_list_a_file_and_to_read_a_directory() {
        let fs = file_system();
        assert!(fs.list_directory("/home/u/skills/pdf/SKILL.md").is_err());
        assert!(fs.read_text("/home/u/skills/pdf").is_err());
    }

    #[test]
    fn fails_to_read_a_denied_file_and_to_list_a_denied_directory() {
        let fs = file_system();
        let read = fs.read_text("/home/u/skills/secret.md").unwrap_err();
        assert!(read.message.contains("EACCES"));
        let list = fs.list_directory("/home/u/skills/vault").unwrap_err();
        assert!(list.message.contains("EACCES"));
        assert_eq!(
            fs.real_path("/home/u/skills/secret.md").expect("real"),
            "/home/u/skills/secret.md"
        );
    }
}
