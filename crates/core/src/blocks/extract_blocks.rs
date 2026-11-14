use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag};
use serde::{Deserialize, Serialize};

use super::frontmatter::blank_frontmatter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockKind {
    Heading,
    Paragraph,
    List,
    Blockquote,
    Code,
    Table,
    Html,
    Rule,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub id: String,
    pub from: usize,
    pub to: usize,
    pub text: String,
    pub kind: BlockKind,
    pub level: Option<u8>,
}

#[derive(Debug, Clone, Copy)]
struct Classification {
    kind: BlockKind,
    level: Option<u8>,
}

#[derive(Debug, Clone)]
struct Found {
    from: usize,
    to: usize,
    classification: Classification,
}

const fn simple(kind: BlockKind) -> Classification {
    Classification { kind, level: None }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn classify_tag(tag: &Tag, source: &str) -> Classification {
    match tag {
        Tag::Heading { level, .. } => Classification {
            kind: BlockKind::Heading,
            level: Some(heading_level(*level)),
        },
        Tag::Paragraph => simple(BlockKind::Paragraph),
        Tag::List(_) => simple(BlockKind::List),
        Tag::BlockQuote(_) => simple(BlockKind::Blockquote),
        Tag::CodeBlock(CodeBlockKind::Fenced(_) | CodeBlockKind::Indented) => {
            simple(BlockKind::Code)
        }
        Tag::Table(_) => simple(BlockKind::Table),
        Tag::HtmlBlock => classify_html(source),
        _ => simple(BlockKind::Other),
    }
}

fn classify_html(source: &str) -> Classification {
    let trimmed = source.trim_start_matches(' ');
    if trimmed.starts_with("<!--") || trimmed.starts_with("<?") {
        simple(BlockKind::Other)
    } else {
        simple(BlockKind::Html)
    }
}

fn trim_line_breaks(text: &str, from: usize, to: usize) -> usize {
    from + text[from..to].trim_end_matches(['\n', '\r']).len()
}

fn is_closed_fence(block_text: &str) -> bool {
    let mut lines = block_text.trim_end().lines();
    let (Some(first), Some(last)) = (lines.next(), lines.next_back()) else {
        return false;
    };
    let opening = first.trim_start();
    let Some(marker) = opening.chars().next().filter(|c| *c == '`' || *c == '~') else {
        return false;
    };
    let opening_length = opening.chars().take_while(|c| *c == marker).count();
    let closing = last.trim();
    closing.len() >= opening_length && closing.chars().all(|c| c == marker)
}

fn is_container(tag: &Tag) -> bool {
    matches!(tag, Tag::List(_) | Tag::Item | Tag::BlockQuote(_))
}

fn parse_top_level(text: &str) -> Vec<Found> {
    let options = Options::ENABLE_TABLES;
    let mut iterator = Parser::new_ext(text, options).into_offset_iter();
    let mut found: Vec<Found> = Vec::new();
    let mut depth = 0usize;
    let mut content_end = 0usize;
    for (event, range) in iterator.by_ref() {
        match event {
            Event::Start(tag) => {
                if depth == 0 {
                    let classification = classify_tag(&tag, &text[range.clone()]);
                    let to = match &tag {
                        Tag::CodeBlock(CodeBlockKind::Fenced(_))
                            if !is_closed_fence(&text[range.clone()]) =>
                        {
                            range.end
                        }
                        _ => trim_line_breaks(text, range.start, range.end),
                    };
                    found.push(Found {
                        from: range.start,
                        to,
                        classification,
                    });
                    content_end = range.start;
                }
                if !is_container(&tag) {
                    content_end = content_end.max(trim_line_breaks(text, range.start, range.end));
                }
                depth += 1;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 0
                    && let Some(last) = found.last_mut()
                    && matches!(
                        last.classification.kind,
                        BlockKind::List | BlockKind::Blockquote
                    )
                {
                    last.to = content_end.max(last.from);
                }
            }
            Event::Rule if depth == 0 => found.push(Found {
                from: range.start,
                to: trim_line_breaks(text, range.start, range.end),
                classification: simple(BlockKind::Rule),
            }),
            _ => {
                content_end = content_end.max(trim_line_breaks(text, range.start, range.end));
            }
        }
    }
    let definitions: Vec<std::ops::Range<usize>> = iterator
        .reference_definitions()
        .iter()
        .map(|(_, definition)| definition.span.clone())
        .collect();
    with_link_references(text, found, definitions)
}

fn line_groups(text: &str, from: usize, to: usize) -> Vec<(usize, usize)> {
    let mut offset = from;
    text[from..to]
        .split_inclusive('\n')
        .filter_map(|line| {
            let line_start = offset;
            offset += line.len();
            let content = line.trim_end_matches(['\n', '\r']);
            let trimmed = content.trim();
            (!trimmed.is_empty()).then(|| {
                let start = line_start + (content.len() - content.trim_start().len());
                (start, start + trimmed.len())
            })
        })
        .collect()
}

fn with_link_references(
    text: &str,
    found: Vec<Found>,
    definitions: Vec<std::ops::Range<usize>>,
) -> Vec<Found> {
    let mut gaps: Vec<(usize, usize)> = Vec::new();
    let mut cursor = 0;
    for block in &found {
        gaps.push((cursor, block.from));
        cursor = block.to.max(cursor);
    }
    gaps.push((cursor, text.len()));
    let mut extra: Vec<Found> = Vec::new();
    for (gap_from, gap_to) in gaps {
        if gap_from >= gap_to || text[gap_from..gap_to].trim().is_empty() {
            continue;
        }
        let mut spans: Vec<(usize, usize)> = definitions
            .iter()
            .filter(|span| span.start >= gap_from && span.end <= gap_to)
            .map(|span| (span.start, trim_line_breaks(text, span.start, span.end)))
            .collect();
        spans.sort_unstable();
        spans.dedup();
        let mut rest = gap_from;
        let push_leftover = |from: usize, to: usize, extra: &mut Vec<Found>| {
            extra.extend(line_groups(text, from, to).into_iter().map(|(a, b)| Found {
                from: a,
                to: b,
                classification: simple(BlockKind::Other),
            }));
        };
        for (from, to) in spans {
            if from < rest {
                continue;
            }
            push_leftover(rest, from, &mut extra);
            extra.push(Found {
                from,
                to,
                classification: simple(BlockKind::Other),
            });
            rest = to;
        }
        push_leftover(rest, gap_to, &mut extra);
    }
    let mut all = found;
    all.extend(extra);
    all.sort_by_key(|block| block.from);
    all
}

pub fn extract_blocks_from_blanked(blanked: &str, text: &str) -> Vec<Block> {
    parse_top_level(blanked)
        .into_iter()
        .enumerate()
        .map(|(index, found)| Block {
            id: format!("p{}", index + 1),
            from: found.from,
            to: found.to,
            text: text[found.from..found.to].to_string(),
            kind: found.classification.kind,
            level: found.classification.level,
        })
        .collect()
}

pub fn extract_blocks(text: &str) -> Vec<Block> {
    extract_blocks_from_blanked(&blank_frontmatter(text), text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::SAMPLE_DOCUMENT;

    fn kinds(blocks: &[Block]) -> Vec<BlockKind> {
        blocks.iter().map(|block| block.kind).collect()
    }

    fn texts(blocks: &[Block]) -> Vec<&str> {
        blocks.iter().map(|block| block.text.as_str()).collect()
    }

    fn ids(blocks: &[Block]) -> Vec<&str> {
        blocks.iter().map(|block| block.id.as_str()).collect()
    }

    #[test]
    fn returns_no_blocks_for_an_empty_document() {
        assert_eq!(extract_blocks(""), vec![]);
    }

    #[test]
    fn returns_no_blocks_for_a_whitespace_only_document() {
        assert_eq!(extract_blocks("\n\n  \n"), vec![]);
    }

    #[test]
    fn numbers_blocks_p1_to_pn_in_document_order() {
        let blocks = extract_blocks("# A\n\nOne.\n\nTwo.\n");
        assert_eq!(ids(&blocks), ["p1", "p2", "p3"]);
    }

    #[test]
    fn slices_each_block_text_from_the_document() {
        let doc = "# Title\n\nFirst paragraph.\n\nSecond paragraph.\n";
        let blocks = extract_blocks(doc);
        assert_eq!(
            texts(&blocks),
            ["# Title", "First paragraph.", "Second paragraph."]
        );
        for block in &blocks {
            assert_eq!(&doc[block.from..block.to], block.text);
        }
    }

    #[test]
    fn classifies_headings_with_their_level() {
        let blocks = extract_blocks("# One\n\n## Two\n\n### Three\n");
        let found: Vec<(BlockKind, Option<u8>)> = blocks
            .iter()
            .map(|block| (block.kind, block.level))
            .collect();
        assert_eq!(
            found,
            vec![
                (BlockKind::Heading, Some(1)),
                (BlockKind::Heading, Some(2)),
                (BlockKind::Heading, Some(3)),
            ]
        );
    }

    #[test]
    fn classifies_setext_headings() {
        let blocks = extract_blocks("Title\n=====\n");
        assert_eq!(
            (blocks[0].kind, blocks[0].level),
            (BlockKind::Heading, Some(1))
        );
        let second = extract_blocks("Sub\n---\n");
        assert_eq!(
            (second[0].kind, second[0].level),
            (BlockKind::Heading, Some(2))
        );
    }

    #[test]
    fn treats_a_whole_list_without_blank_lines_as_one_block() {
        let blocks = extract_blocks("Intro:\n\n- a\n- b\n- c\n\nOutro.\n");
        assert_eq!(
            kinds(&blocks),
            [BlockKind::Paragraph, BlockKind::List, BlockKind::Paragraph]
        );
        assert_eq!(blocks[1].text, "- a\n- b\n- c");
    }

    #[test]
    fn treats_a_loose_list_and_an_ordered_list_as_one_block_each() {
        let blocks = extract_blocks("- a\n\n- b\n\n1. one\n2. two\n");
        assert_eq!(kinds(&blocks), [BlockKind::List, BlockKind::List]);
        assert_eq!(blocks[0].text, "- a\n\n- b");
    }

    #[test]
    fn keeps_a_fenced_code_block_with_blank_lines_as_one_block() {
        let doc = "Before.\n\n```js\nconst a = 1\n\n\nconst b = 2\n```\n\nAfter.\n";
        let blocks = extract_blocks(doc);
        assert_eq!(
            kinds(&blocks),
            [BlockKind::Paragraph, BlockKind::Code, BlockKind::Paragraph]
        );
        assert_eq!(blocks[1].text, "```js\nconst a = 1\n\n\nconst b = 2\n```");
    }

    #[test]
    fn classifies_an_indented_code_block_as_code() {
        let blocks = extract_blocks("Text.\n\n    indented()\n");
        assert_eq!(kinds(&blocks), [BlockKind::Paragraph, BlockKind::Code]);
    }

    #[test]
    fn classifies_blockquotes() {
        let blocks = extract_blocks("> quoted\n> still quoted\n");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].kind, BlockKind::Blockquote);
        assert_eq!(blocks[0].text, "> quoted\n> still quoted");
    }

    #[test]
    fn classifies_tables_html_rules_and_link_references() {
        let blocks = extract_blocks(
            "| a | b |\n|---|---|\n| 1 | 2 |\n\n<div>hi</div>\n\n***\n\n[ref]: https://example.com\n",
        );
        assert_eq!(
            kinds(&blocks),
            [
                BlockKind::Table,
                BlockKind::Html,
                BlockKind::Rule,
                BlockKind::Other
            ]
        );
    }

    mod with_a_frontmatter {
        use super::*;

        const SKILL: &str = "---\nname: demo\ndescription: Does a thing.\n---\n\n# Demo\n\nBody.\n";

        #[test]
        fn skips_it() {
            let blocks = extract_blocks(SKILL);
            assert_eq!(texts(&blocks), ["# Demo", "Body."]);
        }

        #[test]
        fn numbers_blocks_from_p1_after_it() {
            assert_eq!(ids(&extract_blocks(SKILL)), ["p1", "p2"]);
        }

        #[test]
        fn keeps_document_offsets() {
            for block in extract_blocks(SKILL) {
                assert_eq!(&SKILL[block.from..block.to], block.text);
            }
        }

        #[test]
        fn skips_a_frontmatter_that_is_the_whole_document() {
            assert_eq!(extract_blocks("---\nname: demo\n---\n"), vec![]);
            assert_eq!(extract_blocks("---\nname: demo\n---"), vec![]);
        }

        #[test]
        fn skips_an_empty_frontmatter() {
            let blocks = extract_blocks("---\n---\n\nBody.\n");
            assert_eq!(blocks.len(), 1);
            assert_eq!(blocks[0].id, "p1");
            assert_eq!(blocks[0].text, "Body.");
        }

        #[test]
        fn finds_the_block_right_after_it_with_no_blank_line_between() {
            let blocks = extract_blocks("---\nname: demo\n---\n# Demo\n");
            assert_eq!(blocks.len(), 1);
            assert_eq!(blocks[0].id, "p1");
            assert_eq!(blocks[0].kind, BlockKind::Heading);
            assert_eq!(blocks[0].level, Some(1));
            assert_eq!(blocks[0].text, "# Demo");
        }

        #[test]
        fn does_not_let_yaml_that_looks_like_markdown_leak_into_blocks() {
            let doc = "---\ndescription: |\n  # not a heading\n\n  - not a list\n---\n\nBody.\n";
            assert_eq!(texts(&extract_blocks(doc)), ["Body."]);
        }

        #[test]
        fn does_not_let_an_unclosed_fence_in_the_yaml_swallow_the_document() {
            let doc = "---\nnote: |\n  ```\n---\n\n# Demo\n\nBody.\n";
            assert_eq!(texts(&extract_blocks(doc)), ["# Demo", "Body."]);
        }

        #[test]
        fn keeps_blocks_when_the_frontmatter_is_not_closed() {
            let blocks = extract_blocks("---\nname: demo\n\nBody.\n");
            assert!(texts(&blocks).contains(&"Body."));
        }

        #[test]
        fn keeps_the_blocks_of_a_document_that_starts_with_a_plain_rule() {
            let blocks = extract_blocks("---\n\nIntro.\n\n---\n\nRest.\n");
            let found: Vec<(&str, BlockKind, &str)> = blocks
                .iter()
                .map(|block| (block.id.as_str(), block.kind, block.text.as_str()))
                .collect();
            assert_eq!(
                found,
                vec![
                    ("p1", BlockKind::Rule, "---"),
                    ("p2", BlockKind::Paragraph, "Intro."),
                    ("p3", BlockKind::Rule, "---"),
                    ("p4", BlockKind::Paragraph, "Rest."),
                ]
            );
        }

        #[test]
        fn keeps_a_heading_between_two_plain_rules() {
            let blocks = extract_blocks("---\n# Title\n---\n\nBody.\n");
            assert_eq!(
                kinds(&blocks),
                [
                    BlockKind::Rule,
                    BlockKind::Heading,
                    BlockKind::Rule,
                    BlockKind::Paragraph
                ]
            );
        }

        #[test]
        fn does_not_skip_a_rule_that_is_not_at_the_start() {
            let blocks = extract_blocks("Intro.\n\n---\n\nBody.\n");
            assert_eq!(
                kinds(&blocks),
                [BlockKind::Paragraph, BlockKind::Rule, BlockKind::Paragraph]
            );
        }

        #[test]
        fn reads_blocks_from_the_text_with_the_frontmatter_blanked() {
            let text = "---\nnote: |\n  ```\n---\n\n# Demo\n";
            let blocks = extract_blocks_from_blanked(&blank_frontmatter(text), text);
            assert_eq!(texts(&blocks), ["# Demo"]);
        }
    }

    #[test]
    fn extracts_the_sample_document() {
        let blocks = extract_blocks(SAMPLE_DOCUMENT);
        assert_eq!(
            kinds(&blocks),
            [
                BlockKind::Heading,
                BlockKind::Heading,
                BlockKind::Paragraph,
                BlockKind::Paragraph,
                BlockKind::Paragraph,
                BlockKind::Heading,
                BlockKind::Paragraph,
                BlockKind::List,
                BlockKind::Paragraph,
                BlockKind::Paragraph,
                BlockKind::Code,
                BlockKind::Paragraph,
                BlockKind::Paragraph,
            ]
        );
        assert_eq!(blocks[7].text.split('\n').count(), 4);
        assert!(blocks[10].text.starts_with("```bash"));
        assert!(blocks[11].text.contains("recieve"));
    }
}
