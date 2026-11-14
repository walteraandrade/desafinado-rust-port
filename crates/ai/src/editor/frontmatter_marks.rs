use std::sync::LazyLock;

use regex::Regex;

use crate::text::js_trim;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineRole {
    Delimiter,
    Body,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontmatterMark {
    Line { from: usize, role: LineRole },
    Key { from: usize, to: usize },
}

static TOP_LEVEL_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^((?:[\p{L}\p{N}_.-]+|"[^"]*"|'[^']*'))[ \t]*:(?:[ \t]|$)"#).expect("valid regex")
});

fn looks_like_yaml(yaml: &str) -> bool {
    if js_trim(yaml).is_empty() {
        return true;
    }
    yaml.split('\n')
        .find(|line| !js_trim(line).is_empty() && !line.trim_start().starts_with('#'))
        .is_some_and(|first| TOP_LEVEL_KEY.is_match(first))
}

fn is_delimiter(line: &str) -> bool {
    line.strip_prefix("---")
        .is_some_and(|rest| rest.chars().all(|c| c == ' ' || c == '\t'))
}

pub fn frontmatter_end(text: &str) -> Option<usize> {
    if !text.starts_with("---") {
        return None;
    }
    let lines = lf_line_ranges(text);
    let (first_from, first_to) = lines[0];
    if !is_delimiter(&text[first_from..first_to]) || lines.len() < 2 {
        return None;
    }
    let close =
        (1..lines.len()).find(|index| is_delimiter(&text[lines[*index].0..lines[*index].1]))?;
    let yaml = if close == 1 {
        ""
    } else {
        &text[lines[1].0..lines[close - 1].1]
    };
    looks_like_yaml(yaml).then_some(lines[close].1)
}

fn marks_of_line(line: &str, from: usize, role: LineRole) -> Vec<FrontmatterMark> {
    let key = if role == LineRole::Body {
        TOP_LEVEL_KEY.captures(line).and_then(|c| c.get(1))
    } else {
        None
    };
    let mut marks = vec![FrontmatterMark::Line { from, role }];
    if let Some(key) = key {
        marks.push(FrontmatterMark::Key {
            from,
            to: from + key.end(),
        });
    }
    marks
}

pub fn frontmatter_marks(text: &str) -> Vec<FrontmatterMark> {
    let Some(end) = frontmatter_end(text) else {
        return Vec::new();
    };
    let ranges = lf_line_ranges(&text[..end]);
    let last = ranges.len() - 1;
    ranges
        .iter()
        .enumerate()
        .flat_map(|(index, (from, to))| {
            let role = if index == 0 || index == last {
                LineRole::Delimiter
            } else {
                LineRole::Body
            };
            marks_of_line(&text[*from..*to], *from, role)
        })
        .collect()
}

fn lf_line_ranges(text: &str) -> Vec<(usize, usize)> {
    text.split('\n')
        .scan(0, |offset, line| {
            let from = *offset;
            *offset += line.len() + 1;
            Some((from, from + line.len()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKILL: &str = "---\nname: demo\ndescription: Does a thing.\n---\n\n# Title\n";

    fn lines(text: &str) -> Vec<(usize, LineRole)> {
        frontmatter_marks(text)
            .into_iter()
            .filter_map(|mark| match mark {
                FrontmatterMark::Line { from, role } => Some((from, role)),
                FrontmatterMark::Key { .. } => None,
            })
            .collect()
    }

    fn keys(text: &str) -> Vec<&str> {
        frontmatter_marks(text)
            .into_iter()
            .filter_map(|mark| match mark {
                FrontmatterMark::Key { from, to } => Some(&text[from..to]),
                FrontmatterMark::Line { .. } => None,
            })
            .collect()
    }

    #[test]
    fn marks_every_line_of_the_frontmatter_delimiters_included() {
        assert_eq!(
            lines(SKILL),
            [
                (0, LineRole::Delimiter),
                (4, LineRole::Body),
                (15, LineRole::Body),
                (42, LineRole::Delimiter)
            ]
        );
    }

    #[test]
    fn marks_the_key_of_each_top_level_entry() {
        assert_eq!(keys(SKILL), ["name", "description"]);
    }

    #[test]
    fn leaves_the_body_alone() {
        let max = lines(SKILL).iter().map(|(from, _)| *from).max().unwrap();
        assert!(max < SKILL.find("# Title").unwrap());
    }

    #[test]
    fn marks_keys_with_dashes_quotes_and_digits() {
        let text = "---\nallowed-tools: Read\n\"quoted key\": 1\nv2: x\n---\n";
        assert_eq!(keys(text), ["allowed-tools", "\"quoted key\"", "v2"]);
    }

    #[test]
    fn does_not_take_text_inside_a_block_scalar_or_a_nested_map_for_a_key() {
        let text =
            "---\nname: demo\ndescription: |\n  note: not a key\nmetadata:\n  author: me\n---\n";
        assert_eq!(keys(text), ["name", "description", "metadata"]);
    }

    #[test]
    fn does_not_take_a_colon_inside_a_value_for_a_key() {
        assert_eq!(
            keys("---\ndescription: Use when: asked\n---\n"),
            ["description"]
        );
    }

    #[test]
    fn handles_an_empty_frontmatter() {
        let marks = frontmatter_marks("---\n---\n\nText\n");
        assert_eq!(marks.len(), 2);
        assert!(
            marks
                .iter()
                .all(|m| matches!(m, FrontmatterMark::Line { .. }))
        );
    }

    #[test]
    fn handles_a_frontmatter_that_ends_the_document() {
        let froms: Vec<usize> = lines("---\nname: x\n---")
            .into_iter()
            .map(|(f, _)| f)
            .collect();
        assert_eq!(froms, [0, 4, 12]);
    }

    #[test]
    fn marks_nothing_for_text_that_is_no_frontmatter() {
        for text in [
            "# Title\n",
            "",
            "---\n\nText.\n",
            "---\nthis is prose\n---\n",
            "---\nname: x\n\n# Title\n",
            "\n---\nname: x\n---\n",
        ] {
            assert_eq!(frontmatter_marks(text), vec![], "for {text:?}");
        }
    }
}
