use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::frontmatter_fields::{
    FrontmatterProblem, FrontmatterProblemRule, FrontmatterReading, SkillFieldValues,
    read_frontmatter, set_skill_fields,
};
use super::scan_skills::is_skill_file;
use super::skill_frontmatter::{CheckedField, SkillProblem, SkillRule, check_skill_frontmatter};

#[derive(Debug, Clone, Copy, Default)]
pub struct FrontmatterContext<'a> {
    pub file_name: Option<&'a str>,
    pub folder_name: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PanelRule {
    Skill(SkillRule),
    Frontmatter(FrontmatterProblemRule),
}

impl PanelRule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Skill(rule) => rule.as_str(),
            Self::Frontmatter(rule) => rule.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelProblem {
    pub field: Option<CheckedField>,
    pub rule: PanelRule,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FrontmatterReport {
    None,
    Unreadable {
        skill: bool,
        problems: Vec<PanelProblem>,
    },
    Skill {
        name: String,
        description: String,
        present: bool,
        problems: Vec<PanelProblem>,
    },
}

pub fn describe_skill_problem(problem: &SkillProblem) -> String {
    let message = problem.message.clone();
    match problem.rule {
        SkillRule::NameMissing => {
            "name is missing. It is required and must match the skill folder".to_string()
        }
        SkillRule::NameNotText => {
            "name must be text. Put it in quotes if it looks like a number or true / false"
                .to_string()
        }
        SkillRule::NameEmpty => "name is empty. It needs 1 to 64 characters".to_string(),
        SkillRule::NameTooLong
        | SkillRule::NameNotLowercase
        | SkillRule::NameInvalidCharacters
        | SkillRule::NameEdgeHyphen
        | SkillRule::NameDoubleHyphen
        | SkillRule::DescriptionTooLong
        | SkillRule::CompatibilityTooLong => message,
        SkillRule::NameFolderMismatch => {
            format!("{message}. Rename the folder or change the name")
        }
        SkillRule::DescriptionMissing => {
            "description is missing. Agents read it to decide when to use the skill".to_string()
        }
        SkillRule::DescriptionNotText => {
            "description must be text. Put it in quotes if it looks like a number or true / false"
                .to_string()
        }
        SkillRule::DescriptionEmpty => {
            "description is empty. Say what the skill does and when to use it".to_string()
        }
        SkillRule::CompatibilityNotText => "compatibility must be text".to_string(),
        SkillRule::CompatibilityEmpty => {
            "compatibility is empty. Remove it or say what the skill needs".to_string()
        }
    }
}

pub fn describe_frontmatter_problem(problem: &FrontmatterProblem) -> String {
    match problem.rule {
        FrontmatterProblemRule::FrontmatterNotClosed => {
            "The frontmatter is not closed. Add a --- line after the last key".to_string()
        }
        FrontmatterProblemRule::FrontmatterNotRecognized => {
            "The --- block at the top is not key: value lines, so it is not read as frontmatter"
                .to_string()
        }
        FrontmatterProblemRule::FrontmatterNotAMapping => {
            "The frontmatter must be key: value lines".to_string()
        }
        FrontmatterProblemRule::YamlSyntaxError => match problem.line {
            None => format!("YAML error: {}", problem.message),
            Some(line) => format!("YAML error on line {line}: {}", problem.message),
        },
    }
}

pub fn announce_rule(rule: PanelRule) -> &'static str {
    match rule {
        PanelRule::Skill(rule) => match rule {
            SkillRule::NameMissing => "name is missing",
            SkillRule::NameNotText => "name is not text",
            SkillRule::NameEmpty => "name is empty",
            SkillRule::NameTooLong => "name is too long",
            SkillRule::NameNotLowercase => "name is not lowercase",
            SkillRule::NameInvalidCharacters => "name has invalid characters",
            SkillRule::NameEdgeHyphen => "name starts or ends with a hyphen",
            SkillRule::NameDoubleHyphen => "name has consecutive hyphens",
            SkillRule::NameFolderMismatch => "name does not match its folder",
            SkillRule::DescriptionMissing => "description is missing",
            SkillRule::DescriptionNotText => "description is not text",
            SkillRule::DescriptionEmpty => "description is empty",
            SkillRule::DescriptionTooLong => "description is too long",
            SkillRule::CompatibilityNotText => "compatibility is not text",
            SkillRule::CompatibilityEmpty => "compatibility is empty",
            SkillRule::CompatibilityTooLong => "compatibility is too long",
        },
        PanelRule::Frontmatter(rule) => match rule {
            FrontmatterProblemRule::FrontmatterNotClosed => "the frontmatter is not closed",
            FrontmatterProblemRule::FrontmatterNotRecognized => "the frontmatter is not recognized",
            FrontmatterProblemRule::FrontmatterNotAMapping => {
                "the frontmatter is not key: value lines"
            }
            FrontmatterProblemRule::YamlSyntaxError => "the frontmatter has a YAML error",
        },
    }
}

