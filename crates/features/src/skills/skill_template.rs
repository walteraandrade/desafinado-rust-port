use super::frontmatter_fields::{FrontmatterProblem, SkillFieldValues, set_skill_fields};
use crate::text::js_trim;

pub const TEMPLATE_DESCRIPTION: &str =
    "Describe what this skill does and when an agent should use it.";

fn title_of(name: &str) -> String {
    let words = name.replace('-', " ");
    let mut chars = words.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
    }
}

fn body_of(name: &str) -> String {
    [
        format!("# {}", title_of(name)).as_str(),
        "",
        "Say in a sentence or two what this skill is for.",
        "",
        "## Instructions",
        "",
        "1. Describe the first step.",
        "2. Describe the next step.",
        "",
        "## Examples",
        "",
        "Show an input and the output the agent should produce.",
        "",
        "## Edge cases",
        "",
        "List the cases that need care, and what to do in each.",
        "",
    ]
    .join("\n")
}

pub fn skill_template(name: &str) -> Result<String, FrontmatterProblem> {
    let trimmed = js_trim(name);
    set_skill_fields(
        &body_of(trimmed),
        &SkillFieldValues::both(trimmed, TEMPLATE_DESCRIPTION),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::frontmatter_fields::{
        FrontmatterFields, FrontmatterReading, read_frontmatter,
    };
    use crate::skills::skill_frontmatter::{DESCRIPTION_MAX_LENGTH, check_skill_frontmatter};
    use serde_json::json;

    fn template(name: &str) -> String {
        skill_template(name).expect("template")
    }

    fn fields_of(text: &str) -> FrontmatterFields {
        match read_frontmatter(text) {
            FrontmatterReading::Present(fields) => fields,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn starts_with_the_frontmatter_the_agent_skills_format_asks_for() {
        assert!(template("pdf-processing").starts_with(&format!(
            "---\nname: pdf-processing\ndescription: {TEMPLATE_DESCRIPTION}\n---\n\n"
        )));
    }

    #[test]
    fn reads_back_as_the_name_given_and_the_placeholder_description() {
        assert_eq!(
            serde_json::Value::Object(fields_of(&template("pdf-processing"))),
            json!({"name": "pdf-processing", "description": TEMPLATE_DESCRIPTION})
        );
    }

    #[test]
    fn passes_every_check_with_the_folder_named_after_the_skill() {
        for name in ["pdf-processing", "a", "ação", "x-1", "9lives"] {
            assert_eq!(
                check_skill_frontmatter(&fields_of(&template(name)), Some(name)),
                vec![]
            );
        }
    }

    #[test]
    fn keeps_a_name_that_yaml_would_read_as_another_type_a_string() {
        for name in ["true", "null", "123", "1e3", "0x1f", "007", "٣"] {
            assert_eq!(fields_of(&template(name))["name"], json!(name), "{name}");
        }
    }

    #[test]
    fn writes_the_trimmed_name() {
        assert_eq!(fields_of(&template("  pdf  "))["name"], json!("pdf"));
    }

    #[test]
    fn has_a_placeholder_description_inside_the_limit() {
        assert!(!TEMPLATE_DESCRIPTION.trim().is_empty());
        assert!(TEMPLATE_DESCRIPTION.encode_utf16().count() <= DESCRIPTION_MAX_LENGTH);
        assert!(!TEMPLATE_DESCRIPTION.contains('\n'));
    }

    #[test]
    fn titles_the_body_with_the_name_in_words() {
        assert!(template("pdf-processing").contains("\n# Pdf processing\n"));
        assert!(template("ação-rápida").contains("\n# Ação rápida\n"));
        assert!(template("123").contains("\n# 123\n"));
    }

    #[test]
    fn has_the_sections_the_specification_recommends() {
        let text = template("pdf-processing");
        assert!(text.contains("\n## Instructions\n"));
        assert!(text.contains("\n## Examples\n"));
        assert!(text.contains("\n## Edge cases\n"));
    }

    #[test]
    fn ends_with_one_line_break_uses_lf_and_stays_far_under_500_lines() {
        let text = template("pdf-processing");
        assert!(text.ends_with('\n'));
        assert!(!text.ends_with("\n\n"));
        assert!(!text.contains('\r'));
        assert!(text.split('\n').count() < 60);
    }

    #[test]
    fn does_not_open_with_a_second_block_that_would_confuse_the_body() {
        let text = template("pdf-processing");
        assert_eq!(text.find("\n---\n"), text.rfind("\n---\n"));
    }
}
