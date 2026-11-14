use serde::{Deserialize, Serialize};

use crate::text::{js_trim, js_trim_start};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frontmatter {
    pub from: usize,
    pub to: usize,
    pub yaml_from: usize,
    pub yaml_to: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub from: usize,
    pub to: usize,
}

fn is_blank_char(c: char) -> bool {
    c == ' ' || c == '\t'
}

fn skip_blanks(text: &str, index: usize) -> usize {
    index
        + text[index..]
            .chars()
            .take_while(|c| is_blank_char(*c))
            .count()
}

fn closing_delimiter_end(text: &str, index: usize) -> Option<usize> {
    if !text[index..].starts_with("---") {
        return None;
    }
    let end = skip_blanks(text, index + 3);
    (end == text.len() || text[end..].starts_with('\n')).then_some(end)
}

struct Match {
    end: usize,
    yaml_from: usize,
    yaml_to: usize,
}

fn match_frontmatter(text: &str) -> Option<Match> {
    if !text.starts_with("---") {
        return None;
    }
    let after_opening = skip_blanks(text, 3);
    if !text[after_opening..].starts_with('\n') {
        return None;
    }
    let yaml_from = after_opening + 1;
    let with_content = text[yaml_from..]
        .char_indices()
        .filter(|(_, c)| *c == '\n')
        .map(|(offset, _)| yaml_from + offset)
        .find_map(|newline| {
            closing_delimiter_end(text, newline + 1).map(|end| Match {
                end,
                yaml_from,
                yaml_to: newline,
            })
        });
    with_content.or_else(|| {
        closing_delimiter_end(text, yaml_from).map(|end| Match {
            end,
            yaml_from,
            yaml_to: yaml_from,
        })
    })
}

fn is_key_char(c: char) -> bool {
    c.is_alphabetic() || c.is_numeric() || c == '_' || c == '.' || c == '-'
}

fn key_length(line: &str) -> Option<usize> {
    let first = line.chars().next()?;
    if first == '"' || first == '\'' {
        let close = line[1..].find(first)?;
        return Some(close + 2);
    }
    let length: usize = line
        .chars()
        .take_while(|c| is_key_char(*c))
        .map(char::len_utf8)
        .sum();
    (length > 0).then_some(length)
}

fn is_mapping_key(line: &str) -> bool {
    let Some(length) = key_length(line) else {
        return false;
    };
    let after_blanks = skip_blanks(line, length);
    let Some(rest) = line[after_blanks..].strip_prefix(':') else {
        return false;
    };
    rest.is_empty() || rest.starts_with(is_blank_char)
}

fn looks_like_yaml(yaml: &str) -> bool {
    if js_trim(yaml).is_empty() {
        return true;
    }
    yaml.split('\n')
        .find(|line| !js_trim(line).is_empty() && !js_trim_start(line).starts_with('#'))
        .is_some_and(is_mapping_key)
}

pub fn find_frontmatter(text: &str) -> Option<Frontmatter> {
    let found = match_frontmatter(text)?;
    looks_like_yaml(&text[found.yaml_from..found.yaml_to]).then_some(Frontmatter {
        from: 0,
        to: found.end,
        yaml_from: found.yaml_from,
        yaml_to: found.yaml_to,
    })
}

