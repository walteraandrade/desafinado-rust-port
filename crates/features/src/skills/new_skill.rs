use serde::{Deserialize, Serialize};

use super::frontmatter_panel::{PanelRule, announce_rule};
use super::library::{CreateSkillRequest, CreateSkillResult, LibraryRoot};
use super::scan_skills::SKILL_FILE_NAME;
use super::skill_frontmatter::check_skill_name;
use crate::text::js_trim;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum NewSkillOutcome {
    Created { path: String, name: String },
    Failed { reason: String },
    Cancelled,
}

impl From<CreateSkillResult> for NewSkillOutcome {
    fn from(result: CreateSkillResult) -> Self {
        match result {
            CreateSkillResult::Created { path, name } => Self::Created { path, name },
            CreateSkillResult::Failed { reason } => Self::Failed { reason },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewSkillDraft {
    pub problems: Vec<String>,
    pub announcement: String,
    pub ready: bool,
    pub location: Option<String>,
}

pub fn writable_roots(roots: &[LibraryRoot]) -> Vec<&LibraryRoot> {
    roots.iter().filter(|root| root.readable).collect()
}

pub fn first_writable_root(roots: &[LibraryRoot]) -> Option<&LibraryRoot> {
    roots.iter().find(|root| root.readable)
}

pub fn new_skill_draft(name: &str, root: Option<&LibraryRoot>) -> NewSkillDraft {
    let trimmed = js_trim(name);
    if trimmed.is_empty() {
        return NewSkillDraft {
            problems: Vec::new(),
            announcement: String::new(),
            ready: false,
            location: None,
        };
    }
    let taken = root.is_some_and(|found| {
        found
            .skills
            .iter()
            .any(|skill| skill.folder_name == trimmed)
    });
    let rules = check_skill_name(trimmed, None);
    let mut problems: Vec<String> = rules.iter().map(|rule| rule.message.clone()).collect();
    if let (true, Some(found)) = (taken, root) {
        problems.push(format!(
            "A skill folder named \"{trimmed}\" already exists in {}",
            found.label
        ));
    }
    let mut phrases: Vec<&str> = rules
        .iter()
        .map(|rule| announce_rule(PanelRule::Skill(rule.rule)))
        .collect();
    if taken {
        phrases.push("a skill folder with this name already exists");
    }
    NewSkillDraft {
        ready: problems.is_empty() && root.is_some(),
        announcement: if phrases.is_empty() {
            String::new()
        } else {
            format!("Name problems: {}", phrases.join("; "))
        },
        location: root.map(|found| format!("{}/{trimmed}/{SKILL_FILE_NAME}", found.label)),
        problems,
    }
}

pub fn new_skill_request(name: &str, root: &LibraryRoot) -> CreateSkillRequest {
    CreateSkillRequest {
        root: root.root.clone(),
        name: js_trim(name).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::scan_skills::Skill;

    fn skill(folder_name: &str) -> Skill {
        Skill {
            real_path: format!("/skills/{folder_name}"),
            folder_name: folder_name.to_string(),
            root: "/skills".to_string(),
            name: folder_name.to_string(),
            description: None,
            problem: None,
            files: vec![],
        }
    }

    fn root() -> LibraryRoot {
        LibraryRoot {
            root: "/home/u/.claude/skills".to_string(),
            label: "~/.claude/skills".to_string(),
            removable: false,
            readable: true,
            skills: vec![skill("pdf")],
        }
    }

    fn root_at(path: &str, readable: bool) -> LibraryRoot {
        LibraryRoot {
            root: path.to_string(),
            readable,
            ..root()
        }
    }

    #[test]
    fn writable_roots_keeps_the_roots_that_could_be_read_in_order() {
        let a = root();
        let gone = root_at("/gone", false);
        let b = LibraryRoot {
            label: "/b".to_string(),
            ..root_at("/b", true)
        };
        let all = [a.clone(), gone, b.clone()];
        assert_eq!(writable_roots(&all), vec![&a, &b]);
    }

    #[test]
    fn first_writable_root_is_the_first_root_that_could_be_read() {
        let b = root_at("/b", true);
        let all = [root_at("/gone", false), b.clone()];
        assert_eq!(first_writable_root(&all), Some(&b));
    }

    #[test]
    fn first_writable_root_is_none_when_no_root_can_be_read() {
        assert_eq!(first_writable_root(&[]), None);
        assert_eq!(first_writable_root(&[root_at("/x", false)]), None);
    }

    #[test]
    fn draft_is_not_ready_and_says_nothing_while_the_name_is_empty() {
        assert_eq!(
            new_skill_draft("", Some(&root())),
            NewSkillDraft {
                problems: vec![],
                announcement: String::new(),
                ready: false,
                location: None
            }
        );
        assert_eq!(
            new_skill_draft("   ", Some(&root())).problems,
            Vec::<String>::new()
        );
        assert!(!new_skill_draft("   ", Some(&root())).ready);
    }

    #[test]
    fn draft_is_ready_for_a_valid_name_and_shows_where_the_file_will_be() {
        assert_eq!(
            new_skill_draft("pdf-processing", Some(&root())),
            NewSkillDraft {
                problems: vec![],
                announcement: String::new(),
                ready: true,
                location: Some("~/.claude/skills/pdf-processing/SKILL.md".to_string())
            }
        );
    }

    #[test]
    fn draft_uses_the_trimmed_name() {
        assert_eq!(
            new_skill_draft("  pdf-processing ", Some(&root()))
                .location
                .as_deref(),
            Some("~/.claude/skills/pdf-processing/SKILL.md")
        );
    }

    #[test]
    fn draft_applies_the_same_rules_as_the_frontmatter_check() {
        let draft = new_skill_draft("PDF--tool-", Some(&root()));
        assert!(!draft.ready);
        assert_eq!(
            draft.problems,
            vec![
                "name must be lowercase".to_string(),
                "name must not start or end with a hyphen".to_string(),
                "name must not contain consecutive hyphens".to_string(),
            ]
        );
    }

    #[test]
    fn draft_says_how_long_a_name_is_when_it_is_too_long() {
        assert_eq!(
            new_skill_draft(&"a".repeat(70), Some(&root())).problems,
            vec!["name has 70 characters; the limit is 64".to_string()]
        );
    }

    fn announced(name: &str) -> String {
        new_skill_draft(name, Some(&root())).announcement
    }

    #[test]
    fn announcement_names_the_kinds_of_problem_with_no_numbers() {
        assert_eq!(
            announced("PDF--tool-"),
            "Name problems: name is not lowercase; name starts or ends with a hyphen; name has consecutive hyphens"
        );
        assert_eq!(
            announced(&"a".repeat(70)),
            "Name problems: name is too long"
        );
    }

    #[test]
    fn announcement_stays_the_same_while_only_the_numbers_change() {
        assert_eq!(announced(&"a".repeat(70)), announced(&"a".repeat(71)));
    }

    #[test]
    fn announcement_changes_when_a_kind_of_problem_appears_or_goes() {
        assert_eq!(announced("p"), "");
        assert_ne!(announced("P"), announced("p"));
        assert_eq!(announced("Pd"), announced("P"));
        assert_ne!(announced("Pd f"), announced("Pd"));
    }

    #[test]
    fn announcement_says_a_skill_with_the_name_is_there() {
        assert_eq!(
            announced("pdf"),
            "Name problems: a skill folder with this name already exists"
        );
    }

    #[test]
    fn draft_refuses_a_name_another_skill_in_the_folder_already_has() {
        let draft = new_skill_draft("pdf", Some(&root()));
        assert!(!draft.ready);
        assert_eq!(
            draft.problems,
            vec!["A skill folder named \"pdf\" already exists in ~/.claude/skills".to_string()]
        );
    }

    #[test]
    fn draft_is_not_ready_without_a_folder_to_create_in() {
        let draft = new_skill_draft("pdf-processing", None);
        assert!(!draft.ready);
        assert_eq!(draft.location, None);
    }

    #[test]
    fn request_sends_the_root_and_the_trimmed_name() {
        assert_eq!(
            new_skill_request(" pdf ", &root()),
            CreateSkillRequest {
                root: "/home/u/.claude/skills".to_string(),
                name: "pdf".to_string()
            }
        );
    }
}
