use serde::{Deserialize, Serialize};

use super::extract_blocks::{Block, BlockKind};
use super::frontmatter::{Range, find_frontmatter, touches_frontmatter};
use crate::text::js_trim;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub from: usize,
    pub to: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetSource {
    Selection,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub source: TargetSource,
    pub from: usize,
    pub to: usize,
    pub text: String,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Neighbors {
    pub title: Option<String>,
    pub section: Option<String>,
    pub before: Vec<Block>,
    pub after: Vec<Block>,
}

const NEIGHBOR_COUNT: usize = 2;

fn slice(text: &str, from: usize, to: usize) -> &str {
    text.get(from..to).unwrap_or("")
}

pub fn find_block_at(blocks: &[Block], position: usize) -> Option<&Block> {
    blocks
        .iter()
        .find(|block| block.from <= position && position <= block.to)
}

fn distance_to(block: &Block, position: usize) -> usize {
    if position < block.from {
        block.from - position
    } else {
        position - block.to
    }
}

fn nearest_block(blocks: &[Block], position: usize) -> Option<&Block> {
    blocks.iter().fold(None, |best, block| match best {
        Some(current) if distance_to(block, position) >= distance_to(current, position) => {
            Some(current)
        }
        _ => Some(block),
    })
}

fn skip_backward(bytes: &[u8], end: usize, matches: impl Fn(u8) -> bool) -> usize {
    let mut index = end;
    while index > 0 && matches(bytes[index - 1]) {
        index -= 1;
    }
    index
}

fn is_blank(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

fn is_hash(byte: u8) -> bool {
    byte == b'#'
}

fn is_line_break(byte: u8) -> bool {
    byte == b'\r' || byte == b'\n'
}

fn strip_closing_hashes(line: &str) -> &str {
    let bytes = line.as_bytes();
    let after_hashes = skip_backward(bytes, bytes.len(), is_blank);
    let before_hashes = skip_backward(bytes, after_hashes, is_hash);
    let before_space = skip_backward(bytes, before_hashes, is_blank);
    if before_hashes < after_hashes && before_space < before_hashes {
        &line[..before_space]
    } else {
        line
    }
}

fn count_trailing_breaks(text: &str) -> usize {
    let bytes = text.as_bytes();
    bytes.len() - skip_backward(bytes, bytes.len(), is_line_break)
}

fn strip_atx_opening(line: &str) -> &str {
    let hashes = line.bytes().take_while(|byte| *byte == b'#').count().min(6);
    line[hashes..].trim_start_matches([' ', '\t'])
}

pub fn heading_text(block: &Block) -> String {
    let lines: Vec<&str> = block.text.split('\n').collect();
    let content = match lines.first() {
        Some(first) if first.starts_with('#') => {
            strip_closing_hashes(strip_atx_opening(first)).to_string()
        }
        _ => lines[..lines.len() - 1].join(" "),
    };
    js_trim(&content).to_string()
}

fn overlaps(block: &Block, from: usize, to: usize) -> bool {
    block.from < to && block.to > from
}

fn blocks_overlapping(blocks: &[Block], from: usize, to: usize) -> Vec<Block> {
    blocks
        .iter()
        .filter(|block| overlaps(block, from, to))
        .cloned()
        .collect()
}

fn resolve_selection_target(blocks: &[Block], text: &str, selection: Selection) -> Option<Target> {
    let selected = slice(text, selection.from, selection.to);
    if js_trim(selected).is_empty() {
        return None;
    }
    let leading = selected.len() - selected.trim_start_matches(['\r', '\n']).len();
    let from = selection.from + leading;
    let to = selection.to - count_trailing_breaks(selected);
    Some(Target {
        source: TargetSource::Selection,
        from,
        to,
        text: slice(text, from, to).to_string(),
        blocks: blocks_overlapping(blocks, from, to),
    })
}

fn resolve_cursor_target(blocks: &[Block], position: usize) -> Option<Target> {
    find_block_at(blocks, position)
        .or_else(|| nearest_block(blocks, position))
        .map(|block| Target {
            source: TargetSource::Block,
            from: block.from,
            to: block.to,
            text: block.text.clone(),
            blocks: vec![block.clone()],
        })
}

pub fn target_from_range(
    blocks: &[Block],
    text: &str,
    selection: Selection,
    source: TargetSource,
) -> Target {
    Target {
        source,
        from: selection.from,
        to: selection.to,
        text: slice(text, selection.from, selection.to).to_string(),
        blocks: blocks_overlapping(blocks, selection.from, selection.to),
    }
}

pub fn resolve_target(blocks: &[Block], text: &str, selection: Selection) -> Option<Target> {
    let to = text.len().min(selection.from.max(selection.to));
    let from = selection.from.min(selection.to).min(to);
    let frontmatter = find_frontmatter(text);
    if from == to {
        return if touches_frontmatter(frontmatter.as_ref(), Range { from, to }) {
            None
        } else {
            resolve_cursor_target(blocks, from)
        };
    }
    let start = match frontmatter {
        Some(found) if from < found.to => found.to + 1,
        _ => from,
    };
    if start >= to {
        None
    } else {
        resolve_selection_target(blocks, text, Selection { from: start, to })
    }
}

fn anchor_range(target: &Target) -> Selection {
    Selection {
        from: target
            .blocks
            .iter()
            .map(|block| block.from)
            .fold(target.from, usize::min),
        to: target
            .blocks
            .iter()
            .map(|block| block.to)
            .fold(target.to, usize::max),
    }
}

pub fn get_neighbors(blocks: &[Block], target: &Target) -> Neighbors {
    let Selection { from, to } = anchor_range(target);
    let preceding: Vec<&Block> = blocks.iter().filter(|block| block.to <= from).collect();
    let following: Vec<&Block> = blocks.iter().filter(|block| block.from >= to).collect();
    let title = blocks
        .iter()
        .find(|block| block.kind == BlockKind::Heading && block.level == Some(1));
    let section = preceding
        .iter()
        .rev()
        .find(|block| block.kind == BlockKind::Heading);
    Neighbors {
        title: title.map(heading_text),
        section: section.map(|block| heading_text(block)),
        before: preceding
            .iter()
            .skip(preceding.len().saturating_sub(NEIGHBOR_COUNT))
            .map(|block| (*block).clone())
            .collect(),
        after: following
            .iter()
            .take(NEIGHBOR_COUNT)
            .map(|block| (*block).clone())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::extract_blocks;
    use crate::test_support::SAMPLE_DOCUMENT;

    fn doc() -> String {
        [
            "# Title",
            "",
            "Intro.",
            "",
            "## Section A",
            "",
            "A one.",
            "",
            "A two.",
            "",
            "A three.",
            "",
            "## Section B",
            "",
            "B one.",
            "",
            "B two.",
            "",
        ]
        .join("\n")
    }

    fn by_text(blocks: &[Block], text: &str) -> Block {
        blocks
            .iter()
            .find(|candidate| candidate.text == text)
            .unwrap_or_else(|| panic!("no block {text}"))
            .clone()
    }

    fn sel(from: usize, to: usize) -> Selection {
        Selection { from, to }
    }

    fn block_texts(blocks: &[Block]) -> Vec<&str> {
        blocks.iter().map(|block| block.text.as_str()).collect()
    }

    fn block_ids(blocks: &[Block]) -> Vec<&str> {
        blocks.iter().map(|block| block.id.as_str()).collect()
    }

    mod find_block_at_tests {
        use super::*;

        #[test]
        fn returns_the_block_containing_the_position() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let block = by_text(&blocks, "A two.");
            assert_eq!(
                find_block_at(&blocks, block.from + 2).map(|found| &found.id),
                Some(&block.id)
            );
        }

        #[test]
        fn includes_both_edges_of_a_block() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let block = by_text(&blocks, "A two.");
            assert_eq!(
                find_block_at(&blocks, block.from).map(|found| &found.id),
                Some(&block.id)
            );
            assert_eq!(
                find_block_at(&blocks, block.to).map(|found| &found.id),
                Some(&block.id)
            );
        }

        #[test]
        fn returns_none_between_blocks() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let first = by_text(&blocks, "A one.");
            assert_eq!(find_block_at(&blocks, first.to + 1), None);
        }

        #[test]
        fn returns_none_for_an_empty_document() {
            assert_eq!(find_block_at(&[], 0), None);
        }
    }

    mod heading_text_tests {
        use super::*;

        #[test]
        fn strips_atx_markers() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            assert_eq!(heading_text(&by_text(&blocks, "## Section A")), "Section A");
        }

        #[test]
        fn strips_closing_atx_markers() {
            let blocks = extract_blocks("## Closed ##\n");
            assert_eq!(heading_text(&blocks[0]), "Closed");
        }

        #[test]
        fn strips_atx_markers_from_each_source() {
            let cases = [
                ("## Closed \t##  \t", "Closed"),
                ("## Closed ###", "Closed"),
                ("## Closed#", "Closed#"),
                ("## A # B", "A # B"),
                ("## A #  B #", "A #  B"),
                ("##   ", ""),
                ("## #", "#"),
            ];
            for (source, expected) in cases {
                let blocks = extract_blocks(&format!("{source}\n"));
                assert_eq!(heading_text(&blocks[0]), expected, "{source:?}");
            }
        }

        #[test]
        fn drops_the_setext_underline() {
            let blocks = extract_blocks("Setext title\n============\n");
            assert_eq!(heading_text(&blocks[0]), "Setext title");
        }
    }

    mod resolve_target_tests {
        use super::*;

        #[test]
        fn uses_the_selection_when_it_is_not_empty() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let block = by_text(&blocks, "A two.");
            let target =
                resolve_target(&blocks, &doc, sel(block.from + 2, block.from + 5)).unwrap();
            assert_eq!(target.source, TargetSource::Selection);
            assert_eq!(target.from, block.from + 2);
            assert_eq!(target.to, block.from + 5);
            assert_eq!(target.text, "two");
            assert_eq!(block_ids(&target.blocks), [block.id.as_str()]);
        }

        #[test]
        fn lists_every_block_a_selection_overlaps() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let first = by_text(&blocks, "A one.");
            let second = by_text(&blocks, "A two.");
            let target =
                resolve_target(&blocks, &doc, sel(first.from + 3, second.from + 3)).unwrap();
            assert_eq!(
                block_ids(&target.blocks),
                [first.id.as_str(), second.id.as_str()]
            );
        }

        #[test]
        fn uses_the_block_under_the_cursor_when_nothing_is_selected() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let block = by_text(&blocks, "A two.");
            let target =
                resolve_target(&blocks, &doc, sel(block.from + 1, block.from + 1)).unwrap();
            assert_eq!(target.source, TargetSource::Block);
            assert_eq!(target.from, block.from);
            assert_eq!(target.to, block.to);
            assert_eq!(target.text, "A two.");
            assert_eq!(target.blocks, vec![block]);
        }

        #[test]
        fn picks_the_nearest_block_when_the_cursor_sits_between_blocks() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let first = by_text(&blocks, "A one.");
            let second = by_text(&blocks, "A two.");
            let target = resolve_target(&blocks, &doc, sel(first.to + 1, first.to + 1)).unwrap();
            assert!(
                [first.id.as_str(), second.id.as_str()].contains(&target.blocks[0].id.as_str())
            );
            assert_eq!(target.source, TargetSource::Block);
        }

        #[test]
        fn prefers_the_previous_block_when_the_cursor_is_equidistant() {
            let doc = "abc\n\ndef\n";
            let blocks = extract_blocks(doc);
            let target = resolve_target(&blocks, doc, sel(4, 4)).unwrap();
            assert_eq!(target.text, "abc");
        }

        #[test]
        fn returns_none_for_an_empty_document() {
            assert_eq!(resolve_target(&[], "", sel(0, 0)), None);
        }

        #[test]
        fn returns_none_for_a_whitespace_only_selection() {
            let doc = "abc\n\n\n\ndef\n";
            let blocks = extract_blocks(doc);
            assert_eq!(resolve_target(&blocks, doc, sel(4, 7)), None);
        }

        #[test]
        fn normalises_a_reversed_selection() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let block = by_text(&blocks, "A two.");
            let target =
                resolve_target(&blocks, &doc, sel(block.from + 5, block.from + 2)).unwrap();
            assert_eq!(target.text, "two");
        }
    }

    mod get_neighbors_tests {
        use super::*;

        fn target_for(blocks: &[Block], doc: &str, text: &str) -> Target {
            let block = by_text(blocks, text);
            resolve_target(blocks, doc, sel(block.from, block.from)).expect("no target")
        }

        #[test]
        fn takes_2_blocks_before_and_2_after_the_target() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let neighbors = get_neighbors(&blocks, &target_for(&blocks, &doc, "A two."));
            assert_eq!(block_texts(&neighbors.before), ["## Section A", "A one."]);
            assert_eq!(block_texts(&neighbors.after), ["A three.", "## Section B"]);
        }

        #[test]
        fn returns_fewer_blocks_at_the_document_edges() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let first = get_neighbors(&blocks, &target_for(&blocks, &doc, "# Title"));
            assert_eq!(first.before, vec![]);
            assert_eq!(block_texts(&first.after), ["Intro.", "## Section A"]);
            let last = get_neighbors(&blocks, &target_for(&blocks, &doc, "B two."));
            assert_eq!(last.after, vec![]);
        }

        #[test]
        fn reports_the_document_title_and_the_current_section_heading() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let neighbors = get_neighbors(&blocks, &target_for(&blocks, &doc, "B one."));
            assert_eq!(neighbors.title.as_deref(), Some("Title"));
            assert_eq!(neighbors.section.as_deref(), Some("Section B"));
        }

        #[test]
        fn uses_the_nearest_heading_before_the_target_as_the_section() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let neighbors = get_neighbors(&blocks, &target_for(&blocks, &doc, "A three."));
            assert_eq!(neighbors.section.as_deref(), Some("Section A"));
        }

        #[test]
        fn has_no_section_before_the_first_heading() {
            let doc = "Preamble.\n\n# Title\n\nBody.\n";
            let blocks = extract_blocks(doc);
            let target = resolve_target(&blocks, doc, sel(0, 0)).expect("no target");
            let neighbors = get_neighbors(&blocks, &target);
            assert_eq!(neighbors.section, None);
            assert_eq!(neighbors.title.as_deref(), Some("Title"));
        }

        #[test]
        fn has_no_title_when_the_document_has_no_heading_1() {
            let doc = "## Only a subsection\n\nBody.\n";
            let blocks = extract_blocks(doc);
            let target = resolve_target(&blocks, doc, sel(25, 25)).expect("no target");
            assert_eq!(get_neighbors(&blocks, &target).title, None);
        }

        #[test]
        fn excludes_every_overlapped_block_from_the_neighbors_of_a_selection() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let first = by_text(&blocks, "A one.");
            let second = by_text(&blocks, "A two.");
            let target = resolve_target(&blocks, &doc, sel(first.from + 3, second.from + 3))
                .expect("no target");
            let neighbors = get_neighbors(&blocks, &target);
            assert_eq!(block_texts(&neighbors.before), ["Intro.", "## Section A"]);
            assert_eq!(block_texts(&neighbors.after), ["A three.", "## Section B"]);
        }

        #[test]
        fn works_on_the_sample_document() {
            let blocks = extract_blocks(SAMPLE_DOCUMENT);
            let slang = &blocks[4];
            let target = resolve_target(
                &blocks,
                SAMPLE_DOCUMENT,
                sel(slang.from + 10, slang.from + 10),
            )
            .expect("no target");
            let neighbors = get_neighbors(&blocks, &target);
            assert_eq!(
                neighbors.title.as_deref(),
                Some("Release process update: weekday deploys")
            );
            assert_eq!(neighbors.section.as_deref(), Some("Why we are changing"));
            assert_eq!(block_ids(&neighbors.before), ["p3", "p4"]);
            assert_eq!(block_ids(&neighbors.after), ["p6", "p7"]);
        }
    }

    mod line_breaks_at_the_edges_of_a_selection {
        use super::*;

        #[test]
        fn drops_the_line_break_a_triple_click_leaves_at_the_end() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let block = by_text(&blocks, "A two.");
            let target = resolve_target(&blocks, &doc, sel(block.from, block.to + 1)).unwrap();
            assert_eq!(target.source, TargetSource::Selection);
            assert_eq!(target.from, block.from);
            assert_eq!(target.to, block.to);
            assert_eq!(target.text, "A two.");
            assert_eq!(block_texts(&target.blocks), ["A two."]);
        }

        #[test]
        fn drops_line_breaks_at_the_start_and_several_at_the_end() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let block = by_text(&blocks, "A two.");
            let target = resolve_target(&blocks, &doc, sel(block.from - 2, block.to + 2)).unwrap();
            assert_eq!(target.from, block.from);
            assert_eq!(target.to, block.to);
            assert_eq!(target.text, "A two.");
            assert_eq!(target.blocks.len(), 1);
        }

        #[test]
        fn keeps_line_breaks_inside_the_selection_and_spaces_at_the_edges() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let first = by_text(&blocks, "A one.");
            let block = by_text(&blocks, "A two.");
            let target = resolve_target(&blocks, &doc, sel(first.from, block.to + 1)).unwrap();
            assert_eq!(target.text, "A one.\n\nA two.");
            let spaced = resolve_target(&blocks, " x \n", sel(0, 4)).unwrap();
            assert_eq!(spaced.text, " x ");
            assert_eq!(spaced.from, 0);
            assert_eq!(spaced.to, 3);
        }

        #[test]
        fn has_no_target_for_a_selection_of_only_line_breaks() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            assert_eq!(resolve_target(&blocks, &doc, sel(7, 9)), None);
        }
    }

    mod target_from_range_tests {
        use super::*;

        #[test]
        fn rebuilds_a_selection_target_from_its_range() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let block = by_text(&blocks, "A two.");
            let rebuilt = target_from_range(
                &blocks,
                &doc,
                sel(block.from + 2, block.to),
                TargetSource::Selection,
            );
            let original = resolve_target(&blocks, &doc, sel(block.from + 2, block.to));
            assert_eq!(Some(rebuilt), original);
        }

        #[test]
        fn rebuilds_a_block_target_from_the_block_range() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let block = by_text(&blocks, "A two.");
            let rebuilt = target_from_range(
                &blocks,
                &doc,
                sel(block.from, block.to),
                TargetSource::Block,
            );
            assert_eq!(
                Some(rebuilt),
                resolve_target(&blocks, &doc, sel(block.from + 1, block.from + 1))
            );
        }

        #[test]
        fn lists_every_block_the_range_overlaps() {
            let doc = doc();
            let blocks = extract_blocks(&doc);
            let first = by_text(&blocks, "A one.");
            let second = by_text(&blocks, "A two.");
            let rebuilt = target_from_range(
                &blocks,
                &doc,
                sel(first.from, second.to),
                TargetSource::Selection,
            );
            assert_eq!(block_texts(&rebuilt.blocks), ["A one.", "A two."]);
        }
    }

    mod with_a_frontmatter {
        use super::*;

        const DOC: &str = "---\nname: demo\ndescription: Does a thing.\n---\n\n# Demo\n\nBody one.\n\nBody two.\n";

        fn end() -> usize {
            DOC.find("---\n\n").unwrap() + 3
        }

        fn body_one() -> usize {
            DOC.find("Body one.").unwrap()
        }

        fn resolve(from: usize, to: usize) -> Option<Target> {
            resolve_target(&extract_blocks(DOC), DOC, sel(from, to))
        }

        #[test]
        fn refuses_a_cursor_anywhere_inside_it() {
            assert_eq!(resolve(0, 0), None);
            assert_eq!(resolve(8, 8), None);
            let at = DOC.find("description").unwrap() + 4;
            assert_eq!(resolve(at, at), None);
            assert_eq!(resolve(end(), end()), None);
        }

        #[test]
        fn refuses_a_selection_inside_it() {
            assert_eq!(resolve(6, 10), None);
            assert_eq!(resolve(0, end()), None);
        }

        #[test]
        fn clips_a_selection_that_extends_past_it_to_start_after_it() {
            let target = resolve(6, body_one() + 4).unwrap();
            assert_eq!(target.source, TargetSource::Selection);
            assert_eq!(target.from, DOC.find("# Demo").unwrap());
            assert_eq!(target.to, body_one() + 4);
            assert_eq!(target.text, "# Demo\n\nBody");
            assert_eq!(block_texts(&target.blocks), ["# Demo", "Body one."]);
        }

        #[test]
        fn clips_select_all_to_everything_after_it() {
            let target = resolve(0, DOC.len()).unwrap();
            assert_eq!(target.source, TargetSource::Selection);
            assert_eq!(target.from, DOC.find("# Demo").unwrap());
            assert_eq!(target.to, DOC.len() - 1);
            assert_eq!(target.text, "# Demo\n\nBody one.\n\nBody two.");
            assert_eq!(block_ids(&target.blocks), ["p1", "p2", "p3"]);
        }

        #[test]
        fn clips_a_reversed_selection_the_same_way() {
            assert_eq!(resolve(body_one() + 4, 6), resolve(6, body_one() + 4));
        }

        #[test]
        fn clips_a_selection_that_starts_at_the_very_beginning() {
            assert_eq!(resolve(0, body_one() + 4), resolve(6, body_one() + 4));
        }

        #[test]
        fn keeps_a_clipped_target_out_of_the_frontmatter() {
            let target = resolve(0, DOC.len()).unwrap();
            assert!(target.from > end());
            assert!(target.blocks.iter().all(|block| block.from > end()));
        }

        #[test]
        fn refuses_a_selection_that_reaches_only_the_line_breaks_after_it() {
            assert_eq!(resolve(0, end() + 1), None);
            assert_eq!(resolve(6, end() + 2), None);
        }

        #[test]
        fn allows_a_selection_after_it() {
            let target = resolve(body_one(), body_one() + 4).unwrap();
            assert_eq!(target.source, TargetSource::Selection);
            assert_eq!(target.text, "Body");
        }

        #[test]
        fn allows_a_selection_that_begins_at_the_line_break_after_it() {
            let target = resolve(end(), body_one() + 9).unwrap();
            assert_eq!(target.source, TargetSource::Selection);
            assert_eq!(target.from, DOC.find("# Demo").unwrap());
            assert_eq!(target.text, "# Demo\n\nBody one.");
        }

        #[test]
        fn picks_the_first_block_for_a_cursor_on_the_blank_line_after_it() {
            let target = resolve(end() + 1, end() + 1).unwrap();
            assert_eq!(target.source, TargetSource::Block);
            assert_eq!(target.text, "# Demo");
        }

        #[test]
        fn allows_a_cursor_in_a_block_after_it() {
            let target = resolve(body_one() + 2, body_one() + 2).unwrap();
            assert_eq!(target.source, TargetSource::Block);
            assert_eq!(target.text, "Body one.");
        }

        #[test]
        fn refuses_everything_in_a_document_that_is_only_a_frontmatter() {
            let only = "---\nname: demo\n---\n";
            let blocks = extract_blocks(only);
            assert_eq!(resolve_target(&blocks, only, sel(6, 6)), None);
            assert_eq!(resolve_target(&blocks, only, sel(0, 18)), None);
            assert_eq!(resolve_target(&blocks, only, sel(0, only.len())), None);
        }

        #[test]
        fn does_not_refuse_anything_for_an_unclosed_frontmatter() {
            let open = "---\nname: demo\n\nBody.\n";
            assert!(resolve_target(&extract_blocks(open), open, sel(6, 6)).is_some());
        }

        #[test]
        fn does_not_put_the_frontmatter_in_the_neighbors() {
            let blocks = extract_blocks(DOC);
            let at = DOC.find("# Demo").unwrap();
            let target = resolve_target(&blocks, DOC, sel(at, at)).expect("no target");
            let neighbors = get_neighbors(&blocks, &target);
            assert_eq!(neighbors.before, vec![]);
            assert_eq!(neighbors.title.as_deref(), Some("Demo"));
            assert_eq!(block_texts(&neighbors.after), ["Body one.", "Body two."]);
        }
    }
}
