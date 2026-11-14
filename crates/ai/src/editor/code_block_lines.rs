use pulldown_cmark::{Event, Options, Parser, Tag};

use crate::text::line_ranges;

pub fn code_block_line_starts(doc: &str) -> Vec<usize> {
    let lines = line_ranges(doc);
    let line_of = |offset: usize| {
        lines
            .iter()
            .rposition(|(from, _)| *from <= offset)
            .unwrap_or(0)
    };
    Parser::new_ext(doc, Options::empty())
        .into_offset_iter()
        .filter(|(event, _)| matches!(event, Event::Start(Tag::CodeBlock(_))))
        .flat_map(|(_, range)| {
            let end = if range.end > range.start && doc.as_bytes()[range.end - 1] == b'\n' {
                range.end - 1
            } else {
                range.end
            };
            let (first, last) = (line_of(range.start), line_of(end));
            lines[first..=last]
                .iter()
                .map(|(from, _)| *from)
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_no_lines_for_prose() {
        assert_eq!(
            code_block_line_starts("# Title\n\nSome text."),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn covers_every_line_of_a_fenced_code_block_fences_included() {
        let doc = "Intro\n\n```bash\nls\npwd\n```\n\nOutro";
        assert_eq!(
            code_block_line_starts(doc),
            [
                doc.find("```bash").unwrap(),
                doc.find("ls").unwrap(),
                doc.find("pwd").unwrap(),
                doc.rfind("```").unwrap(),
            ]
        );
    }

    #[test]
    fn covers_an_unclosed_fence_through_the_end_of_the_document() {
        let doc = "```\nline one\nline two";
        assert_eq!(
            code_block_line_starts(doc),
            [
                0,
                doc.find("line one").unwrap(),
                doc.find("line two").unwrap()
            ]
        );
    }

    #[test]
    fn returns_lines_for_every_code_block_in_order() {
        let doc = "```\na\n```\n\ntext\n\n```\nb\n```";
        let starts = code_block_line_starts(doc);
        assert_eq!(starts.len(), 6);
        let mut sorted = starts.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, starts);
    }

    #[test]
    fn covers_an_indented_code_block() {
        let doc = "text\n\n    one\n    two\n\nend";
        assert_eq!(
            code_block_line_starts(doc),
            [doc.find("    one").unwrap(), doc.find("    two").unwrap()]
        );
    }
}
