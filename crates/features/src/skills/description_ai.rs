use regex::Regex;
use std::sync::LazyLock;

use super::frontmatter_block::find_frontmatter;
use super::skill_frontmatter::DESCRIPTION_MAX_LENGTH;
use crate::text::js_trim;

pub const SKILL_BODY_MAX_CHARS: usize = 20_000;

static FENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^```[a-z]*\s*\n?|\n?```\s*$").expect("valid pattern"));
static LABEL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^description:\s*").expect("valid pattern"));
static WHITESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").expect("valid pattern"));

fn unquote(text: &str) -> &str {
    let mut chars = text.chars();
    match (chars.next(), chars.next_back()) {
        (Some(first), Some(last)) if first == last && (first == '"' || first == '\'') => {
            &text[first.len_utf8()..text.len() - last.len_utf8()]
        }
        _ => text,
    }
}

pub fn normalize_description(text: &str) -> String {
    let unfenced = FENCE.replace_all(text, "");
    let unlabeled = LABEL.replace(js_trim(&unfenced), "");
    let unquoted = unquote(&unlabeled);
    js_trim(&WHITESPACE.replace_all(unquoted, " ")).to_string()
}

const CUT_NOTE: &str = "\n[The body is cut here.]";

pub fn skill_body(text: &str) -> String {
    let start = find_frontmatter(text).map_or(0, |found| found.to);
    let body = js_trim(&text[start..]);
    if body.chars().count() > SKILL_BODY_MAX_CHARS {
        let cut: String = body.chars().take(SKILL_BODY_MAX_CHARS).collect();
        format!("{cut}{CUT_NOTE}")
    } else {
        body.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DescriptionOutcome {
    Ready { description: String },
    Failed { message: String },
}

pub fn description_outcome(replacement: &str, current: &str) -> DescriptionOutcome {
    let next = normalize_description(replacement);
    if next.is_empty() || next == js_trim(current) {
        DescriptionOutcome::Failed {
            message: "No better description came back.".to_string(),
        }
    } else if next.encode_utf16().count() > DESCRIPTION_MAX_LENGTH {
        DescriptionOutcome::Failed {
            message: format!(
                "The new description came back over {DESCRIPTION_MAX_LENGTH} characters."
            ),
        }
    } else {
        DescriptionOutcome::Ready { description: next }
    }
}

pub fn description_source(name: &str, description: &str) -> String {
    serde_json::to_string(&[name, description]).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_turns_lines_into_one_line() {
        assert_eq!(
            normalize_description("Use when the user\nasks about  PDFs.\n"),
            "Use when the user asks about PDFs."
        );
    }

    #[test]
    fn normalize_drops_quotes_a_code_fence_and_a_leading_description_label() {
        assert_eq!(normalize_description("\"Use when X.\""), "Use when X.");
        assert_eq!(
            normalize_description("description: Use when X."),
            "Use when X."
        );
        assert_eq!(
            normalize_description("```\nUse when X.\n```"),
            "Use when X."
        );
    }

    #[test]
    fn normalize_leaves_an_ordinary_description_alone() {
        assert_eq!(
            normalize_description("Use when X: do Y, then Z."),
            "Use when X: do Y, then Z."
        );
    }

    #[test]
    fn normalize_returns_empty_text_for_nothing() {
        assert_eq!(normalize_description("  \n "), "");
    }

    #[test]
    fn skill_body_returns_the_text_after_the_frontmatter_trimmed() {
        assert_eq!(
            skill_body("---\nname: a\ndescription: b\n---\n\n# Title\n\nBody.\n"),
            "# Title\n\nBody."
        );
    }

    #[test]
    fn skill_body_returns_the_whole_text_when_there_is_no_frontmatter() {
        assert_eq!(skill_body("# Title\n"), "# Title");
    }

    #[test]
    fn skill_body_cuts_a_long_body_and_says_so() {
        let body = skill_body(&format!("---\nname: a\n---\n{}", "x".repeat(50_000)));
        assert_eq!(
            body,
            format!(
                "{}\n[The body is cut here.]",
                "x".repeat(SKILL_BODY_MAX_CHARS)
            )
        );
    }

    #[test]
    fn skill_body_keeps_a_body_of_exactly_the_limit_whole() {
        assert_eq!(
            skill_body(&"x".repeat(SKILL_BODY_MAX_CHARS)),
            "x".repeat(SKILL_BODY_MAX_CHARS)
        );
    }

    #[test]
    fn outcome_accepts_a_new_description_normalized() {
        assert_eq!(
            description_outcome("\"Does X. Use when Y.\"", "Does X."),
            DescriptionOutcome::Ready {
                description: "Does X. Use when Y.".to_string()
            }
        );
    }

    #[test]
    fn outcome_fails_on_nothing_or_on_the_same_text() {
        let none = DescriptionOutcome::Failed {
            message: "No better description came back.".to_string(),
        };
        assert_eq!(description_outcome(" ", "Does X."), none);
        assert_eq!(description_outcome("Does X.", " Does X. "), none);
    }

    #[test]
    fn outcome_fails_when_the_answer_only_restates_the_current_text() {
        assert!(matches!(
            description_outcome("description: \"Does X.\"", "Does X."),
            DescriptionOutcome::Failed { .. }
        ));
    }

    #[test]
    fn outcome_accepts_exactly_1024_characters() {
        assert!(matches!(
            description_outcome(&"x".repeat(1024), "Does X."),
            DescriptionOutcome::Ready { .. }
        ));
    }

    #[test]
    fn outcome_fails_on_a_description_over_the_spec_limit() {
        assert_eq!(
            description_outcome(&"x".repeat(1025), "Does X."),
            DescriptionOutcome::Failed {
                message: "The new description came back over 1024 characters.".to_string()
            }
        );
    }

    #[test]
    fn source_differs_when_the_skill_or_the_description_differs() {
        let base = description_source("pdf-tools", "Works with PDFs.");
        assert_eq!(description_source("pdf-tools", "Works with PDFs."), base);
        assert_ne!(description_source("other", "Works with PDFs."), base);
        assert_ne!(description_source("pdf-tools", "Works with docs."), base);
        assert_ne!(
            description_source("a", "b\nc"),
            description_source("a\nb", "c")
        );
    }
}
