#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseLine<'a> {
    Data(&'a str),
    Done,
    Ignored,
}

pub fn parse_sse_line(line: &str) -> SseLine<'_> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.is_empty() || line.starts_with(':') {
        return SseLine::Ignored;
    }
    match line.strip_prefix("data:") {
        None => SseLine::Ignored,
        Some(rest) => {
            let data = rest.strip_prefix(' ').unwrap_or(rest);
            if data.trim() == "[DONE]" {
                SseLine::Done
            } else {
                SseLine::Data(data)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_data_lines_with_or_without_the_space() {
        assert_eq!(
            parse_sse_line("data: {\"a\":1}\n"),
            SseLine::Data("{\"a\":1}")
        );
        assert_eq!(
            parse_sse_line("data:{\"a\":1}\r\n"),
            SseLine::Data("{\"a\":1}")
        );
    }

    #[test]
    fn recognizes_done() {
        assert_eq!(parse_sse_line("data: [DONE]"), SseLine::Done);
    }

    #[test]
    fn ignores_comments_blank_lines_and_other_fields() {
        for line in [": OPENROUTER PROCESSING", "", "\n", "event: ping", "id: 4"] {
            assert_eq!(parse_sse_line(line), SseLine::Ignored, "{line:?}");
        }
    }
}
