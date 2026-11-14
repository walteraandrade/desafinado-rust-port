const BYTE_ORDER_MARK: char = '\u{feff}';

pub fn decode_markdown(text: &str) -> String {
    let stripped = text.strip_prefix(BYTE_ORDER_MARK).unwrap_or(text);
    let mut out = String::with_capacity(stripped.len());
    let mut chars = stripped.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push('\n');
        } else {
            out.push(c);
        }
    }
    out
}

pub fn decode_disk(bytes: &[u8]) -> Option<String> {
    std::str::from_utf8(bytes).ok().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_the_byte_order_mark_and_normalizes_line_endings() {
        assert_eq!(decode_markdown("\u{feff}a\r\nb\rc\n"), "a\nb\nc\n");
    }

    #[test]
    fn keeps_a_mark_that_is_not_first() {
        assert_eq!(decode_markdown("a\u{feff}"), "a\u{feff}");
    }

    #[test]
    fn refuses_bytes_that_are_not_utf8() {
        assert_eq!(decode_disk(&[0xff, 0xfe, 0x00]), None);
        assert_eq!(decode_disk("ok ✓".as_bytes()).as_deref(), Some("ok ✓"));
    }
}
