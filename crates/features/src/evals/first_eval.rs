#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExampleFile {
    pub path: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirstEvalExample {
    pub tree: String,
    pub files: Vec<ExampleFile>,
}

const EVAL_NAME: &str = "first-eval";

fn folder() -> String {
    format!("evals/{EVAL_NAME}")
}

fn quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn escape_regex(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| {
            if ".*+?^${}()|[]\\/".contains(c) {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect()
}

fn frontmatter(fields: &[(&str, String)], body: &str) -> String {
    let mut lines = vec!["---".to_string()];
    lines.extend(fields.iter().map(|(key, value)| format!("{key}: {value}")));
    lines.extend([
        "---".to_string(),
        String::new(),
        body.to_string(),
        String::new(),
    ]);
    lines.join("\n")
}

fn skill_name_pattern(skill_name: &str) -> String {
    format!("[\":]{}\"", escape_regex(skill_name))
}

pub fn first_eval_example(skill_name: &str, folder_name: &str) -> FirstEvalExample {
    let folder = folder();
    FirstEvalExample {
        tree: [
            format!("{folder_name}/{folder}/"),
            "├── prompt.md       what a user would ask".to_string(),
            "└── graders/".to_string(),
            "    ├── fired.md    type: tool_used, did the Skill fire?".to_string(),
            "    └── answer.md   type: regex, does the answer say it?".to_string(),
        ]
        .join("\n"),
        files: vec![
            ExampleFile {
                path: format!("{folder}/prompt.md"),
                text: frontmatter(
                    &[
                        ("max_turns", "10".to_string()),
                        ("allowed_tools", "[Read, Glob, Grep, Skill]".to_string()),
                    ],
                    "Write the request a user would make that this Skill should handle.",
                ),
            },
            ExampleFile {
                path: format!("{folder}/graders/fired.md"),
                text: frontmatter(
                    &[
                        ("type", "tool_used".to_string()),
                        ("tool", "Skill".to_string()),
                        ("input_match", quoted(&skill_name_pattern(skill_name))),
                    ],
                    "Passes when the agent loads this Skill.",
                ),
            },
            ExampleFile {
                path: format!("{folder}/graders/answer.md"),
                text: frontmatter(
                    &[
                        ("type", "regex".to_string()),
                        ("pattern", quoted("text the answer must contain")),
                    ],
                    "Passes when the last message matches the pattern.",
                ),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use regex::Regex;

    use super::*;
    use crate::evals::report::matches_skill_pattern;

    fn file_text(skill_name: &str, path: &str) -> String {
        first_eval_example(skill_name, skill_name)
            .files
            .into_iter()
            .find(|file| file.path == path)
            .map(|file| file.text)
            .unwrap_or_default()
    }

    fn frontmatter_value(text: &str, key: &str) -> String {
        let line = text
            .split('\n')
            .find(|row| row.starts_with(&format!("{key}: ")))
            .unwrap_or("");
        let value = &line[(key.len() + 2).min(line.len())..];
        let unquoted = value
            .strip_prefix('\'')
            .and_then(|rest| rest.strip_suffix('\''))
            .unwrap_or(value);
        unquoted.replace("''", "'")
    }

    #[test]
    fn draws_the_folder_under_the_skills_own_folder_name() {
        let example = first_eval_example("pr-format", "pr-format-skill");
        assert_eq!(
            example.tree.split('\n').next(),
            Some("pr-format-skill/evals/first-eval/")
        );
        for part in ["prompt.md", "graders/", "fired.md", "answer.md"] {
            assert!(example.tree.contains(part));
        }
    }

    #[test]
    fn lists_a_prompt_a_fired_grader_and_an_answer_grader() {
        let paths: Vec<_> = first_eval_example("pr-format", "pr-format")
            .files
            .into_iter()
            .map(|file| file.path)
            .collect();
        assert_eq!(
            paths,
            vec![
                "evals/first-eval/prompt.md",
                "evals/first-eval/graders/fired.md",
                "evals/first-eval/graders/answer.md"
            ]
        );
    }

    #[test]
    fn checks_that_the_skill_fired_with_a_free_tool_used_grader() {
        let text = file_text("pr-format", "evals/first-eval/graders/fired.md");
        assert_eq!(frontmatter_value(&text, "type"), "tool_used");
        assert_eq!(frontmatter_value(&text, "tool"), "Skill");
        assert!(matches_skill_pattern(
            &frontmatter_value(&text, "input_match"),
            "pr-format"
        ));
    }

    #[test]
    fn checks_the_answer_with_a_free_regex_grader() {
        let text = file_text("pr-format", "evals/first-eval/graders/answer.md");
        assert_eq!(frontmatter_value(&text, "type"), "regex");
        let pattern = frontmatter_value(&text, "pattern");
        assert!(Regex::new(&pattern).unwrap().is_match(&pattern));
    }

    #[test]
    fn escapes_the_skill_name_so_input_match_matches_only_that_skill() {
        let text = file_text("a.b (x)", "evals/first-eval/graders/fired.md");
        let pattern = frontmatter_value(&text, "input_match");
        assert!(matches_skill_pattern(&pattern, "a.b (x)"));
        assert!(!matches_skill_pattern(&pattern, "axb (x)"));
    }

    #[test]
    fn does_not_fire_for_a_skill_whose_name_only_starts_with_this_one() {
        let text = file_text("pr", "evals/first-eval/graders/fired.md");
        let pattern = frontmatter_value(&text, "input_match");
        assert!(matches_skill_pattern(&pattern, "pr"));
        assert!(!matches_skill_pattern(&pattern, "pr-format"));
        assert!(!matches_skill_pattern(&pattern, "api-pr"));
    }

    #[test]
    fn doubles_single_quotes_inside_a_quoted_value() {
        let text = file_text("it's", "evals/first-eval/graders/fired.md");
        assert!(text.contains("input_match: '[\":]it''s\"'"));
    }

    #[test]
    fn gives_the_prompt_frontmatter_the_cli_scaffolds() {
        let text = file_text("pr-format", "evals/first-eval/prompt.md");
        assert!(text.starts_with("---\n"));
        assert_eq!(frontmatter_value(&text, "max_turns"), "10");
    }
}
