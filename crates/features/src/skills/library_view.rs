use super::library::{LibraryRoot, LibrarySnapshot};
use super::scan_skills::Skill;

pub fn skill_count(roots: &[LibraryRoot]) -> usize {
    roots.iter().map(|root| root.skills.len()).sum()
}

pub fn skill_of_file<'a>(
    roots: &'a [LibraryRoot],
    is_open_file: &dyn Fn(&str) -> bool,
) -> Option<&'a Skill> {
    roots
        .iter()
        .flat_map(|root| root.skills.iter())
        .find(|skill| skill.files.iter().any(|file| is_open_file(&file.path)))
}

pub fn skill_file_label(
    roots: &[LibraryRoot],
    is_open_file: &dyn Fn(&str) -> bool,
) -> Option<String> {
    roots
        .iter()
        .flat_map(|root| root.skills.iter())
        .flat_map(|skill| {
            skill
                .files
                .iter()
                .filter(|file| is_open_file(&file.path))
                .map(move |file| format!("{} › {}", skill.name, file.name))
        })
        .next()
}

pub fn same_library(left: &LibrarySnapshot, right: &LibrarySnapshot) -> bool {
    left == right
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSkillFile {
    pub folder_name: String,
    pub file_name: String,
}

pub fn open_skill_file(
    roots: &[LibraryRoot],
    is_open_file: &dyn Fn(&str) -> bool,
) -> Option<OpenSkillFile> {
    roots
        .iter()
        .flat_map(|root| root.skills.iter())
        .flat_map(|skill| {
            skill
                .files
                .iter()
                .filter(|file| is_open_file(&file.path))
                .map(move |file| OpenSkillFile {
                    folder_name: skill.folder_name.clone(),
                    file_name: file.name.clone(),
                })
        })
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::scan_skills::SkillFile;

    fn skill(folder_name: &str) -> Skill {
        Skill {
            real_path: format!("/skills/{folder_name}"),
            folder_name: folder_name.to_string(),
            root: "/skills".to_string(),
            name: folder_name.to_string(),
            description: None,
            problem: None,
            files: vec![
                file(&format!("/skills/{folder_name}/SKILL.md"), "SKILL.md", true),
                file(&format!("/skills/{folder_name}/notes.md"), "notes.md", true),
                file(&format!("/skills/{folder_name}/run.sh"), "run.sh", false),
            ],
        }
    }

    fn file(path: &str, name: &str, is_markdown: bool) -> SkillFile {
        SkillFile {
            path: path.to_string(),
            name: name.to_string(),
            is_markdown,
        }
    }

    fn root(skills: Vec<Skill>) -> LibraryRoot {
        LibraryRoot {
            root: "/skills".to_string(),
            label: "/skills".to_string(),
            removable: false,
            readable: true,
            skills,
        }
    }

    fn listing() -> Vec<LibraryRoot> {
        let mut other = root(vec![skill("docs")]);
        other.root = "/other".to_string();
        vec![root(vec![skill("pdf"), skill("git")]), other]
    }

    fn opening(path: &'static str) -> impl Fn(&str) -> bool {
        move |candidate| candidate == path
    }

    #[test]
    fn skill_count_adds_the_skills_of_every_root() {
        assert_eq!(skill_count(&listing()), 3);
    }

    #[test]
    fn skill_count_is_zero_for_no_roots() {
        assert_eq!(skill_count(&[]), 0);
    }

    fn nested_listing() -> Vec<LibraryRoot> {
        let mut nested = skill("pdf-tools");
        nested.files = vec![
            file("/skills/pdf-tools/SKILL.md", "SKILL.md", true),
            file("/real/forms.md", "references/forms.md", true),
        ];
        vec![root(vec![skill("git"), nested])]
    }

    #[test]
    fn skill_file_label_is_the_skill_name_and_the_file_inside_the_skill() {
        assert_eq!(
            skill_file_label(&nested_listing(), &opening("/skills/pdf-tools/SKILL.md")),
            Some("pdf-tools › SKILL.md".to_string())
        );
    }

    #[test]
    fn skill_file_label_keeps_the_path_inside_the_skill_for_a_nested_file() {
        assert_eq!(
            skill_file_label(&nested_listing(), &opening("/real/forms.md")),
            Some("pdf-tools › references/forms.md".to_string())
        );
    }

    #[test]
    fn skill_file_label_uses_the_frontmatter_name_not_the_folder_name() {
        let mut renamed = skill("pdf");
        renamed.name = "pdf-processing".to_string();
        assert_eq!(
            skill_file_label(&[root(vec![renamed])], &opening("/skills/pdf/SKILL.md")),
            Some("pdf-processing › SKILL.md".to_string())
        );
    }

    #[test]
    fn skill_file_label_is_none_for_a_file_outside_the_library() {
        assert_eq!(
            skill_file_label(&nested_listing(), &opening("/tmp/other.md")),
            None
        );
    }

    #[test]
    fn skill_file_label_is_none_while_the_library_is_not_loaded() {
        assert_eq!(
            skill_file_label(&[], &opening("/skills/pdf-tools/SKILL.md")),
            None
        );
    }

    fn snapshot(skills: Vec<Skill>, problem: Option<&str>) -> LibrarySnapshot {
        LibrarySnapshot {
            roots: vec![root(skills)],
            problem: problem.map(str::to_string),
        }
    }

    #[test]
    fn same_library_is_true_for_the_same_listing_read_twice() {
        assert!(same_library(
            &snapshot(vec![skill("pdf")], None),
            &snapshot(vec![skill("pdf")], None)
        ));
    }

    #[test]
    fn same_library_is_false_when_a_skill_was_added_removed_or_renamed() {
        assert!(!same_library(
            &snapshot(vec![skill("pdf")], None),
            &snapshot(vec![], None)
        ));
        assert!(!same_library(
            &snapshot(vec![skill("pdf")], None),
            &snapshot(vec![skill("pdf"), skill("docx")], None)
        ));
        let mut renamed = skill("pdf");
        renamed.name = "pdf tools".to_string();
        assert!(!same_library(
            &snapshot(vec![skill("pdf")], None),
            &snapshot(vec![renamed], None)
        ));
    }

    #[test]
    fn same_library_is_false_when_only_a_file_of_a_skill_changed() {
        let mut emptied = skill("pdf");
        emptied.files = vec![];
        assert!(!same_library(
            &snapshot(vec![skill("pdf")], None),
            &snapshot(vec![emptied], None)
        ));
    }

    #[test]
    fn same_library_is_false_when_the_problem_with_the_root_list_changed() {
        assert!(!same_library(
            &snapshot(vec![skill("pdf")], None),
            &snapshot(
                vec![skill("pdf")],
                Some("Could not read library-roots.json")
            )
        ));
    }

    fn linked_listing() -> Vec<LibraryRoot> {
        let mut linked = skill("pdf-link");
        linked.real_path = "/real/pdf-processing".to_string();
        linked.files = vec![
            file("/real/pdf-processing/README.md", "SKILL.md", true),
            file("/real/pdf-processing/forms.md", "references/forms.md", true),
        ];
        vec![root(vec![skill("git"), linked])]
    }

    #[test]
    fn open_skill_file_gives_the_folder_name_the_root_lists_and_the_name_inside_the_skill() {
        assert_eq!(
            open_skill_file(
                &linked_listing(),
                &opening("/real/pdf-processing/README.md")
            ),
            Some(OpenSkillFile {
                folder_name: "pdf-link".to_string(),
                file_name: "SKILL.md".to_string()
            })
        );
    }

    #[test]
    fn open_skill_file_also_finds_a_nested_file_with_its_path_inside_the_skill() {
        assert_eq!(
            open_skill_file(&linked_listing(), &opening("/real/pdf-processing/forms.md")),
            Some(OpenSkillFile {
                folder_name: "pdf-link".to_string(),
                file_name: "references/forms.md".to_string()
            })
        );
    }

    #[test]
    fn open_skill_file_is_none_for_a_file_outside_the_library() {
        assert_eq!(
            open_skill_file(&linked_listing(), &opening("/tmp/x.md")),
            None
        );
        assert_eq!(open_skill_file(&[], &opening("/tmp/x.md")), None);
    }

    #[test]
    fn skill_of_file_returns_the_whole_skill_that_holds_the_open_file() {
        assert_eq!(
            skill_of_file(&listing(), &opening("/skills/git/notes.md"))
                .map(|skill| skill.name.as_str()),
            Some("git")
        );
    }

    #[test]
    fn skill_of_file_returns_nothing_for_a_file_outside_the_library() {
        assert_eq!(skill_of_file(&listing(), &|_| false), None);
    }
}
