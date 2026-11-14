use serde::Serialize;
use serde_norway::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FrontmatterProblem {
    pub rule: &'static str,
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FrontmatterReading {
    Absent,
    Present(serde_norway::Mapping),
    Invalid(FrontmatterProblem),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frontmatter {
    pub to: usize,
    pub yaml_from: usize,
    pub yaml_to: usize,
}

fn problem(rule: &'static str, message: &str) -> FrontmatterProblem {
    FrontmatterProblem {
        rule,
        message: message.to_owned(),
        line: None,
        column: None,
    }
}

fn is_delimiter(line: &str) -> bool {
    line.strip_prefix("---")
        .is_some_and(|rest| rest.chars().all(|c| c == ' ' || c == '\t'))
}

fn is_mapping_key(line: &str) -> bool {
    let rest = if let Some(rest) = line.strip_prefix('"') {
        match rest.find('"') {
            Some(end) => &rest[end + 1..],
            None => return false,
        }
    } else if let Some(rest) = line.strip_prefix('\'') {
        match rest.find('\'') {
            Some(end) => &rest[end + 1..],
            None => return false,
        }
    } else {
        let end = line
            .char_indices()
            .find(|(_, c)| !(c.is_alphanumeric() || matches!(c, '_' | '.' | '-')))
            .map_or(line.len(), |(i, _)| i);
        if end == 0 {
            return false;
        }
        &line[end..]
    };
    let rest = rest.trim_start_matches([' ', '\t']);
    match rest.strip_prefix(':') {
        Some(after) => after.is_empty() || after.starts_with([' ', '\t']),
        None => false,
    }
}

fn looks_like_yaml(yaml: &str) -> bool {
    if yaml.trim().is_empty() {
        return true;
    }
    yaml.split('\n')
        .find(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .is_some_and(is_mapping_key)
}

pub fn find_frontmatter(text: &str) -> Option<Frontmatter> {
    if !text.starts_with("---") {
        return None;
    }
    let first_end = text.find('\n')?;
    if !is_delimiter(&text[..first_end]) {
        return None;
    }
    let yaml_from = first_end + 1;
    let mut offset = yaml_from;
    loop {
        let rest = &text[offset..];
        let line_end = rest.find('\n').map_or(text.len(), |i| offset + i);
        let line = &text[offset..line_end];
        if is_delimiter(line) {
            let yaml_to = if offset == yaml_from {
                yaml_from
            } else {
                offset - 1
            };
            let yaml = &text[yaml_from..yaml_to.max(yaml_from)];
            return looks_like_yaml(yaml).then_some(Frontmatter {
                to: line_end,
                yaml_from,
                yaml_to: yaml_to.max(yaml_from),
            });
        }
        if line_end >= text.len() {
            return None;
        }
        offset = line_end + 1;
    }
}

fn has_opening_delimiter(text: &str) -> bool {
    text.find('\n')
        .is_some_and(|end| is_delimiter(&text[..end]))
}

fn is_unclosed(text: &str) -> bool {
    if !has_opening_delimiter(text) {
        return false;
    }
    let closed = format!("{}\n---", text.trim_end_matches('\n'));
    find_frontmatter(&closed)
        .is_some_and(|found| !closed[found.yaml_from..found.yaml_to].trim().is_empty())
}

fn line_number_at(text: &str, offset: usize) -> usize {
    text[..offset].matches('\n').count() + 1
}

pub fn read_frontmatter(text: &str) -> FrontmatterReading {
    let Some(found) = find_frontmatter(text) else {
        return if is_unclosed(text) {
            FrontmatterReading::Invalid(problem("frontmatter-not-closed", "frontmatter not closed"))
        } else {
            FrontmatterReading::Absent
        };
    };
    let yaml = &text[found.yaml_from..found.yaml_to];
    let first_line = line_number_at(text, found.yaml_from);
    match serde_norway::from_str::<Value>(yaml) {
        Ok(Value::Null) => FrontmatterReading::Present(serde_norway::Mapping::new()),
        Ok(Value::Mapping(map)) => FrontmatterReading::Present(map),
        Ok(_) => FrontmatterReading::Invalid(problem(
            "frontmatter-not-a-mapping",
            "frontmatter must be a mapping of keys to values",
        )),
        Err(error) => {
            let location = error.location();
            FrontmatterReading::Invalid(FrontmatterProblem {
                rule: "yaml-syntax-error",
                message: error.to_string(),
                line: location.as_ref().map(|l| l.line() + first_line - 1),
                column: location.as_ref().map(|l| l.column()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(reading: &FrontmatterReading, key: &str) -> Option<String> {
        match reading {
            FrontmatterReading::Present(map) => {
                map.get(key).and_then(|v| v.as_str()).map(str::to_owned)
            }
            _ => None,
        }
    }

    #[test]
    fn reads_the_fields() {
        let reading = read_frontmatter("---\nname: pdf\ndescription: Reads PDFs.\n---\n\n# x\n");
        assert_eq!(field(&reading, "name").as_deref(), Some("pdf"));
        assert_eq!(
            field(&reading, "description").as_deref(),
            Some("Reads PDFs.")
        );
    }

    #[test]
    fn is_absent_without_frontmatter_or_when_it_does_not_look_like_yaml() {
        assert_eq!(read_frontmatter("# Title\n"), FrontmatterReading::Absent);
        assert_eq!(
            read_frontmatter("---\nnot a mapping line\n---\n"),
            FrontmatterReading::Absent
        );
    }

    #[test]
    fn an_empty_block_has_no_fields() {
        assert_eq!(
            read_frontmatter("---\n---\nbody"),
            FrontmatterReading::Present(serde_norway::Mapping::new())
        );
    }

    #[test]
    fn reports_a_block_that_is_not_closed() {
        let FrontmatterReading::Invalid(problem) = read_frontmatter("---\nname: a\n# body\n")
        else {
            panic!("expected a problem")
        };
        assert_eq!(problem.rule, "frontmatter-not-closed");
    }

    #[test]
    fn reports_a_yaml_syntax_error_with_the_line() {
        let FrontmatterReading::Invalid(problem) =
            read_frontmatter("---\nname: a\ndescription: [unclosed\n---\n")
        else {
            panic!("expected a problem")
        };
        assert_eq!(problem.rule, "yaml-syntax-error");
        assert!(problem.line.is_some());
    }

    #[test]
    fn mapping_key_forms() {
        for line in [
            "name: x",
            "\"quoted key\": 1",
            "'q': 2",
            "a.b-c_d:",
            "ключ: 3",
        ] {
            assert!(is_mapping_key(line), "{line}");
        }
        for line in ["- item", "just text", ": no", "name:value"] {
            assert!(!is_mapping_key(line), "{line}");
        }
    }
}
