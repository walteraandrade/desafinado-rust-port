use regex::Regex;
use std::sync::LazyLock;

use crate::text::{is_js_space, js_trim};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frontmatter {
    pub from: usize,
    pub to: usize,
    pub yaml_from: usize,
    pub yaml_to: usize,
}

static MAPPING_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^(?:[\p{L}\p{N}_.-]+|"[^"]*"|'[^']*')[ \t]*:(?:[ \t]|$)"#).expect("valid pattern")
});

fn looks_like_yaml(yaml: &str) -> bool {
    if js_trim(yaml).is_empty() {
        return true;
    }
    yaml.split('\n')
        .find(|line| {
            !js_trim(line).is_empty() && !line.trim_start_matches(is_js_space).starts_with('#')
        })
        .is_some_and(|first| MAPPING_KEY.is_match(first))
}

fn rule_end(text: &str, at: usize) -> Option<usize> {
    let rest = text.get(at..)?;
    let after = rest.strip_prefix("---")?;
    let blanks = after.len() - after.trim_start_matches([' ', '\t']).len();
    let end = at + 3 + blanks;
    match text.as_bytes().get(end) {
        None | Some(b'\n') => Some(end),
        _ => None,
    }
}

pub fn opening_end(text: &str) -> Option<usize> {
    let end = rule_end(text, 0)?;
    (text.as_bytes().get(end) == Some(&b'\n')).then_some(end + 1)
}

fn closing(text: &str, yaml_from: usize) -> Option<(Option<usize>, usize)> {
    let grouped = text
        .char_indices()
        .skip_while(|(index, _)| *index < yaml_from)
        .filter(|(_, c)| *c == '\n')
        .find_map(|(index, _)| rule_end(text, index + 1).map(|end| (Some(index), end)));
    grouped.or_else(|| rule_end(text, yaml_from).map(|end| (None, end)))
}

pub fn find_frontmatter(text: &str) -> Option<Frontmatter> {
    if !text.starts_with("---") {
        return None;
    }
    let yaml_from = opening_end(text)?;
    let (group_end, to) = closing(text, yaml_from)?;
    let yaml = group_end.map_or("", |end| &text[yaml_from..end]);
    looks_like_yaml(yaml).then_some(Frontmatter {
        from: 0,
        to,
        yaml_from,
        yaml_to: yaml_from + yaml.len(),
    })
}

pub fn is_delimited_block(text: &str) -> bool {
    opening_end(text).is_some_and(|yaml_from| closing(text, yaml_from).is_some())
}

pub fn touches_frontmatter(frontmatter: Option<&Frontmatter>, from: usize, to: usize) -> bool {
    frontmatter.is_some_and(|found| from < found.to || (from == to && from == found.to))
}

