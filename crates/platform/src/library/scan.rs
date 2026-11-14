use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;

use serde::Serialize;

use super::frontmatter::{FrontmatterProblem, FrontmatterReading, read_frontmatter};

pub const SKILL_FILE_NAME: &str = "SKILL.md";
pub const DEFAULT_MAX_DEPTH: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryEntry {
    pub name: String,
    pub kind: EntryKind,
}

pub trait SkillFileSystem {
    fn list_directory(&self, path: &str) -> io::Result<Vec<DirectoryEntry>>;
    fn real_path(&self, path: &str) -> io::Result<String>;
    fn read_text(&self, path: &str) -> io::Result<String>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RealFileSystem;

impl SkillFileSystem for RealFileSystem {
    fn list_directory(&self, path: &str) -> io::Result<Vec<DirectoryEntry>> {
        fs::read_dir(path)?
            .map(|entry| {
                let entry = entry?;
                let kind = entry.file_type()?;
                Ok(DirectoryEntry {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    kind: if kind.is_symlink() {
                        EntryKind::Symlink
                    } else if kind.is_dir() {
                        EntryKind::Directory
                    } else if kind.is_file() {
                        EntryKind::File
                    } else {
                        EntryKind::Other
                    },
                })
            })
            .collect()
    }

    fn real_path(&self, path: &str) -> io::Result<String> {
        Ok(fs::canonicalize(Path::new(path))?
            .to_string_lossy()
            .into_owned())
    }

