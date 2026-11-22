use desafinado_core::log_err::LogErr;
use std::fs;
use std::io;
use std::path::Path;

pub use desafinado_features::skills::library::CreateSkillResult;
use desafinado_features::skills::skill_frontmatter::check_skill_name;
pub use desafinado_features::skills::skill_template::{TEMPLATE_DESCRIPTION, skill_template};

use super::handlers::{LibraryDeps, is_listed_root};
use super::scan::SKILL_FILE_NAME;
use crate::files::{parse_path_request, write_atomic_unconditionally};
use crate::fs_failure::describe_failure;

const MAX_NAME_CHARS: usize = 200;

fn failed(reason: impl Into<String>) -> CreateSkillResult {
    CreateSkillResult::Failed {
        reason: reason.into(),
    }
}

fn make_folder(folder: &Path, name: &str) -> Option<CreateSkillResult> {
    match fs::create_dir(folder) {
        Ok(()) => None,
        Err(error) => Some(failed(match error.kind() {
            io::ErrorKind::AlreadyExists => {
                format!("A folder named \"{name}\" already exists there")
            }
            io::ErrorKind::NotFound => "Folder not found".to_owned(),
            _ => describe_failure(&error).to_owned(),
        })),
    }
}

pub fn create_skill(deps: &LibraryDeps, root: &str, name: &str) -> CreateSkillResult {
    let Some(root) = parse_path_request(root) else {
        return failed("Invalid request");
    };
    if name.is_empty() || name.chars().count() > MAX_NAME_CHARS || name.contains('\0') {
        return failed("Invalid request");
    }
    let name = name.trim();
    if let Some(problem) = check_skill_name(name, None).first() {
        return failed(problem.message.clone());
    }
    match is_listed_root(deps, &root) {
        None => return failed(format!("Could not read {}", super::roots::ROOTS_FILE_NAME)),
        Some(false) => return failed("Not a Library folder"),
        Some(true) => {}
    }
    let folder = Path::new(&root).join(name);
    if let Some(refused) = make_folder(&folder, name) {
        return refused;
    }
    let file = folder.join(SKILL_FILE_NAME);
    let text = match skill_template(name) {
        Ok(text) => text,
        Err(problem) => {
            fs::remove_dir(&folder).log_warn();
            return failed(problem.message);
        }
    };
    if let Err(error) = write_atomic_unconditionally(&file, &text) {
        fs::remove_dir(&folder).log_warn();
        return failed(describe_failure(&error));
    }
    CreateSkillResult::Created {
        path: fs::canonicalize(&file)
            .unwrap_or(file)
            .to_string_lossy()
            .into_owned(),
        name: name.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{RealFileSystem, RootsStore};
    use desafinado_features::skills::frontmatter_fields::{FrontmatterReading, read_frontmatter};
    use std::os::unix::fs::symlink;

    struct Env {
        _dir: tempfile::TempDir,
        base: std::path::PathBuf,
        home: String,
        store: RootsStore,
    }

    fn env() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();
        let home = base.join("home");
        fs::create_dir_all(home.join(".claude/skills")).unwrap();
        fs::create_dir_all(home.join(".agents/skills")).unwrap();
        fs::create_dir_all(base.join("data")).unwrap();
        Env {
            store: RootsStore::new(&base.join("data")),
            home: home.to_string_lossy().into_owned(),
            base,
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
        fn claude(&self) -> String {
            format!("{}/.claude/skills", self.home)
        }
    }

    #[test]
    fn creates_root_name_skill_md_from_the_template() {
        let e = env();
        let result = create_skill(&e.deps(), &e.claude(), "pdf-processing");
        let path = format!("{}/pdf-processing/SKILL.md", e.claude());
        assert_eq!(
            result,
            CreateSkillResult::Created {
                path: path.clone(),
                name: "pdf-processing".into()
            }
        );
        let text = fs::read_to_string(path).unwrap();
        assert!(
            text.starts_with(
                "---\nname: pdf-processing\ndescription: Describe what this skill does"
            )
        );
        assert!(text.contains("\n# Pdf processing\n"));
        assert!(text.ends_with("what to do in each.\n"));
    }

    #[test]
    fn writes_a_skill_md_whose_frontmatter_reads_back_and_passes_the_name_check() {
        let e = env();
        create_skill(&e.deps(), &e.claude(), "123");
        let text = fs::read_to_string(format!("{}/123/SKILL.md", e.claude())).unwrap();
        let FrontmatterReading::Present(fields) = read_frontmatter(&text) else {
            panic!("no frontmatter")
        };
        assert_eq!(fields.get("name").and_then(|v| v.as_str()), Some("123"));
        assert!(check_skill_name("123", None).is_empty());
    }

    #[test]
    fn creates_in_the_second_default_root_a_project_root_and_with_the_trimmed_name() {
        let e = env();
        let project = e.base.join("repo/skills");
        fs::create_dir_all(&project).unwrap();
        let project = project.to_string_lossy().into_owned();
        e.store.save(std::slice::from_ref(&project)).unwrap();
        assert!(matches!(
            create_skill(&e.deps(), &project, "  trimmed  "),
            CreateSkillResult::Created { name, .. } if name == "trimmed"
        ));
        assert!(Path::new(&project).join("trimmed/SKILL.md").is_file());
        let agents = format!("{}/.agents/skills", e.home);
        assert!(matches!(
            create_skill(&e.deps(), &agents, "organização"),
            CreateSkillResult::Created { .. }
        ));
    }

    #[test]
    fn refuses_a_name_that_breaks_a_rule_with_its_message() {
        let e = env();
        assert_eq!(
            create_skill(&e.deps(), &e.claude(), "Pdf"),
            failed("name must be lowercase")
        );
        assert!(matches!(
            create_skill(&e.deps(), &e.claude(), "a b"),
            CreateSkillResult::Failed { .. }
        ));
        assert_eq!(
            create_skill(&e.deps(), &e.claude(), ""),
            failed("Invalid request")
        );
    }

    #[test]
    fn refuses_a_name_that_is_taken_by_a_folder_a_file_or_a_broken_link() {
        let e = env();
        let claude = e.claude();
        fs::create_dir(format!("{claude}/dir")).unwrap();
        fs::write(format!("{claude}/file"), "x").unwrap();
        symlink("/nowhere", format!("{claude}/link")).unwrap();
        for name in ["dir", "file", "link"] {
            assert_eq!(
                create_skill(&e.deps(), &claude, name),
                failed(format!("A folder named \"{name}\" already exists there"))
            );
        }
        assert!(matches!(
            create_skill(&e.deps(), &claude, "free"),
            CreateSkillResult::Created { .. }
        ));
    }

    #[test]
    fn creates_once_when_requests_race_for_the_same_name() {
        let e = env();
        let results: Vec<CreateSkillResult> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..3)
                .map(|_| scope.spawn(|| create_skill(&e.deps(), &e.claude(), "race")))
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let created = results
            .iter()
            .filter(|r| matches!(r, CreateSkillResult::Created { .. }))
            .count();
        assert_eq!(created, 1);
    }

    #[test]
    fn refuses_a_folder_that_is_not_a_listed_root() {
        let e = env();
        let other = e.base.join("other");
        fs::create_dir_all(&other).unwrap();
        for root in [
            other.to_string_lossy().into_owned(),
            e.home.clone(),
            format!("{}/inside", e.claude()),
            format!("{}/", e.claude()),
            format!("{}/.", e.claude()),
        ] {
            assert_eq!(
                create_skill(&e.deps(), &root, "x"),
                failed("Not a Library folder"),
                "{root}"
            );
        }
    }

    #[test]
    fn refuses_a_project_root_that_was_removed_and_unreadable_root_lists() {
        let e = env();
        let project = e.base.join("repo");
        fs::create_dir_all(&project).unwrap();
        let project = project.to_string_lossy().into_owned();
        e.store.save(std::slice::from_ref(&project)).unwrap();
        e.store.save(&[]).unwrap();
        assert_eq!(
            create_skill(&e.deps(), &project, "x"),
            failed("Not a Library folder")
        );
        fs::write(e.base.join("data/library-roots.json"), "{bad").unwrap();
        assert_eq!(
            create_skill(&e.deps(), &e.claude(), "x"),
            failed("Could not read library-roots.json")
        );
    }

    #[test]
    fn answers_with_the_real_path_when_the_root_is_a_link() {
        let e = env();
        let real = e.base.join("real-skills");
        fs::create_dir_all(&real).unwrap();
        let link = format!("{}/proj", e.home);
        symlink(&real, &link).unwrap();
        e.store.save(std::slice::from_ref(&link)).unwrap();
        let CreateSkillResult::Created { path, .. } = create_skill(&e.deps(), &link, "linked")
        else {
            panic!("not created")
        };
        assert_eq!(path, real.join("linked/SKILL.md").to_string_lossy());
    }
}