pub fn blank_frontmatter(text: &str) -> String {
    match find_frontmatter(text) {
        None => text.to_string(),
        Some(found) => format!("{}{}", " ".repeat(found.to), &text[found.to..]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKILL: &str = "---\nname: demo\ndescription: Does a thing.\n---\n\n# Demo\n";

    fn slice(text: &str, from: usize, to: usize) -> &str {
        &text[from..to]
    }

    #[test]
    fn finds_a_frontmatter_at_the_start_of_the_document() {
        let found = find_frontmatter(SKILL).expect("found");
        assert_eq!(
            slice(SKILL, found.from, found.to),
            "---\nname: demo\ndescription: Does a thing.\n---"
        );
    }

    #[test]
    fn exposes_the_yaml_between_the_delimiters() {
        let found = find_frontmatter(SKILL).expect("found");
        assert_eq!(
            slice(SKILL, found.yaml_from, found.yaml_to),
            "name: demo\ndescription: Does a thing."
        );
    }

    #[test]
    fn returns_nothing_for_a_document_without_one() {
        assert_eq!(find_frontmatter("# Title\n\nBody.\n"), None);
        assert_eq!(find_frontmatter(""), None);
    }

    #[test]
    fn returns_nothing_when_the_closing_delimiter_is_missing() {
        assert_eq!(find_frontmatter("---\nname: demo\n\n# Demo\n"), None);
        assert_eq!(find_frontmatter("---\n"), None);
        assert_eq!(find_frontmatter("---"), None);
    }

    #[test]
    fn returns_nothing_when_it_is_not_at_offset_0() {
        assert_eq!(find_frontmatter("\n---\nname: demo\n---\n"), None);
        assert_eq!(find_frontmatter("# Title\n\n---\nname: demo\n---\n"), None);
        assert_eq!(find_frontmatter(" ---\nname: demo\n---\n"), None);
    }

    #[test]
    fn requires_exactly_three_dashes_on_both_delimiters() {
        assert_eq!(find_frontmatter("----\nname: demo\n---\n"), None);
        assert_eq!(find_frontmatter("---\nname: demo\n----\n"), None);
        assert_eq!(find_frontmatter("--- a\nname: demo\n---\n"), None);
        assert_eq!(find_frontmatter("---\nname: demo\n--- b\n"), None);
    }

    #[test]
    fn does_not_close_on_a_dot_line() {
        assert_eq!(find_frontmatter("---\nname: demo\n...\n\nBody.\n"), None);
    }

    #[test]
    fn does_not_close_on_an_indented_delimiter() {
        assert_eq!(find_frontmatter("---\nname: demo\n ---\n"), None);
    }

    #[test]
    fn tolerates_trailing_blanks_on_the_delimiters() {
        let text = "---  \nname: demo\n---\t\n\nBody.\n";
        let found = find_frontmatter(text).expect("found");
        assert_eq!(slice(text, found.yaml_from, found.yaml_to), "name: demo");
        assert_eq!(
            slice(text, found.from, found.to),
            "---  \nname: demo\n---\t"
        );
    }

    #[test]
    fn ends_at_the_closing_delimiter_before_its_line_break() {
        assert_eq!(
            find_frontmatter("---\na: 1\n---\nBody.").map(|found| found.to),
            Some(12)
        );
    }

    #[test]
    fn accepts_a_closing_delimiter_at_the_very_end() {
        let text = "---\nname: demo\n---";
        assert_eq!(find_frontmatter(text).map(|found| found.to), Some(text.len()));
    }

    #[test]
    fn stops_at_the_first_closing_delimiter() {
        let text = "---\na: 1\n---\n\nBody.\n\n---\n\nMore.\n";
        let found = find_frontmatter(text).expect("found");
        assert_eq!(&text[..found.to], "---\na: 1\n---");
    }

    #[test]
    fn finds_an_empty_frontmatter() {
        let adjacent = find_frontmatter("---\n---\n\nBody.\n").expect("found");
        assert_eq!(
            adjacent,
            Frontmatter {
                from: 0,
                to: 7,
                yaml_from: 4,
                yaml_to: 4
            }
        );
        let blank = "---\n\n---\n\nBody.\n";
        let found = find_frontmatter(blank).expect("found");
        assert_eq!(found.to, 8);
        assert_eq!(slice(blank, found.yaml_from, found.yaml_to), "");
    }

    #[test]
    fn finds_a_document_that_is_only_a_frontmatter() {
        let text = "---\nname: demo\n---\n";
        assert_eq!(
            find_frontmatter(text).map(|found| found.to),
            Some(text.len() - 1)
        );
    }

    #[test]
    fn keeps_blank_lines_and_yaml_that_looks_like_markdown_inside_it() {
        let text = "---\ndescription: |\n  # not a heading\n\n  - not a list\n```\n---\n\nBody.\n";
        let found = find_frontmatter(text).expect("found");
        assert_eq!(
            slice(text, found.yaml_from, found.yaml_to),
            "description: |\n  # not a heading\n\n  - not a list\n```"
        );
    }

    #[test]
    fn between_two_plain_rules_is_not_a_frontmatter_when_the_first_line_is_prose() {
        assert_eq!(find_frontmatter("---\n\nIntro.\n\n---\n\nRest.\n"), None);
        assert_eq!(find_frontmatter("---\nJust a paragraph.\n---\n"), None);
        assert_eq!(find_frontmatter("---\nNote that: it works\n---\n"), None);
    }

    #[test]
    fn between_two_plain_rules_is_not_a_frontmatter_for_a_list_item_or_a_heading() {
        assert_eq!(find_frontmatter("---\n- one\n- two\n---\n"), None);
        assert_eq!(find_frontmatter("---\n## Heading\ntext\n---\n"), None);
    }

    #[test]
    fn between_two_plain_rules_is_not_a_frontmatter_with_only_comments() {
        assert_eq!(find_frontmatter("---\n# Title\n---\n"), None);
        assert_eq!(find_frontmatter("---\n# one\n\n  # two\n---\n"), None);
    }

    #[test]
    fn between_two_plain_rules_is_not_a_frontmatter_when_comments_are_followed_by_prose() {
        assert_eq!(find_frontmatter("---\n# Title\nIntro.\n---\n"), None);
    }

    #[test]
    fn needs_a_space_or_the_end_of_the_line_after_the_colon() {
        assert_eq!(find_frontmatter("---\nhttps://example.com\n---\n"), None);
        assert_eq!(find_frontmatter("---\nname:demo\n---\n"), None);
        assert!(find_frontmatter("---\nname:\n---\n").is_some());
        assert!(find_frontmatter("---\nname:\tdemo\n---\n").is_some());
    }

    #[test]
    fn real_frontmatter_opens_with_a_comment_line() {
        let text = "---\n# Managed by hand\nname: demo\n---\n\nBody.\n";
        assert_eq!(
            find_frontmatter(text).map(|found| found.to),
            text.find("\n\nBody.")
        );
    }

    #[test]
    fn real_frontmatter_opens_with_blank_lines_and_comments() {
        let text = "---\n\n  # note\n# another\n\nname: demo\n---\n";
        assert!(find_frontmatter(text).is_some());
    }

    #[test]
    fn real_frontmatter_has_a_quoted_or_hyphenated_first_key() {
        assert!(find_frontmatter("---\n\"name\": demo\n---\n").is_some());
        assert!(find_frontmatter("---\n'name': demo\n---\n").is_some());
        assert!(find_frontmatter("---\nallowed-tools: Read\n---\n").is_some());
        assert!(find_frontmatter("---\nnome-da-habilidade: x\n---\n").is_some());
    }

    #[test]
    fn real_frontmatter_is_empty_or_only_blank_lines() {
        assert!(find_frontmatter("---\n---\n").is_some());
        assert!(find_frontmatter("---\n\n\n---\n").is_some());
        assert!(find_frontmatter("---\n \t\n---\n").is_some());
    }

    #[test]
    fn real_frontmatter_has_a_first_key_with_a_block_value() {
        let text = "---\ndescription: >\n  Prose: with a colon.\n---\n";
        assert!(find_frontmatter(text).is_some());
    }

    #[test]
    fn does_not_treat_a_lone_cr_as_a_line_break() {
        assert_eq!(find_frontmatter("---\r\nname: demo\r\n---\r\n"), None);
    }

    fn touches(frontmatter: Option<&Frontmatter>, from: usize, to: usize) -> bool {
        touches_frontmatter(frontmatter, from, to)
    }

    #[test]
    fn touches_refuses_a_cursor_inside_it_including_the_end_of_the_closing_line() {
        let found = find_frontmatter(SKILL);
        let end = found.map_or(0, |f| f.to);
        assert!(touches(found.as_ref(), 0, 0));
        assert!(touches(found.as_ref(), 8, 8));
        assert!(touches(found.as_ref(), end, end));
    }

    #[test]
    fn touches_refuses_a_range_inside_or_overlapping_it() {
        let found = find_frontmatter(SKILL);
        let end = found.map_or(0, |f| f.to);
        assert!(touches(found.as_ref(), 4, 10));
        assert!(touches(found.as_ref(), 4, end + 6));
        assert!(touches(found.as_ref(), 0, SKILL.len()));
    }

    #[test]
    fn touches_allows_a_range_that_starts_after_it() {
        let found = find_frontmatter(SKILL);
        let end = found.map_or(0, |f| f.to);
        assert!(!touches(found.as_ref(), end + 1, end + 1));
        assert!(!touches(found.as_ref(), end + 2, end + 8));
    }

    #[test]
    fn touches_allows_a_range_that_only_starts_where_the_frontmatter_ends() {
        let found = find_frontmatter(SKILL);
        let end = found.map_or(0, |f| f.to);
        assert!(!touches(found.as_ref(), end, end + 8));
    }

    #[test]
    fn touches_allows_anything_when_there_is_no_frontmatter() {
        assert!(!touches(None, 0, 5));
    }

    #[test]
    fn blank_frontmatter_blanks_it_and_keeps_every_offset() {
        let blanked = blank_frontmatter(SKILL);
        let to = find_frontmatter(SKILL).map_or(0, |f| f.to);
        assert_eq!(blanked.len(), SKILL.len());
        assert_eq!(&blanked[to..], &SKILL[to..]);
        assert_eq!(js_trim(&blanked[..to]), "");
    }

    #[test]
    fn blank_frontmatter_returns_the_text_unchanged_without_one() {
        let text = "# Title\n\nBody.\n";
        assert_eq!(blank_frontmatter(text), text);
    }
}
