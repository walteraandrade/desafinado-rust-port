#[expect(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "the path is lowercased first"
)]
pub fn is_markdown_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown")
}

pub fn with_markdown_extension(path: &str) -> String {
    if is_markdown_path(path) {
        path.to_owned()
    } else {
        format!("{path}.md")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_md_and_markdown_in_any_case() {
        for path in ["a.md", "a.MD", "/x/SKILL.md", "a.markdown", "a.Markdown"] {
            assert!(is_markdown_path(path), "{path}");
        }
    }

    #[test]
    fn refuses_everything_else() {
        for path in ["a.txt", "a.md.bak", "md", "a.mdx", "a.md/"] {
            assert!(!is_markdown_path(path), "{path}");
        }
    }

    #[test]
    fn adds_md_when_the_name_has_none_of_ours() {
        assert_eq!(with_markdown_extension("/x/notes"), "/x/notes.md");
        assert_eq!(with_markdown_extension("/x/notes.txt"), "/x/notes.txt.md");
    }

    #[test]
    fn keeps_a_markdown_name_as_it_is() {
        assert_eq!(with_markdown_extension("/x/a.MD"), "/x/a.MD");
    }
}
