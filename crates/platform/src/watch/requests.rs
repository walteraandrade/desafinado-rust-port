use crate::files::{is_markdown_path, parse_path_request};

pub fn parse_watch_request(input: Option<&str>) -> Option<Option<String>> {
    match input {
        None => Some(None),
        Some(path) => {
            let path = parse_path_request(path)?;
            is_markdown_path(&path).then_some(Some(path))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_null_and_an_absolute_markdown_path_only() {
        assert_eq!(parse_watch_request(None), Some(None));
        assert_eq!(
            parse_watch_request(Some("/a/b.md")),
            Some(Some("/a/b.md".into()))
        );
        assert_eq!(parse_watch_request(Some("/a/b.txt")), None);
        assert_eq!(parse_watch_request(Some("b.md")), None);
    }
}
