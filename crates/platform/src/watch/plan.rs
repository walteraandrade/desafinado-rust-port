use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use crate::library::Skill;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchDiff {
    pub open: Vec<String>,
    pub close: Vec<String>,
}

fn dirname(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| ".".to_owned())
}

fn folders_of(skill: &Skill) -> Vec<String> {
    let mut folders = vec![skill.real_path.clone(), dirname(&skill.real_path)];
    folders.extend(skill.files.iter().map(|file| dirname(&file.path)));
    folders
}

pub fn watch_directories(
    roots: &[String],
    skills: &[&Skill],
    open_file: Option<&str>,
    folders: &[String],
    anchors: &[String],
) -> Vec<String> {
    let mut all: BTreeSet<String> = BTreeSet::new();
    all.extend(roots.iter().cloned());
    for folder in folders {
        all.insert(folder.clone());
        all.insert(dirname(folder));
    }
    all.extend(anchors.iter().cloned());
    for skill in skills {
        all.extend(folders_of(skill));
    }
    if let Some(open) = open_file {
        all.insert(dirname(open));
    }
    all.into_iter().collect()
}

pub fn diff_watches(
    active: &HashMap<String, String>,
    wanted: &HashMap<String, String>,
) -> WatchDiff {
    let mut open: Vec<String> = wanted
        .iter()
        .filter(|(dir, identity)| active.get(*dir) != Some(*identity))
        .map(|(dir, _)| dir.clone())
        .collect();
    let mut close: Vec<String> = active
        .iter()
        .filter(|(dir, identity)| wanted.get(*dir) != Some(*identity))
        .map(|(dir, _)| dir.clone())
        .collect();
    open.sort();
    close.sort();
    WatchDiff { open, close }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::SkillFile;

    fn skill(real: &str, files: &[&str]) -> Skill {
        Skill {
            real_path: real.into(),
            folder_name: "x".into(),
            root: "/r".into(),
            name: "x".into(),
            description: None,
            problem: None,
            files: files
                .iter()
                .map(|f| SkillFile {
                    path: (*f).into(),
                    name: "n".into(),
                    is_markdown: true,
                })
                .collect(),
        }
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn watches_roots_skill_folders_their_parents_and_file_folders_sorted_and_unique() {
        let s = skill("/r/pdf", &["/r/pdf/SKILL.md", "/r/pdf/references/g.md"]);
        assert_eq!(
            watch_directories(&strings(&["/r"]), &[&s], None, &[], &[]),
            ["/r", "/r/pdf", "/r/pdf/references"]
        );
    }

    #[test]
    fn watches_the_folder_of_the_open_file_the_anchors_and_linked_folders() {
        let s = skill("/r/pdf", &["/r/pdf/SKILL.md"]);
        let dirs = watch_directories(
            &strings(&["/r"]),
            &[&s],
            Some("/elsewhere/notes/a.md"),
            &strings(&["/lib/shared"]),
            &strings(&["/home/u"]),
        );
        assert_eq!(
            dirs,
            [
                "/elsewhere/notes",
                "/home/u",
                "/lib",
                "/lib/shared",
                "/r",
                "/r/pdf"
            ]
        );
    }

    #[test]
    fn diffs_by_directory_and_identity() {
        let active: HashMap<String, String> = [
            ("/a".into(), "1".into()),
            ("/b".into(), "1".into()),
            ("/c".into(), "1".into()),
        ]
        .into();
        let wanted: HashMap<String, String> = [
            ("/a".into(), "1".into()),
            ("/b".into(), "2".into()),
            ("/d".into(), "1".into()),
        ]
        .into();
        assert_eq!(
            diff_watches(&active, &wanted),
            WatchDiff {
                open: strings(&["/b", "/d"]),
                close: strings(&["/b", "/c"])
            }
        );
    }
}