pub fn touches_frontmatter(frontmatter: Option<&Frontmatter>, range: Range) -> bool {
    frontmatter.is_some_and(|found| {
        range.from < found.to || (range.from == range.to && range.from == found.to)
    })
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

    mod find_frontmatter_tests {
        use super::*;

        #[test]
        fn finds_a_frontmatter_at_the_start_of_the_document() {
            let found = find_frontmatter(SKILL).unwrap();
            assert_eq!(
                &SKILL[found.from..found.to],
                "---\nname: demo\ndescription: Does a thing.\n---"
            );
        }

        #[test]
        fn exposes_the_yaml_between_the_delimiters() {
            let found = find_frontmatter(SKILL).unwrap();
            assert_eq!(
                &SKILL[found.yaml_from..found.yaml_to],
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
            let found = find_frontmatter(text).unwrap();
            assert_eq!(&text[found.yaml_from..found.yaml_to], "name: demo");
            assert_eq!(&text[found.from..found.to], "---  \nname: demo\n---\t");
        }

        #[test]
        fn ends_at_the_closing_delimiter_before_its_line_break() {
            assert_eq!(find_frontmatter("---\na: 1\n---\nBody.").unwrap().to, 12);
        }

        #[test]
        fn accepts_a_closing_delimiter_at_the_very_end() {
            let text = "---\nname: demo\n---";
            assert_eq!(find_frontmatter(text).unwrap().to, text.len());
        }

        #[test]
        fn stops_at_the_first_closing_delimiter() {
            let text = "---\na: 1\n---\n\nBody.\n\n---\n\nMore.\n";
            assert_eq!(
                &text[..find_frontmatter(text).unwrap().to],
                "---\na: 1\n---"
            );
        }

        #[test]
        fn finds_an_empty_frontmatter() {
            let adjacent = find_frontmatter("---\n---\n\nBody.\n").unwrap();
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
            let found = find_frontmatter(blank).unwrap();
            assert_eq!(found.to, 8);
            assert_eq!(&blank[found.yaml_from..found.yaml_to], "");
        }

        #[test]
        fn finds_a_document_that_is_only_a_frontmatter() {
            let text = "---\nname: demo\n---\n";
            assert_eq!(find_frontmatter(text).unwrap().to, text.len() - 1);
        }

        #[test]
        fn keeps_blank_lines_and_yaml_that_looks_like_markdown_inside_it() {
            let text =
                "---\ndescription: |\n  # not a heading\n\n  - not a list\n```\n---\n\nBody.\n";
            let found = find_frontmatter(text).unwrap();
            assert_eq!(
                &text[found.yaml_from..found.yaml_to],
                "description: |\n  # not a heading\n\n  - not a list\n```"
            );
        }

        mod between_two_plain_rules {
            use super::*;

            #[test]
            fn is_not_a_frontmatter_when_the_first_line_is_prose() {
                assert_eq!(find_frontmatter("---\n\nIntro.\n\n---\n\nRest.\n"), None);
                assert_eq!(find_frontmatter("---\nJust a paragraph.\n---\n"), None);
                assert_eq!(find_frontmatter("---\nNote that: it works\n---\n"), None);
            }

            #[test]
            fn is_not_a_frontmatter_when_the_first_line_is_a_list_item_or_a_heading() {
                assert_eq!(find_frontmatter("---\n- one\n- two\n---\n"), None);
                assert_eq!(find_frontmatter("---\n## Heading\ntext\n---\n"), None);
            }

            #[test]
            fn is_not_a_frontmatter_when_there_are_only_comments() {
                assert_eq!(find_frontmatter("---\n# Title\n---\n"), None);
                assert_eq!(find_frontmatter("---\n# one\n\n  # two\n---\n"), None);
            }

            #[test]
            fn is_not_a_frontmatter_when_comments_are_followed_by_prose() {
                assert_eq!(find_frontmatter("---\n# Title\nIntro.\n---\n"), None);
            }

            #[test]
            fn needs_a_space_or_the_end_of_the_line_after_the_colon() {
                assert_eq!(find_frontmatter("---\nhttps://example.com\n---\n"), None);
                assert_eq!(find_frontmatter("---\nname:demo\n---\n"), None);
                assert!(find_frontmatter("---\nname:\n---\n").is_some());
                assert!(find_frontmatter("---\nname:\tdemo\n---\n").is_some());
            }
        }

        mod real_frontmatters {
            use super::*;

            #[test]
            fn opens_with_a_comment_line() {
                let text = "---\n# Managed by hand\nname: demo\n---\n\nBody.\n";
                assert_eq!(
                    find_frontmatter(text).unwrap().to,
                    text.find("\n\nBody.").unwrap()
                );
            }

            #[test]
            fn opens_with_blank_lines_and_comments() {
                let text = "---\n\n  # note\n# another\n\nname: demo\n---\n";
                assert!(find_frontmatter(text).is_some());
            }

            #[test]
            fn has_a_quoted_or_hyphenated_first_key() {
                assert!(find_frontmatter("---\n\"name\": demo\n---\n").is_some());
                assert!(find_frontmatter("---\n'name': demo\n---\n").is_some());
                assert!(find_frontmatter("---\nallowed-tools: Read\n---\n").is_some());
                assert!(find_frontmatter("---\nnome-da-habilidade: x\n---\n").is_some());
            }

            #[test]
            fn is_empty_or_only_blank_lines() {
                assert!(find_frontmatter("---\n---\n").is_some());
                assert!(find_frontmatter("---\n\n\n---\n").is_some());
                assert!(find_frontmatter("---\n \t\n---\n").is_some());
            }

            #[test]
            fn has_a_first_key_with_a_block_value() {
                let text = "---\ndescription: >\n  Prose: with a colon.\n---\n";
                assert!(find_frontmatter(text).is_some());
            }
        }

        #[test]
        fn does_not_treat_a_lone_cr_as_a_line_break() {
            assert_eq!(find_frontmatter("---\r\nname: demo\r\n---\r\n"), None);
        }
    }

    mod touches_frontmatter_tests {
        use super::*;

        fn range(from: usize, to: usize) -> Range {
            Range { from, to }
        }

        #[test]
        fn refuses_a_cursor_inside_it_including_the_end_of_the_closing_line() {
            let frontmatter = find_frontmatter(SKILL);
            let end = frontmatter.unwrap().to;
            assert!(touches_frontmatter(frontmatter.as_ref(), range(0, 0)));
            assert!(touches_frontmatter(frontmatter.as_ref(), range(8, 8)));
            assert!(touches_frontmatter(frontmatter.as_ref(), range(end, end)));
        }

        #[test]
        fn refuses_a_range_inside_or_overlapping_it() {
            let frontmatter = find_frontmatter(SKILL);
            let end = frontmatter.unwrap().to;
            assert!(touches_frontmatter(frontmatter.as_ref(), range(4, 10)));
            assert!(touches_frontmatter(frontmatter.as_ref(), range(4, end + 6)));
            assert!(touches_frontmatter(
                frontmatter.as_ref(),
                range(0, SKILL.len())
            ));
        }

        #[test]
        fn allows_a_range_that_starts_after_it() {
            let frontmatter = find_frontmatter(SKILL);
            let end = frontmatter.unwrap().to;
            assert!(!touches_frontmatter(
                frontmatter.as_ref(),
                range(end + 1, end + 1)
            ));
            assert!(!touches_frontmatter(
                frontmatter.as_ref(),
                range(end + 2, end + 8)
            ));
        }

        #[test]
        fn allows_a_range_that_only_starts_where_the_frontmatter_ends() {
            let frontmatter = find_frontmatter(SKILL);
            let end = frontmatter.unwrap().to;
            assert!(!touches_frontmatter(
                frontmatter.as_ref(),
                range(end, end + 8)
            ));
        }

        #[test]
        fn allows_anything_when_there_is_no_frontmatter() {
            assert!(!touches_frontmatter(None, range(0, 5)));
        }
    }

    mod blank_frontmatter_tests {
        use super::*;

        #[test]
        fn blanks_the_frontmatter_and_keeps_every_offset() {
            let blanked = blank_frontmatter(SKILL);
            let end = find_frontmatter(SKILL).unwrap().to;
            assert_eq!(blanked.len(), SKILL.len());
            assert_eq!(&blanked[end..], &SKILL[end..]);
            assert_eq!(js_trim(&blanked[..end]), "");
        }

        #[test]
        fn returns_the_text_unchanged_without_a_frontmatter() {
            let text = "# Title\n\nBody.\n";
            assert_eq!(blank_frontmatter(text), text);
        }
    }
}
