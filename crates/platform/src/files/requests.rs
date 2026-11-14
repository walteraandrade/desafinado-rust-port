use std::path::Path;

pub const MAX_FILE_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_TEXT_CHARS: usize = MAX_FILE_BYTES;
const MAX_PATH_CHARS: usize = 4096;
const MAX_NAME_CHARS: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveRequest {
    pub path: String,
    pub text: String,
    pub expected: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveAsRequest {
    pub text: String,
    pub suggested_name: String,
}

pub fn parse_path_request(input: &str) -> Option<String> {
    let valid = !input.is_empty()
        && input.chars().count() <= MAX_PATH_CHARS
        && !input.contains('\0')
        && Path::new(input).is_absolute();
    valid.then(|| input.to_owned())
}

fn text_ok(text: &str) -> bool {
    text.chars().count() <= MAX_TEXT_CHARS
}

pub fn parse_save_request(path: &str, text: &str, expected: &str) -> Option<SaveRequest> {
    let path = parse_path_request(path)?;
    (text_ok(text) && text_ok(expected)).then(|| SaveRequest {
        path,
        text: text.to_owned(),
        expected: expected.to_owned(),
    })
}

pub fn parse_save_as_request(text: &str, suggested_name: &str) -> Option<SaveAsRequest> {
    if !text_ok(text)
        || suggested_name.contains('\0')
        || suggested_name.chars().count() > MAX_NAME_CHARS * 4
    {
        return None;
    }
    let name = suggested_name
        .rsplit('/')
        .next()
        .unwrap_or(suggested_name)
        .to_owned();
    let length = name.chars().count();
    (1..=MAX_NAME_CHARS)
        .contains(&length)
        .then(|| SaveAsRequest {
            text: text.to_owned(),
            suggested_name: name,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_limit_is_the_file_limit_in_characters() {
        assert_eq!(MAX_TEXT_CHARS, MAX_FILE_BYTES);
    }

    #[test]
    fn path_accepts_an_absolute_path_only() {
        assert_eq!(parse_path_request("/a/b.md").as_deref(), Some("/a/b.md"));
        assert_eq!(parse_path_request("a/b.md"), None);
        assert_eq!(parse_path_request(""), None);
        assert_eq!(parse_path_request("/a\0b"), None);
        assert_eq!(parse_path_request(&format!("/{}", "a".repeat(4096))), None);
    }

    #[test]
    fn save_accepts_a_path_with_the_new_and_the_last_known_text() {
        let request = parse_save_request("/a/b.md", "new", "old").unwrap();
        assert_eq!(request.expected, "old");
        assert!(parse_save_request("/a/b.md", "", "").is_some());
        assert!(parse_save_request("b.md", "x", "x").is_none());
        assert!(parse_save_request("/a/b.md", &"x".repeat(MAX_TEXT_CHARS + 1), "").is_none());
    }

    #[test]
    fn save_as_keeps_only_the_last_segment_of_the_name() {
        let request = parse_save_as_request("t", "../../etc/notes.md").unwrap();
        assert_eq!(request.suggested_name, "notes.md");
        assert!(parse_save_as_request("t", "dir/").is_none());
        assert!(parse_save_as_request("t", &"n".repeat(256)).is_none());
        assert!(parse_save_as_request("t", "a\0b").is_none());
    }
}
