use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::library::SKILL_FILE_NAME;

pub const STAGE_EXCLUDED: [&str; 7] = [
    "hooks",
    ".claude-plugin",
    ".mcp.json",
    ".lsp.json",
    "settings.json",
    "agents",
    "commands",
];

static COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct Stage {
    pub dir: PathBuf,
    pub skill_text: String,
    pub case_count: usize,
    pub left_out: Vec<String>,
    container: PathBuf,
}

impl Stage {
    pub fn cleanup(&self) -> io::Result<()> {
        match fs::remove_dir_all(&self.container) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}

fn copy_entry(
    real_root: &Path,
    source: &Path,
    target: &Path,
    label: &str,
    visited: &HashSet<PathBuf>,
) -> io::Result<Vec<String>> {
    let Ok(info) = fs::symlink_metadata(source) else {
        return Ok(Vec::new());
    };
    if info.file_type().is_symlink() {
        return Ok(match fs::canonicalize(source) {
            Ok(real) if real.starts_with(real_root) => {
                copy_entry(real_root, &real, target, label, visited)?
            }
            _ => vec![label.to_owned()],
        });
    }
    if info.is_file() {
        fs::copy(source, target)?;
        return Ok(Vec::new());
    }
    if !info.is_dir() {
        return Ok(vec![label.to_owned()]);
    }
    let real = fs::canonicalize(source)?;
    if visited.contains(&real) {
        return Ok(vec![label.to_owned()]);
    }
    fs::create_dir(target)?;
    let mut next = visited.clone();
    next.insert(real);
    let mut left_out = Vec::new();
    for entry in fs::read_dir(source)? {
        let name = entry?.file_name();
        let name_text = name.to_string_lossy();
        left_out.extend(copy_entry(
            real_root,
            &source.join(&name),
            &target.join(&name),
            &format!("{label}/{name_text}"),
            &next,
        )?);
    }
    Ok(left_out)
}

pub fn count_evals(skill_dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(skill_dir.join("evals")) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            fs::symlink_metadata(entry.path().join("prompt.md")).is_ok_and(|info| info.is_file())
        })
        .count()
}

