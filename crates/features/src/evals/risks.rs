use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Risk {
    HooksFrontmatter,
    ShellCommands { lines: Vec<String> },
    LeftOut { names: Vec<String> },
}

#[derive(Clone, Debug)]
pub struct RiskInput {
    pub skill_text: String,
    pub left_out: Vec<String>,
}

const MAX_LINES: usize = 5;
const MAX_LINE_CHARS: usize = 100;

static FRONTMATTER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\u{feff}?---\r?\n([\s\S]*?)\r?\n---").expect("frontmatter"));
static HOOKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^hooks\s*:").expect("hooks"));
static INLINE_SHELL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!`[^`\n]+`").expect("inline"));
static SHELL_FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*```!").expect("fence"));

fn has_hooks_field(text: &str) -> bool {
    FRONTMATTER
        .captures(text)
        .and_then(|captures| captures.get(1))
        .is_some_and(|block| HOOKS.is_match(block.as_str()))
}

fn runs_shell(line: &str) -> bool {
    INLINE_SHELL.is_match(line) || SHELL_FENCE.is_match(line)
}

fn clip(line: &str) -> String {
    if line.encode_utf16().count() > MAX_LINE_CHARS {
        let head: String = line.chars().take(MAX_LINE_CHARS).collect();
        format!("{head}…")
    } else {
        line.to_string()
    }
}

pub fn find_risks(input: &RiskInput) -> Vec<Risk> {
    let shell_lines: Vec<String> = input
        .skill_text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| runs_shell(line))
        .map(|line| clip(crate::text::js_trim(line)))
        .take(MAX_LINES)
        .collect();
    let mut risks = Vec::new();
    if has_hooks_field(&input.skill_text) {
        risks.push(Risk::HooksFrontmatter);
    }
    if !shell_lines.is_empty() {
        risks.push(Risk::ShellCommands { lines: shell_lines });
    }
    if !input.left_out.is_empty() {
        risks.push(Risk::LeftOut {
            names: input.left_out.clone(),
        });
    }
    risks
}

fn describe_risk(risk: &Risk) -> String {
    match risk {
        Risk::HooksFrontmatter => {
            "SKILL.md has a hooks field. The Stage keeps it, so those hooks can run.".to_string()
        }
        Risk::ShellCommands { lines } => format!(
            "SKILL.md has lines that run shell commands, kept in the Stage: {}",
            lines.join(" | ")
        ),
        Risk::LeftOut { names } => format!("The Stage leaves out: {}", names.join(", ")),
    }
}

pub fn risk_lines(risks: &[Risk]) -> Vec<String> {
    risks.iter().map(describe_risk).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(frontmatter: &str, body: &str) -> String {
        format!("---\nname: s\ndescription: d\n{frontmatter}---\n{body}")
    }

    fn plain(frontmatter: &str) -> String {
        skill(frontmatter, "Do the thing.\n")
    }

    fn find(text: String, left_out: &[&str]) -> Vec<Risk> {
        find_risks(&RiskInput {
            skill_text: text,
            left_out: left_out
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
        })
    }

    #[test]
    fn finds_nothing_in_a_plain_skill() {
        assert_eq!(find(plain(""), &[]), vec![]);
    }

    #[test]
    fn flags_a_hooks_frontmatter_field() {
        assert_eq!(
            find(plain("hooks:\n  PreToolUse: []\n"), &[]),
            vec![Risk::HooksFrontmatter]
        );
    }

    #[test]
    fn flags_lines_that_run_a_shell_command() {
        assert_eq!(
            find(
                skill("", "Branch: !`git branch --show-current`\nplain\n"),
                &[]
            ),
            vec![Risk::ShellCommands {
                lines: vec!["Branch: !`git branch --show-current`".into()]
            }]
        );
    }

    #[test]
    fn flags_fenced_shell_blocks() {
        assert_eq!(
            find(skill("", "```!\nls\n```\n"), &[]),
            vec![Risk::ShellCommands {
                lines: vec!["```!".into()]
            }]
        );
    }

    #[test]
    fn names_what_the_stage_left_out() {
        assert_eq!(
            find(plain(""), &["hooks/", "link-out"]),
            vec![Risk::LeftOut {
                names: vec!["hooks/".into(), "link-out".into()]
            }]
        );
    }

    #[test]
    fn still_reads_a_skill_whose_frontmatter_is_broken() {
        assert_eq!(
            find("---\n: [\n---\n!`ls`\n".into(), &[]),
            vec![Risk::ShellCommands {
                lines: vec!["!`ls`".into()]
            }]
        );
    }

    #[test]
    fn is_stable_between_calls() {
        let text = skill("", "!`a`\n");
        assert_eq!(find(text.clone(), &["x/"]), find(text, &["x/"]));
    }

    #[test]
    fn words_each_risk() {
        let lines = risk_lines(&[
            Risk::HooksFrontmatter,
            Risk::ShellCommands {
                lines: vec!["!`ls`".into()],
            },
            Risk::LeftOut {
                names: vec!["hooks/".into()],
            },
        ]);
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("hooks"));
        assert!(lines[1].contains("!`ls`"));
        assert!(lines[2].contains("hooks/"));
    }
}