    fn read_text(&self, path: &str) -> io::Result<String> {
        fs::read_to_string(path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFile {
    pub path: String,
    pub name: String,
    pub is_markdown: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub real_path: String,
    pub folder_name: String,
    pub root: String,
    pub name: String,
    pub description: Option<String>,
    pub problem: Option<FrontmatterProblem>,
    pub files: Vec<SkillFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RootScan {
    pub root: String,
    pub readable: bool,
    pub skills: Vec<Skill>,
}

fn join_path(directory: &str, name: &str) -> String {
    format!("{}/{name}", directory.trim_end_matches(['/', '\\']))
}

fn compare_ignoring_case(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

fn is_hidden(entry: &DirectoryEntry) -> bool {
    entry.name.starts_with('.') || entry.name == "node_modules"
}

fn is_markdown(name: &str) -> bool {
    name.to_lowercase().ends_with(".md")
}

fn text_or_none(value: Option<&serde_norway::Value>) -> Option<String> {
    let text = value?.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

struct Described {
    name: String,
    description: Option<String>,
    problem: Option<FrontmatterProblem>,
}

fn describe_skill_file(text: &str, folder_name: &str) -> Described {
    let stripped = text.strip_prefix('\u{feff}').unwrap_or(text);
    let normalized = stripped.replace("\r\n", "\n").replace('\r', "\n");
    match read_frontmatter(&normalized) {
        FrontmatterReading::Present(fields) => Described {
            name: text_or_none(fields.get("name")).unwrap_or_else(|| folder_name.to_owned()),
            description: text_or_none(fields.get("description")),
            problem: None,
        },
        FrontmatterReading::Absent => Described {
            name: folder_name.to_owned(),
            description: None,
            problem: None,
        },
        FrontmatterReading::Invalid(problem) => Described {
            name: folder_name.to_owned(),
            description: None,
            problem: Some(problem),
        },
    }
}

struct Visit<'a> {
    directory: &'a str,
    children: &'a [DirectoryEntry],
    prefix: &'a str,
    depth: usize,
    ancestors: &'a HashSet<String>,
}

fn visit_files(fs: &dyn SkillFileSystem, max_depth: usize, visit: &Visit) -> Vec<SkillFile> {
    let mut entries: Vec<&DirectoryEntry> = visit
        .children
        .iter()
        .filter(|entry| !is_hidden(entry) && entry.kind != EntryKind::Other)
        .collect();
    entries.sort_by(|a, b| compare_ignoring_case(&a.name, &b.name));
    let mut found = Vec::new();
    for entry in entries {
        let name = format!("{}{}", visit.prefix, entry.name);
        let Ok(real) = fs.real_path(&join_path(visit.directory, &entry.name)) else {
            continue;
        };
        let file = SkillFile {
            path: real.clone(),
            is_markdown: is_markdown(&name),
            name: name.clone(),
        };
        if entry.kind == EntryKind::File {
            found.push(file);
            continue;
        }
        if entry.kind == EntryKind::Directory && visit.depth >= max_depth {
            continue;
        }
        let Ok(nested) = fs.list_directory(&real) else {
            if entry.kind == EntryKind::Symlink {
                found.push(file);
            }
            continue;
        };
        if visit.depth >= max_depth || visit.ancestors.contains(&real) {
            continue;
        }
        let mut ancestors = visit.ancestors.clone();
        ancestors.insert(real.clone());
        found.extend(visit_files(
            fs,
            max_depth,
            &Visit {
                directory: &real,
                children: &nested,
                prefix: &format!("{name}/"),
                depth: visit.depth + 1,
                ancestors: &ancestors,
            },
        ));
    }
    found
}

fn order_files(mut files: Vec<SkillFile>) -> Vec<SkillFile> {
    files.sort_by(|a, b| {
        (b.name == SKILL_FILE_NAME)
            .cmp(&(a.name == SKILL_FILE_NAME))
            .then_with(|| compare_ignoring_case(&a.name, &b.name))
    });
    let mut seen = HashSet::new();
    files.retain(|file| seen.insert(file.path.clone()));
    files
}

fn load_skill(
    fs: &dyn SkillFileSystem,
    max_depth: usize,
    root: &str,
    folder_name: &str,
    real_path: &str,
) -> Option<Skill> {
    let children = fs.list_directory(real_path).ok()?;
    let has_skill_file = children.iter().any(|entry| {
        entry.name == SKILL_FILE_NAME && matches!(entry.kind, EntryKind::File | EntryKind::Symlink)
    });
    if !has_skill_file {
        return None;
    }
    let skill_file = fs.real_path(&join_path(real_path, SKILL_FILE_NAME)).ok()?;
    let text = fs.read_text(&skill_file).ok()?;
    let ancestors = HashSet::from([real_path.to_owned()]);
    let files = visit_files(
        fs,
        max_depth,
        &Visit {
            directory: real_path,
            children: &children,
            prefix: "",
            depth: 0,
            ancestors: &ancestors,
        },
    );
    let described = describe_skill_file(&text, folder_name);
    Some(Skill {
        real_path: real_path.to_owned(),
        folder_name: folder_name.to_owned(),
        root: root.to_owned(),
        name: described.name,
        description: described.description,
        problem: described.problem,
        files: order_files(files),
    })
}

fn scan_root(
    fs: &dyn SkillFileSystem,
    max_depth: usize,
    root: &str,
    claimed: &mut HashSet<String>,
) -> RootScan {
    let Ok(entries) = fs.list_directory(root) else {
        return RootScan {
            root: root.to_owned(),
            readable: false,
            skills: Vec::new(),
        };
    };
    let mut folders: Vec<&DirectoryEntry> = entries
        .iter()
        .filter(|e| matches!(e.kind, EntryKind::Directory | EntryKind::Symlink))
        .collect();
    folders.sort_by(|a, b| compare_ignoring_case(&a.name, &b.name));
    let mut candidates = Vec::new();
    for entry in folders {
        let Ok(real) = fs.real_path(&join_path(root, &entry.name)) else {
            continue;
        };
        if claimed.insert(real.clone()) {
            candidates.push((entry.name.clone(), real));
        }
    }
    let mut skills: Vec<Skill> = candidates
        .iter()
        .filter_map(|(folder, real)| load_skill(fs, max_depth, root, folder, real))
        .collect();
    skills.sort_by(|a, b| {
        compare_ignoring_case(&a.name, &b.name)
            .then_with(|| compare_ignoring_case(&a.folder_name, &b.folder_name))
    });
    RootScan {
        root: root.to_owned(),
        readable: true,
        skills,
    }
}

pub fn scan_skill_roots(
    fs: &dyn SkillFileSystem,
    roots: &[String],
    max_depth: usize,
) -> Vec<RootScan> {
    let mut claimed = HashSet::new();
    roots
        .iter()
        .map(|root| scan_root(fs, max_depth, root, &mut claimed))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};

    fn skill_text(name: &str) -> String {
        format!("---\nname: {name}\ndescription: Does a thing.\n---\n\n# {name}\n")
    }

    struct Tree {
        _dir: tempfile::TempDir,
        base: PathBuf,
    }

    fn tree() -> Tree {
        let dir = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();
        Tree { _dir: dir, base }
    }

    impl Tree {
        fn path(&self, relative: &str) -> PathBuf {
            self.base.join(relative)
        }
        fn write(&self, relative: &str, text: &str) {
            let path = self.path(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        fn link(&self, from: &str, to: &str) {
            let target = self.path(to);
            let link = self.path(from);
            fs::create_dir_all(link.parent().unwrap()).unwrap();
            symlink(target, link).unwrap();
        }
        fn root(&self, relative: &str) -> String {
            self.path(relative).to_string_lossy().into_owned()
        }
    }

    fn scan(roots: &[String]) -> Vec<RootScan> {
        scan_skill_roots(&RealFileSystem, roots, DEFAULT_MAX_DEPTH)
    }

    fn folders(scan: &RootScan) -> Vec<&str> {
        scan.skills.iter().map(|s| s.folder_name.as_str()).collect()
    }

    #[test]
    fn a_skill_is_a_folder_under_a_root_with_a_skill_md() {
        let t = tree();
        t.write("claude/pdf/SKILL.md", &skill_text("pdf-processing"));
        let scans = scan(&[t.root("claude")]);
        let skill = &scans[0].skills[0];
        assert_eq!(skill.real_path, t.root("claude/pdf"));
        assert_eq!(skill.folder_name, "pdf");
        assert_eq!(skill.name, "pdf-processing");
        assert_eq!(skill.description.as_deref(), Some("Does a thing."));
        assert!(skill.problem.is_none());
    }

    #[test]
    fn falls_back_to_the_folder_name_and_trims() {
        let t = tree();
        t.write("r/a/SKILL.md", "---\nname: \"\"\ndescription: 5\n---\n");
        t.write("r/b/SKILL.md", "# no frontmatter\n");
        t.write(
            "r/c/SKILL.md",
            "---\nname:   padded  \ndescription:   d   \n---\n",
        );
        let scans = scan(&[t.root("r")]);
        let by: Vec<(&str, &str, Option<&str>)> = scans[0]
            .skills
            .iter()
            .map(|s| {
                (
                    s.folder_name.as_str(),
                    s.name.as_str(),
                    s.description.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            by,
            [
                ("a", "a", None),
                ("b", "b", None),
                ("c", "padded", Some("d"))
            ]
        );
    }

    #[test]
    fn keeps_the_folder_name_and_reports_the_problem_for_a_broken_frontmatter() {
        let t = tree();
        t.write("r/a/SKILL.md", "---\nname: a\ndescription: [oops\n---\n");
        let skill = &scan(&[t.root("r")])[0].skills[0];
        assert_eq!(skill.name, "a");
        assert_eq!(skill.problem.as_ref().unwrap().rule, "yaml-syntax-error");
    }

    #[test]
    fn reads_a_skill_md_with_a_bom_and_crlf() {
        let t = tree();
        t.write(
            "r/a/SKILL.md",
            "\u{feff}---\r\nname: crlf\r\ndescription: d\r\n---\r\n# x\r\n",
        );
        let skill = &scan(&[t.root("r")])[0].skills[0];
        assert_eq!(skill.name, "crlf");
        assert!(skill.problem.is_none());
    }

    #[test]
    fn skips_what_is_not_a_skill() {
        let t = tree();
        t.write("r/empty/readme.md", "x");
        t.write("r/dir/SKILL.md/inner.md", "x");
        t.write("r/loose.md", "x");
        fs::create_dir_all(t.path("r/none")).unwrap();
        assert!(scan(&[t.root("r")])[0].skills.is_empty());
    }

    #[test]
    fn shows_a_skill_reached_through_a_link_and_through_its_target_once() {
        let t = tree();
        t.write("agents/pdf/SKILL.md", &skill_text("pdf"));
        t.link("claude/pdf", "agents/pdf");
        let scans = scan(&[t.root("claude"), t.root("agents")]);
        assert_eq!(folders(&scans[0]), ["pdf"]);
        assert_eq!(scans[0].skills[0].real_path, t.root("agents/pdf"));
        assert!(scans[1].skills.is_empty());
    }

    #[test]
    fn shows_a_skill_once_when_the_same_root_is_given_twice_or_a_root_is_a_link() {
        let t = tree();
        t.write("a/pdf/SKILL.md", &skill_text("pdf"));
        t.link("b", "a");
        let scans = scan(&[t.root("a"), t.root("a"), t.root("b")]);
        assert_eq!(folders(&scans[0]), ["pdf"]);
        assert!(scans[1].skills.is_empty() && scans[2].skills.is_empty());
        assert!(scans.iter().all(|s| s.readable));
    }

    #[test]
    fn shows_a_skill_whose_link_points_outside_every_root() {
        let t = tree();
        t.write("elsewhere/pdf/SKILL.md", &skill_text("pdf"));
        t.link("claude/pdf", "elsewhere/pdf");
        assert_eq!(folders(&scan(&[t.root("claude")])[0]), ["pdf"]);
    }

    #[test]
    fn reads_a_skill_md_that_is_itself_a_link() {
        let t = tree();
        t.write("store/real.md", &skill_text("linked"));
        t.link("r/a/SKILL.md", "store/real.md");
        assert_eq!(scan(&[t.root("r")])[0].skills[0].name, "linked");
    }

    #[test]
    fn skips_a_broken_link_and_a_link_to_a_file_and_keeps_scanning() {
        let t = tree();
        t.write("r/good/SKILL.md", &skill_text("good"));
        t.write("file.md", "x");
        symlink(t.path("missing"), {
            fs::create_dir_all(t.path("r")).unwrap();
            t.path("r/broken")
        })
        .unwrap();
        t.link("r/tofile", "file.md");
        assert_eq!(folders(&scan(&[t.root("r")])[0]), ["good"]);
    }

    #[test]
    fn lists_files_with_real_paths_and_relative_names_skill_md_first() {
        let t = tree();
        t.write("r/s/SKILL.md", &skill_text("s"));
        t.write("r/s/b.txt", "x");
        t.write("r/s/A.MD", "x");
        t.write("r/s/refs/deep.md", "x");
        t.write("r/s/.hidden", "x");
        t.write("r/s/node_modules/x.js", "x");
        let files = &scan(&[t.root("r")])[0].skills[0].files;
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["SKILL.md", "A.MD", "b.txt", "refs/deep.md"]);
        assert_eq!(files[0].path, t.root("r/s/SKILL.md"));
        assert!(files[1].is_markdown && !files[2].is_markdown);
    }

    #[test]
    fn stops_at_the_maximum_depth() {
        let t = tree();
        t.write("r/s/SKILL.md", &skill_text("s"));
        t.write("r/s/a/b/c/d/deep.md", "x");
        t.write("r/s/a/b/c/ok.md", "x");
        let names: Vec<String> = scan(&[t.root("r")])[0].skills[0]
            .files
            .iter()
            .map(|f| f.name.clone())
            .collect();
        assert_eq!(names, ["SKILL.md", "a/b/c/ok.md"]);
    }

    #[test]
    fn does_not_loop_on_a_link_that_points_back_up() {
        let t = tree();
        t.write("r/s/SKILL.md", &skill_text("s"));
        t.link("r/s/up", "r/s");
        let names: Vec<String> = scan(&[t.root("r")])[0].skills[0]
            .files
            .iter()
            .map(|f| f.name.clone())
            .collect();
        assert_eq!(names, ["SKILL.md"]);
    }

    #[test]
    fn follows_a_linked_folder_inside_the_skill_and_lists_a_file_once() {
        let t = tree();
        t.write("r/s/SKILL.md", &skill_text("s"));
        t.write("shared/ref.md", "x");
        t.link("r/s/shared", "shared");
        t.link("r/s/alias.md", "shared/ref.md");
        let names: Vec<String> = scan(&[t.root("r")])[0].skills[0]
            .files
            .iter()
            .map(|f| f.name.clone())
            .collect();
        assert_eq!(names, ["SKILL.md", "alias.md"]);
    }

    #[test]
    fn reports_a_missing_root_and_an_empty_root() {
        let t = tree();
        fs::create_dir_all(t.path("empty")).unwrap();
        let scans = scan(&[t.root("missing"), t.root("empty")]);
        assert!(!scans[0].readable);
        assert!(scans[1].readable && scans[1].skills.is_empty());
    }

    #[test]
    fn sorts_skills_by_name_ignoring_case_then_folder_and_keeps_the_root_order() {
        let t = tree();
        t.write("one/x/SKILL.md", &skill_text("Zebra"));
        t.write("one/y/SKILL.md", &skill_text("apple"));
        t.write("one/z/SKILL.md", &skill_text("apple"));
        t.write("two/q/SKILL.md", &skill_text("q"));
        let scans = scan(&[t.root("two"), t.root("one")]);
        assert_eq!(scans[0].root, t.root("two"));
        assert_eq!(folders(&scans[1]), ["y", "z", "x"]);
    }

    #[test]
    fn accepts_a_root_with_trailing_slashes_and_scans_nothing_for_no_roots() {
        let t = tree();
        t.write("r/s/SKILL.md", &skill_text("s"));
        let scans = scan(&[format!("{}///", t.root("r"))]);
        assert_eq!(folders(&scans[0]), ["s"]);
        assert!(scan(&[]).is_empty());
        let _ = Path::new("");
    }
}
