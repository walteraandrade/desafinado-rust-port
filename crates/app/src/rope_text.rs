//! Reads of the Document that stay on the Rope, so typing does not copy the whole text.

use desafinado_core::{
    blocks::{Frontmatter, find_frontmatter},
    change_set::{ChangeSet, TextChange},
};
use gpui_kit::base::input::Rope;

/// How much of the start of the text is read to find a frontmatter before reading it all.
const FRONTMATTER_PROBE: usize = 8 * 1024;

/// The edit as a change set; `after` is the text once the edit was applied.
pub(crate) fn edit_change(
    edit_start: usize,
    old_end: usize,
    new_end: usize,
    after: &Rope,
) -> ChangeSet {
    let inserted = after
        .try_slice(edit_start..new_end)
        .map(|slice| slice.to_string())
        .unwrap_or_default();
    let before_len = after.len() + (old_end - edit_start) - (new_end - edit_start);
    ChangeSet::of(
        &[TextChange::new(edit_start, old_end, inserted)],
        before_len,
    )
}

fn floor_char_boundary(text: &Rope, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The frontmatter at the start of the text. It reads the first few KB; the whole text only
/// when it opens like a frontmatter that does not close within them.
pub(crate) fn frontmatter_of(text: &Rope) -> Option<Frontmatter> {
    let probe_end = floor_char_boundary(text, FRONTMATTER_PROBE);
    let probe = text.slice(..probe_end).to_string();
    if probe_end == text.len() {
        return find_frontmatter(&probe);
    }
    if !probe.starts_with("---") {
        return None;
    }
    match find_frontmatter(&probe) {
        Some(found) if found.to < probe_end => Some(found),
        _ => find_frontmatter(&text.to_string()),
    }
}

/// The part of the text the frontmatter panel reads. A skill file is read whole (the panel
/// also checks what its frontmatter would be); any other file only up to the end of its
/// frontmatter, which is all that decides the report.
pub(crate) fn frontmatter_source(text: &Rope, is_skill: bool) -> String {
    if is_skill {
        return text.to_string();
    }
    match frontmatter_of(text) {
        Some(found) => text.slice(..found.to).to_string(),
        None if !text.bytes().take(3).eq("---".bytes()) => String::new(),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desafinado_features::skills::frontmatter_panel::{FrontmatterContext, inspect_frontmatter};

    fn report(text: &str, file_name: &str) -> String {
        let report = inspect_frontmatter(
            text,
            &FrontmatterContext {
                file_name: Some(file_name),
                folder_name: None,
            },
        );
        format!("{report:?}")
    }

    #[test]
    fn the_panel_reads_the_same_report_from_the_source_as_from_the_whole_text() {
        let body = "word ".repeat(10_000);
        let texts = [
            format!("---\nname: demo\n---\n{body}"),
            format!("---\nname: [oops\n---\n{body}"),
            format!("---\nname: demo\n{body}"),
            format!("# Title\n{body}"),
            format!("---\n{body}"),
            "---\nname: x\n---".to_string(),
            String::new(),
        ];
        for text in texts {
            let source = frontmatter_source(&Rope::from_str(&text), false);
            let label = &text[..text.len().min(30)];
            assert_eq!(report(&source, "a.md"), report(&text, "a.md"), "{label}");
        }
    }

    #[test]
    fn a_skill_file_is_read_whole() {
        let text = format!("---\nname: demo\n---\n{}", "word ".repeat(100));
        assert_eq!(frontmatter_source(&Rope::from_str(&text), true), text);
    }

    #[test]
    fn a_plain_document_without_a_frontmatter_has_an_empty_source() {
        assert_eq!(
            frontmatter_source(&Rope::from_str("# Title\nbody"), false),
            ""
        );
    }

    fn rope(text: &str) -> Rope {
        Rope::from_str(text)
    }

    #[test]
    fn edit_change_describes_an_insertion() {
        let after = rope("hello brave world");
        let changes = edit_change(6, 6, 12, &after);
        assert_eq!(
            changes,
            ChangeSet::of(&[TextChange::new(6, 6, "brave ".to_string())], 11)
        );
    }

    #[test]
    fn edit_change_describes_a_replacement_and_a_deletion() {
        let replaced = edit_change(0, 5, 2, &rope("hi world"));
        assert_eq!(
            replaced,
            ChangeSet::of(&[TextChange::new(0, 5, "hi".to_string())], 11)
        );
        let deleted = edit_change(5, 11, 5, &rope("hello"));
        assert_eq!(
            deleted,
            ChangeSet::of(&[TextChange::new(5, 11, String::new())], 11)
        );
    }

    #[test]
    fn frontmatter_matches_the_string_search_on_short_text() {
        let text = "---\nname: demo\n---\n\n# Demo\n";
        assert_eq!(frontmatter_of(&rope(text)), find_frontmatter(text));
        assert!(frontmatter_of(&rope(text)).is_some());
    }

    #[test]
    fn frontmatter_is_found_in_a_long_text() {
        let text = format!("---\nname: demo\n---\n{}", "word ".repeat(10_000));
        assert_eq!(frontmatter_of(&rope(&text)), find_frontmatter(&text));
        assert!(frontmatter_of(&rope(&text)).is_some());
    }

    #[test]
    fn a_long_text_that_does_not_open_with_a_frontmatter_has_none() {
        let text = format!("# Title\n---\nname: x\n---\n{}", "word ".repeat(10_000));
        assert_eq!(frontmatter_of(&rope(&text)), None);
    }

    #[test]
    fn a_frontmatter_that_closes_past_the_probe_is_still_found() {
        let yaml = "key: v\n".repeat(2_000);
        let text = format!("---\n{yaml}---\n{}", "word ".repeat(10_000));
        assert!(text.len() > FRONTMATTER_PROBE * 2);
        assert_eq!(frontmatter_of(&rope(&text)), find_frontmatter(&text));
        assert!(frontmatter_of(&rope(&text)).is_some());
    }

    #[test]
    fn the_probe_never_cuts_a_multibyte_character() {
        let text = format!("---\nname: demo\n---\n{}", "é".repeat(20_000));
        assert_eq!(frontmatter_of(&rope(&text)), find_frontmatter(&text));
    }

    #[test]
    fn ropes_of_the_same_text_are_equal_however_they_were_built() {
        let mut typed = rope("hello");
        typed.insert(5, " world");
        let before = typed.clone();
        assert_eq!(typed, rope("hello world"));
        typed.insert(11, "!");
        assert_ne!(typed, before);
        typed.remove(11..12);
        assert_eq!(typed, before);
    }
}
