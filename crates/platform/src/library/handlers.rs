use serde::Serialize;

use super::roots::{
    ROOTS_FILE_NAME, RootsLoad, RootsStore, default_roots, display_path, with_root, without_root,
};
use super::scan::{DEFAULT_MAX_DEPTH, RootScan, SkillFileSystem, scan_skill_roots};
use crate::files::parse_path_request;

pub struct LibraryDeps<'a> {
    pub home: &'a str,
    pub store: &'a RootsStore,
    pub file_system: &'a dyn SkillFileSystem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibraryRoot {
    #[serde(flatten)]
    pub scan: RootScan,
    pub label: String,
    pub removable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LibrarySnapshot {
    pub roots: Vec<LibraryRoot>,
    pub problem: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AddRootResult {
    Added { label: String },
    Duplicate { label: String },
    Cancelled,
    Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RemoveRootResult {
    Removed,
    Failed { reason: String },
}

fn roots_problem() -> String {
    format!("Could not read {ROOTS_FILE_NAME}")
}

pub fn list_library(deps: &LibraryDeps) -> LibrarySnapshot {
    let defaults = default_roots(deps.home);
    let loaded = deps.store.load();
    let (project_roots, problem) = match loaded {
        RootsLoad::Loaded(roots) => (roots, None),
        RootsLoad::Unreadable => (Vec::new(), Some(roots_problem())),
    };
    let all: Vec<String> = defaults
        .iter()
        .chain(project_roots.iter())
        .cloned()
        .collect();
    let scans = scan_skill_roots(deps.file_system, &all, DEFAULT_MAX_DEPTH);
    LibrarySnapshot {
        roots: scans
            .into_iter()
            .map(|scan| LibraryRoot {
                label: display_path(&scan.root, deps.home),
                removable: !defaults.contains(&scan.root),
                scan,
            })
            .collect(),
        problem,
    }
}

fn real_path_or_self(fs: &dyn SkillFileSystem, path: &str) -> String {
    fs.real_path(path).unwrap_or_else(|_| path.to_owned())
}

pub fn add_library_root(deps: &LibraryDeps, pick: &dyn Fn() -> Option<String>) -> AddRootResult {
    let unreadable = || AddRootResult::Failed {
        reason: roots_problem(),
    };
    if deps.store.load() == RootsLoad::Unreadable {
        return unreadable();
    }
    let Some(picked) = pick() else {
        return AddRootResult::Cancelled;
    };
    let Some(chosen) = parse_path_request(&picked) else {
        return AddRootResult::Failed {
            reason: "Invalid request".into(),
        };
    };
    let real = match deps.file_system.real_path(&chosen) {
        Ok(real) if deps.file_system.list_directory(&real).is_ok() => real,
        _ => {
            return AddRootResult::Failed {
                reason: "Not a readable folder".into(),
            };
        }
    };
    let label = display_path(&real, deps.home);
    let project_roots = match deps.store.load() {
        RootsLoad::Unreadable => return unreadable(),
        RootsLoad::Loaded(roots) => roots,
    };
    let listed: Vec<String> = default_roots(deps.home)
        .iter()
        .chain(project_roots.iter())
        .map(|root| real_path_or_self(deps.file_system, root))
        .collect();
    if listed.contains(&real) {
        return AddRootResult::Duplicate { label };
    }
    match deps.store.save(&with_root(&project_roots, &real)) {
        Ok(()) => AddRootResult::Added { label },
        Err(_) => AddRootResult::Failed {
            reason: "Could not keep the roots".into(),
        },
    }
}

pub fn remove_library_root(deps: &LibraryDeps, path: &str) -> RemoveRootResult {
    let failed = |reason: &str| RemoveRootResult::Failed {
        reason: reason.to_owned(),
    };
    let Some(path) = parse_path_request(path) else {
        return failed("Invalid request");
    };
    let project_roots = match deps.store.load() {
        RootsLoad::Unreadable => return failed(&roots_problem()),
        RootsLoad::Loaded(roots) => roots,
    };
    if !project_roots.contains(&path) {
        return failed("Not a project root");
    }
    match deps.store.save(&without_root(&project_roots, &path)) {
        Ok(()) => RemoveRootResult::Removed,
        Err(_) => failed("Could not keep the roots"),
    }
}

pub fn is_listed_root(deps: &LibraryDeps, root: &str) -> Option<bool> {
    match deps.store.load() {
        RootsLoad::Unreadable => None,
        RootsLoad::Loaded(roots) => Some(
            default_roots(deps.home)
                .iter()
                .chain(roots.iter())
                .any(|candidate| candidate == root),
        ),
    }
}

pub fn skill_real_paths(snapshot: &LibrarySnapshot) -> Vec<&str> {
    snapshot
        .roots
        .iter()
        .flat_map(|root| {
            root.scan
                .skills
                .iter()
                .map(|skill| skill.real_path.as_str())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::RealFileSystem;
    use std::fs;
    use std::os::unix::fs::symlink;

    struct Env {
        _dir: tempfile::TempDir,
        base: std::path::PathBuf,
        store: RootsStore,
        home: String,
    }

    fn env() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();
        let home = base.join("home");
        fs::create_dir_all(home.join(".claude/skills")).unwrap();
        fs::create_dir_all(home.join(".agents/skills")).unwrap();
        fs::create_dir_all(base.join("data")).unwrap();
        let store = RootsStore::new(&base.join("data"));
        Env {
            home: home.to_string_lossy().into_owned(),
            base,
            store,
            _dir: dir,
        }
    }

    impl Env {
        fn deps(&self) -> LibraryDeps<'_> {
            LibraryDeps {
                home: &self.home,
                store: &self.store,
                file_system: &RealFileSystem,
            }
        }
        fn skill(&self, relative: &str, name: &str) {
            let path = self.base.join(relative).join("SKILL.md");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, format!("---\nname: {name}\ndescription: d\n---\n")).unwrap();
        }
        fn dir(&self, relative: &str) -> String {
            let path = self.base.join(relative);
            fs::create_dir_all(&path).unwrap();
            path.to_string_lossy().into_owned()
        }
    }

    #[test]
    fn lists_the_default_roots_first_then_the_project_roots_with_labels() {
        let e = env();
        let project = e.dir("repo/skills");
        e.store.save(std::slice::from_ref(&project)).unwrap();
        let snapshot = list_library(&e.deps());
        let labels: Vec<&str> = snapshot.roots.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels[..2], ["~/.claude/skills", "~/.agents/skills"]);
        assert_eq!(snapshot.roots[2].scan.root, project);
        let removable: Vec<bool> = snapshot.roots.iter().map(|r| r.removable).collect();
        assert_eq!(removable, [false, false, true]);
        assert_eq!(snapshot.problem, None);
    }

    #[test]
    fn shows_a_skill_once_under_the_first_root_that_reaches_it() {
        let e = env();
        e.skill("home/.agents/skills/pdf", "pdf");
        symlink(
            e.base.join("home/.agents/skills/pdf"),
            e.base.join("home/.claude/skills/pdf"),
        )
        .unwrap();
        let snapshot = list_library(&e.deps());
        assert_eq!(snapshot.roots[0].scan.skills.len(), 1);
        assert!(snapshot.roots[1].scan.skills.is_empty());
    }

    #[test]
    fn keeps_an_unreadable_root_in_the_list_marked() {
        let e = env();
        e.store.save(&["/definitely/missing".to_owned()]).unwrap();
        let snapshot = list_library(&e.deps());
        assert!(!snapshot.roots[2].scan.readable);
    }

    #[test]
    fn still_lists_the_default_roots_and_says_what_is_wrong_when_the_roots_are_unreadable() {
        let e = env();
        fs::write(e.base.join("data").join(ROOTS_FILE_NAME), "{bad").unwrap();
        let snapshot = list_library(&e.deps());
        assert_eq!(snapshot.roots.len(), 2);
        assert_eq!(
            snapshot.problem.as_deref(),
            Some("Could not read library-roots.json")
        );
    }

    #[test]
    fn adds_the_folder_the_dialog_returns_and_keeps_it() {
        let e = env();
        let folder = e.dir("code/proj/skills");
        let result = add_library_root(&e.deps(), &|| Some(folder.clone()));
        assert_eq!(
            result,
            AddRootResult::Added {
                label: folder.clone()
            }
        );
        assert_eq!(e.store.load(), RootsLoad::Loaded(vec![folder]));
    }

    #[test]
    fn keeps_the_real_path_of_a_folder_reached_through_a_link() {
        let e = env();
        let real = e.dir("code/real");
        let link = e.base.join("link");
        symlink(&real, &link).unwrap();
        add_library_root(&e.deps(), &|| Some(link.to_string_lossy().into_owned()));
        assert_eq!(e.store.load(), RootsLoad::Loaded(vec![real]));
    }

    #[test]
    fn is_cancelled_when_the_dialog_is() {
        let e = env();
        assert_eq!(
            add_library_root(&e.deps(), &|| None),
            AddRootResult::Cancelled
        );
        assert_eq!(e.store.load(), RootsLoad::Loaded(Vec::new()));
    }

    #[test]
    fn does_not_add_a_default_root_or_a_root_twice_even_through_a_link() {
        let e = env();
        let default = format!("{}/.claude/skills", e.home);
        assert!(matches!(
            add_library_root(&e.deps(), &|| Some(default.clone())),
            AddRootResult::Duplicate { .. }
        ));
        let folder = e.dir("code/p");
        add_library_root(&e.deps(), &|| Some(folder.clone()));
        let link = e.base.join("alias");
        symlink(&folder, &link).unwrap();
        let again = add_library_root(&e.deps(), &|| Some(link.to_string_lossy().into_owned()));
        assert!(matches!(again, AddRootResult::Duplicate { .. }));
        assert_eq!(e.store.load(), RootsLoad::Loaded(vec![folder]));
    }

    #[test]
    fn fails_for_a_path_that_is_not_a_readable_folder_or_not_absolute() {
        let e = env();
        assert_eq!(
            add_library_root(&e.deps(), &|| Some("/nope/missing".into())),
            AddRootResult::Failed {
                reason: "Not a readable folder".into()
            }
        );
        assert_eq!(
            add_library_root(&e.deps(), &|| Some("rel".into())),
            AddRootResult::Failed {
                reason: "Invalid request".into()
            }
        );
    }

    #[test]
    fn refuses_before_asking_for_a_folder_when_the_roots_are_unreadable() {
        let e = env();
        let file = e.base.join("data").join(ROOTS_FILE_NAME);
        fs::write(&file, "{bad").unwrap();
        let asked = std::cell::Cell::new(false);
        let result = add_library_root(&e.deps(), &|| {
            asked.set(true);
            None
        });
        assert_eq!(
            result,
            AddRootResult::Failed {
                reason: "Could not read library-roots.json".into()
            }
        );
        assert!(!asked.get());
        assert_eq!(fs::read_to_string(file).unwrap(), "{bad");
    }

    #[test]
    fn removes_a_project_root_and_refuses_a_default_one() {
        let e = env();
        let folder = e.dir("code/p");
        e.store.save(std::slice::from_ref(&folder)).unwrap();
        assert_eq!(
            remove_library_root(&e.deps(), &folder),
            RemoveRootResult::Removed
        );
        assert_eq!(e.store.load(), RootsLoad::Loaded(Vec::new()));
        let default = format!("{}/.claude/skills", e.home);
        assert_eq!(
            remove_library_root(&e.deps(), &default),
            RemoveRootResult::Failed {
                reason: "Not a project root".into()
            }
        );
    }

    #[test]
    fn refuses_to_remove_while_the_root_list_is_unreadable() {
        let e = env();
        fs::write(e.base.join("data").join(ROOTS_FILE_NAME), "{bad").unwrap();
        assert!(matches!(
            remove_library_root(&e.deps(), "/a"),
            RemoveRootResult::Failed { .. }
        ));
    }
}