fn unreadable(skill: bool, problem: &FrontmatterProblem) -> FrontmatterReport {
    FrontmatterReport::Unreadable {
        skill,
        problems: vec![PanelProblem {
            field: None,
            rule: PanelRule::Frontmatter(problem.rule),
            message: describe_frontmatter_problem(problem),
        }],
    }
}

fn shown(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Bool(flag)) => flag.to_string(),
        _ => String::new(),
    }
}

pub fn inspect_frontmatter(text: &str, context: &FrontmatterContext) -> FrontmatterReport {
    let skill = is_skill_file(context.file_name);
    let reading = read_frontmatter(text);
    if let FrontmatterReading::Invalid(problem) = &reading {
        return unreadable(skill, problem);
    }
    if !skill {
        return FrontmatterReport::None;
    }
    if reading == FrontmatterReading::Absent
        && let Err(problem) = set_skill_fields(text, &SkillFieldValues::default())
    {
        return unreadable(true, &problem);
    }
    let (fields, present) = match reading {
        FrontmatterReading::Present(fields) => (fields, true),
        _ => (serde_json::Map::new(), false),
    };
    FrontmatterReport::Skill {
        name: shown(fields.get("name")),
        description: shown(fields.get("description")),
        present,
        problems: check_skill_frontmatter(&fields, context.folder_name)
            .into_iter()
            .map(|problem| PanelProblem {
                field: Some(problem.field),
                rule: PanelRule::Skill(problem.rule),
                message: describe_skill_problem(&problem),
            })
            .collect(),
    }
}

fn problems_in(report: &FrontmatterReport) -> &[PanelProblem] {
    match report {
        FrontmatterReport::None => &[],
        FrontmatterReport::Unreadable { problems, .. }
        | FrontmatterReport::Skill { problems, .. } => problems,
    }
}

pub fn report_problems(report: &FrontmatterReport) -> Vec<String> {
    problems_in(report)
        .iter()
        .map(|problem| problem.message.clone())
        .collect()
}

pub fn problems_of_field(report: &FrontmatterReport, field: CheckedField) -> Vec<PanelProblem> {
    problems_in(report)
        .iter()
        .filter(|problem| problem.field == Some(field))
        .cloned()
        .collect()
}

pub fn announce_problems(report: &FrontmatterReport) -> String {
    match report {
        FrontmatterReport::None => String::new(),
        FrontmatterReport::Unreadable { problems, .. }
        | FrontmatterReport::Skill { problems, .. } => {
            if problems.is_empty() {
                "No frontmatter problems".to_string()
            } else {
                let phrases: Vec<&str> = problems
                    .iter()
                    .map(|problem| announce_rule(problem.rule))
                    .collect();
                format!("Frontmatter problems: {}", phrases.join("; "))
            }
        }
    }
}

pub fn other_problems(report: &FrontmatterReport) -> Vec<PanelProblem> {
    problems_in(report)
        .iter()
        .filter(|problem| {
            problem.field != Some(CheckedField::Name)
                && problem.field != Some(CheckedField::Description)
        })
        .cloned()
        .collect()
}

pub fn is_same_report(a: &FrontmatterReport, b: &FrontmatterReport) -> bool {
    a == b
}
