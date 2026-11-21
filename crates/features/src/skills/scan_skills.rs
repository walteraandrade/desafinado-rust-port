use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::frontmatter_fields::{FrontmatterProblem, FrontmatterReading, read_frontmatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub name: String,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsError {
    pub code: String,
    pub message: String,
}

impl FsError {
    pub fn new(code: &str, path: &str) -> Self {
        Self {
            code: code.to_string(),
            message: format!("{code}: {path}"),
        }
    }
}

impl std::fmt::Display for FsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for FsError {}

pub trait SkillFileSystem {
    fn list_directory(&self, path: &str) -> Result<Vec<DirectoryEntry>, FsError>;
    fn real_path(&self, path: &str) -> Result<String, FsError>;
    fn read_text(&self, path: &str) -> Result<String, FsError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFile {
    pub path: String,
    pub name: String,
    pub is_markdown: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootScan {
    pub root: String,
    pub readable: bool,
    pub skills: Vec<Skill>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ScanOptions {
    pub max_depth: Option<usize>,
}

pub const SKILL_FILE_NAME: &str = "SKILL.md";

pub fn is_skill_file(file_name: Option<&str>) -> bool {
    file_name == Some(SKILL_FILE_NAME)
}

pub const DEFAULT_MAX_DEPTH: usize = 3;

fn trailing_slash_start(text: &str) -> &str {
    text.trim_end_matches(['/', '\\'])
}

fn join_path(directory: &str, name: &str) -> String {
    format!("{}/{name}", trailing_slash_start(directory))
}

fn compare_ignoring_case(a: &str, b: &str) -> std::cmp::Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

fn by_name(a: &DirectoryEntry, b: &DirectoryEntry) -> std::cmp::Ordering {
    compare_ignoring_case(&a.name, &b.name)
}

fn is_hidden(entry: &DirectoryEntry) -> bool {
    entry.name.starts_with('.') || entry.name == "node_modules"
}

fn is_markdown(name: &str) -> bool {
    name.to_lowercase().ends_with(".md")
}

fn text_or_none(value: Option<&serde_json::Value>) -> Option<String> {
    match value {
        Some(serde_json::Value::String(text)) if !crate::text::js_trim(text).is_empty() => {
            Some(crate::text::js_trim(text).to_string())
        }
        _ => None,
    }
}

struct Described {
    name: String,
    description: Option<String>,
    problem: Option<FrontmatterProblem>,
}

fn describe_skill_file(text: &str, folder_name: &str) -> Described {
    let without_bom = text.strip_prefix('\u{feff}').unwrap_or(text);
    let normalized = without_bom.replace("\r\n", "\n").replace('\r', "\n");
    let reading = read_frontmatter(&normalized);
    let (fields, problem) = match reading {
        FrontmatterReading::Present(fields) => (fields, None),
        FrontmatterReading::Invalid(problem) => (serde_json::Map::new(), Some(problem)),
        FrontmatterReading::Absent => (serde_json::Map::new(), None),
    };
    Described {
        name: text_or_none(fields.get("name")).unwrap_or_else(|| folder_name.to_string()),
        description: text_or_none(fields.get("description")),
        problem,
    }
}

struct Visit<'a> {
    directory: &'a str,
    children: &'a [DirectoryEntry],
    prefix: &'a str,
    depth: usize,
    ancestors: &'a HashSet<String>,
}

fn visit_files(
    file_system: &dyn SkillFileSystem,
    max_depth: usize,
    visit: &Visit,
) -> Vec<SkillFile> {
    let mut entries: Vec<&DirectoryEntry> = visit
        .children
        .iter()
        .filter(|entry| !is_hidden(entry) && entry.kind != EntryKind::Other)
        .collect();
    entries.sort_by(|a, b| by_name(a, b));
    entries
        .into_iter()
        .flat_map(|entry| {
            let name = format!("{}{}", visit.prefix, entry.name);
            let Ok(real) = file_system.real_path(&join_path(visit.directory, &entry.name)) else {
                return Vec::new();
            };
            let file = SkillFile {
                path: real.clone(),
                is_markdown: is_markdown(&name),
                name: name.clone(),
            };
            if entry.kind == EntryKind::File {
                return vec![file];
            }
            if entry.kind == EntryKind::Directory && visit.depth >= max_depth {
                return Vec::new();
            }
            let Ok(nested) = file_system.list_directory(&real) else {
                return if entry.kind == EntryKind::Symlink {
                    vec![file]
                } else {
                    Vec::new()
                };
            };
            if visit.depth >= max_depth || visit.ancestors.contains(&real) {
                return Vec::new();
            }
            let mut ancestors = visit.ancestors.clone();
            ancestors.insert(real.clone());
            visit_files(
                file_system,
                max_depth,
                &Visit {
                    directory: &real,
                    children: &nested,
                    prefix: &format!("{name}/"),
                    depth: visit.depth + 1,
                    ancestors: &ancestors,
                },
            )
        })
        .collect()
}

fn order_files(files: Vec<SkillFile>) -> Vec<SkillFile> {
    let mut sorted = files;
    sorted.sort_by(|a, b| {
        (b.name == SKILL_FILE_NAME)
            .cmp(&(a.name == SKILL_FILE_NAME))
            .then_with(|| compare_ignoring_case(&a.name, &b.name))
    });
    let mut seen = HashSet::new();
    sorted
        .into_iter()
        .filter(|file| seen.insert(file.path.clone()))
        .collect()
}

fn load_skill(
    file_system: &dyn SkillFileSystem,
    max_depth: usize,
    root: &str,
    folder_name: &str,
    real_path: &str,
) -> Option<Skill> {
    let children = file_system.list_directory(real_path).ok()?;
    let has_skill_file = children.iter().any(|entry| {
        entry.name == SKILL_FILE_NAME && matches!(entry.kind, EntryKind::File | EntryKind::Symlink)
    });
    if !has_skill_file {
        return None;
    }
    let skill_file_path = file_system
        .real_path(&join_path(real_path, SKILL_FILE_NAME))
        .ok()?;
    let text = file_system.read_text(&skill_file_path).ok()?;
    let ancestors = HashSet::from([real_path.to_string()]);
    let files = visit_files(
        file_system,
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
        real_path: real_path.to_string(),
        folder_name: folder_name.to_string(),
        root: root.to_string(),
        name: described.name,
        description: described.description,
        problem: described.problem,
        files: order_files(files),
    })
}

fn compare_skills(a: &Skill, b: &Skill) -> std::cmp::Ordering {
    compare_ignoring_case(&a.name, &b.name)
        .then_with(|| compare_ignoring_case(&a.folder_name, &b.folder_name))
}

fn scan_root(
    file_system: &dyn SkillFileSystem,
    max_depth: usize,
    root: &str,
    claimed: &HashSet<String>,
) -> (RootScan, HashSet<String>) {
    let Ok(entries) = file_system.list_directory(root) else {
        return (
            RootScan {
                root: root.to_string(),
                readable: false,
                skills: Vec::new(),
            },
            claimed.clone(),
        );
    };
    let mut folders: Vec<&DirectoryEntry> = entries
        .iter()
        .filter(|entry| matches!(entry.kind, EntryKind::Directory | EntryKind::Symlink))
        .collect();
    folders.sort_by(|a, b| by_name(a, b));
    let mut seen = claimed.clone();
    let mut candidates: Vec<(String, String)> = Vec::new();
    for entry in folders {
        let Ok(real) = file_system.real_path(&join_path(root, &entry.name)) else {
            continue;
        };
        if seen.insert(real.clone()) {
            candidates.push((entry.name.clone(), real));
        }
    }
    let mut skills: Vec<Skill> = candidates
        .iter()
        .filter_map(|(folder_name, real)| {
            load_skill(file_system, max_depth, root, folder_name, real)
        })
        .collect();
    skills.sort_by(compare_skills);
    (
        RootScan {
            root: root.to_string(),
            readable: true,
            skills,
        },
        seen,
    )
}

pub fn scan_skill_roots(
    file_system: &dyn SkillFileSystem,
    roots: &[String],
    options: ScanOptions,
) -> Vec<RootScan> {
    let max_depth = options.max_depth.unwrap_or(DEFAULT_MAX_DEPTH);
    let mut claimed = HashSet::new();
    roots
        .iter()
        .map(|root| {
            let (scan, next) = scan_root(file_system, max_depth, root, &claimed);
            claimed = next;
            scan
        })
        .collect()
}
