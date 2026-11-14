use std::fs;
use std::io;
use std::path::Path;

pub use desafinado_features::skills::scan_skills::{
    DEFAULT_MAX_DEPTH, DirectoryEntry, EntryKind, FsError, RootScan, SKILL_FILE_NAME, ScanOptions,
    Skill, SkillFile, SkillFileSystem, scan_skill_roots,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct RealFileSystem;

fn fs_error(error: &io::Error, path: &str) -> FsError {
    let code = match error.raw_os_error() {
        Some(libc::ENOENT) => "ENOENT",
        Some(libc::EACCES) => "EACCES",
        Some(libc::ENOTDIR) => "ENOTDIR",
        Some(libc::ELOOP) => "ELOOP",
        Some(libc::EPERM) => "EPERM",
        _ => "EIO",
    };
    FsError::new(code, path)
}

impl SkillFileSystem for RealFileSystem {
    fn list_directory(&self, path: &str) -> Result<Vec<DirectoryEntry>, FsError> {
        let read = || -> io::Result<Vec<DirectoryEntry>> {
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
        };
        read().map_err(|error| fs_error(&error, path))
    }

    fn real_path(&self, path: &str) -> Result<String, FsError> {
        fs::canonicalize(Path::new(path))
            .map(|real| real.to_string_lossy().into_owned())
            .map_err(|error| fs_error(&error, path))
    }

    fn read_text(&self, path: &str) -> Result<String, FsError> {
        fs::read_to_string(path).map_err(|error| fs_error(&error, path))
    }
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
        scan_skill_roots(&RealFileSystem, roots, ScanOptions::default())
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
        assert_eq!(skill.problem.as_ref().unwrap().rule.as_str(), "yaml-syntax-error");
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
