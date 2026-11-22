use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Whether the path ends in `.md` or `.markdown`, in any case. Works on names that are not UTF-8.
pub fn is_markdown_path(path: impl AsRef<Path>) -> bool {
    let bytes = path.as_ref().as_os_str().as_encoded_bytes();
    [b".md".as_slice(), b".markdown"].iter().any(|extension| {
        bytes.len() >= extension.len()
            && bytes[bytes.len() - extension.len()..].eq_ignore_ascii_case(extension)
    })
}

pub fn with_markdown_extension(path: &Path) -> PathBuf {
    if is_markdown_path(path) {
        path.to_path_buf()
    } else {
        let mut named = OsString::from(path.as_os_str());
        named.push(".md");
        PathBuf::from(named)
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
        assert_eq!(
            with_markdown_extension(Path::new("/x/notes")),
            Path::new("/x/notes.md")
        );
        assert_eq!(
            with_markdown_extension(Path::new("/x/notes.txt")),
            Path::new("/x/notes.txt.md")
        );
    }

    #[test]
    fn keeps_a_markdown_name_as_it_is() {
        assert_eq!(
            with_markdown_extension(Path::new("/x/a.MD")),
            Path::new("/x/a.MD")
        );
    }

    #[test]
    fn keeps_a_name_that_is_not_utf8() {
        use std::os::unix::ffi::OsStrExt;
        let named = Path::new(std::ffi::OsStr::from_bytes(b"/x/caf\xe9"));
        let target = with_markdown_extension(named);
        assert_eq!(target.as_os_str().as_bytes(), b"/x/caf\xe9.md");
        assert!(is_markdown_path(&target));
    }
}