fn container_in(temp_base: &Path) -> io::Result<PathBuf> {
    loop {
        let unique = format!(
            "desafinado-eval-{:x}{:x}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let container = temp_base.join(unique);
        match fs::create_dir(&container) {
            Ok(()) => return Ok(container),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

pub fn stage_skill(
    skill_path: &Path,
    temp_base: &Path,
    skill_text_override: Option<&str>,
) -> io::Result<Stage> {
    let real_root = fs::canonicalize(skill_path)?;
    let container = container_in(temp_base)?;
    let built = build(&real_root, &container, skill_text_override);
    if built.is_err() {
        let _ = fs::remove_dir_all(&container);
    }
    built
}

fn build(
    real_root: &Path,
    container: &Path,
    skill_text_override: Option<&str>,
) -> io::Result<Stage> {
    let base = real_root.file_name().map(PathBuf::from).unwrap_or_default();
    let dir = container.join(base);
    fs::create_dir(&dir)?;
    let mut present = Vec::new();
    let mut skipped = Vec::new();
    let visited = HashSet::from([real_root.to_path_buf()]);
    for entry in fs::read_dir(real_root)? {
        let name = entry?.file_name();
        let name_text = name.to_string_lossy().into_owned();
        if STAGE_EXCLUDED.contains(&name_text.as_str()) {
            present.push(name_text);
            continue;
        }
        skipped.extend(copy_entry(
            real_root,
            &real_root.join(&name),
            &dir.join(&name),
            &name_text,
            &visited,
        )?);
    }
    if let Some(text) = skill_text_override {
        let skill_file = dir.join(SKILL_FILE_NAME);
        let _ = fs::remove_file(&skill_file);
        fs::write(&skill_file, text)?;
    }
    let skill_text = fs::read_to_string(dir.join(SKILL_FILE_NAME))?;
    let mut left_out: Vec<String> = present.into_iter().chain(skipped).collect();
    left_out.sort();
    Ok(Stage {
        case_count: count_evals(&dir),
        dir,
        skill_text,
        left_out,
        container: container.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    struct Env {
        _dir: tempfile::TempDir,
        base: PathBuf,
        skill: PathBuf,
        temp: PathBuf,
    }

    fn env() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();
        let skill = base.join("skills/pdf");
        let temp = base.join("tmp");
        fs::create_dir_all(&skill).unwrap();
        fs::create_dir_all(&temp).unwrap();
        fs::write(skill.join("SKILL.md"), "---\nname: pdf\n---\n# today\n").unwrap();
        Env {
            _dir: dir,
            base,
            skill,
            temp,
        }
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn copies_the_skill_and_counts_the_cases() {
        let e = env();
        write(&e.skill.join("references/guide.md"), "guide");
        write(&e.skill.join("evals/one/prompt.md"), "p");
        write(&e.skill.join("evals/two/prompt.md"), "p");
        write(&e.skill.join("evals/notes/readme.md"), "n");
        let stage = stage_skill(&e.skill, &e.temp, None).unwrap();
        assert_eq!(stage.dir.file_name().unwrap(), "pdf");
        assert!(stage.dir.starts_with(&e.temp));
        assert_eq!(
            fs::read_to_string(stage.dir.join("references/guide.md")).unwrap(),
            "guide"
        );
        assert_eq!(stage.case_count, 2);
        assert!(stage.skill_text.contains("# today"));
        assert!(stage.left_out.is_empty());
        stage.cleanup().unwrap();
    }

    #[test]
    fn leaves_out_the_folders_and_files_that_load_code() {
        let e = env();
        for name in [
            "hooks/h.sh",
            ".claude-plugin/p.json",
            ".mcp.json",
            ".lsp.json",
            "settings.json",
            "agents/a.md",
            "commands/c.md",
        ] {
            write(&e.skill.join(name), "x");
        }
        write(&e.skill.join("keep.md"), "k");
        let stage = stage_skill(&e.skill, &e.temp, None).unwrap();
        let mut copied: Vec<String> = fs::read_dir(&stage.dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        copied.sort();
        assert_eq!(copied, ["SKILL.md", "keep.md"]);
        assert_eq!(
            stage.left_out,
            [
                ".claude-plugin",
                ".lsp.json",
                ".mcp.json",
                "agents",
                "commands",
                "hooks",
                "settings.json"
            ]
        );
    }

    #[test]
    fn follows_no_link_out_of_the_folder() {
        let e = env();
        write(&e.base.join("outside/secret.md"), "secret");
        symlink(e.base.join("outside/secret.md"), e.skill.join("link.md")).unwrap();
        symlink(e.base.join("outside"), e.skill.join("linked-dir")).unwrap();
        write(&e.skill.join("sub/ok.md"), "ok");
        symlink(
            e.base.join("outside/secret.md"),
            e.skill.join("sub/deep.md"),
        )
        .unwrap();
        let stage = stage_skill(&e.skill, &e.temp, None).unwrap();
        assert!(!stage.dir.join("link.md").exists());
        assert!(!stage.dir.join("linked-dir").exists());
        assert!(!stage.dir.join("sub/deep.md").exists());
        assert!(stage.dir.join("sub/ok.md").exists());
        assert_eq!(stage.left_out, ["link.md", "linked-dir", "sub/deep.md"]);
    }

    #[test]
    fn copies_a_link_that_stays_inside_and_survives_a_loop() {
        let e = env();
        write(&e.skill.join("shared/data.md"), "data");
        symlink(e.skill.join("shared"), e.skill.join("alias")).unwrap();
        symlink(&e.skill, e.skill.join("shared/back")).unwrap();
        let stage = stage_skill(&e.skill, &e.temp, None).unwrap();
        assert_eq!(
            fs::read_to_string(stage.dir.join("alias/data.md")).unwrap(),
            "data"
        );
        assert!(
            !fs::symlink_metadata(stage.dir.join("alias"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(stage.left_out.iter().any(|label| label.ends_with("back")));
    }

    #[test]
    fn replaces_skill_md_in_the_copy_and_never_touches_the_real_folder() {
        let e = env();
        write(&e.skill.join("other.md"), "kept");
        let stage = stage_skill(&e.skill, &e.temp, Some("# older\n")).unwrap();
        assert_eq!(stage.skill_text, "# older\n");
        assert_eq!(
            fs::read_to_string(stage.dir.join("other.md")).unwrap(),
            "kept"
        );
        assert!(
            fs::read_to_string(e.skill.join("SKILL.md"))
                .unwrap()
                .contains("# today")
        );
    }

    #[test]
    fn does_not_write_through_a_skill_md_that_links_to_a_file_outside() {
        let e = env();
        let outside = e.base.join("outside.md");
        fs::write(&outside, "original").unwrap();
        fs::remove_file(e.skill.join("SKILL.md")).unwrap();
        symlink(&outside, e.skill.join("SKILL.md")).unwrap();
        let result = stage_skill(&e.skill, &e.temp, Some("# older\n"));
        assert!(result.is_ok());
        assert_eq!(fs::read_to_string(&outside).unwrap(), "original");
    }

    #[test]
    fn cleanup_deletes_the_copy_and_never_touches_the_real_folder() {
        let e = env();
        let stage = stage_skill(&e.skill, &e.temp, None).unwrap();
        let dir = stage.dir.clone();
        stage.cleanup().unwrap();
        stage.cleanup().unwrap();
        assert!(!dir.exists());
        assert!(e.skill.join("SKILL.md").exists());
        assert_eq!(fs::read_dir(&e.temp).unwrap().count(), 0);
    }

    #[test]
    fn cleans_up_when_the_copy_fails() {
        let e = env();
        fs::remove_file(e.skill.join("SKILL.md")).unwrap();
        assert!(stage_skill(&e.skill, &e.temp, None).is_err());
        assert_eq!(fs::read_dir(&e.temp).unwrap().count(), 0);
    }

    #[test]
    fn counts_zero_without_an_evals_folder() {
        let e = env();
        assert_eq!(count_evals(&e.skill), 0);
    }
}
