pub const fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

pub fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

pub fn line_ranges(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                lines.push((start, index));
                index += 1;
                start = index;
            }
            b'\r' => {
                lines.push((start, index));
                index += if bytes.get(index + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                start = index;
            }
            _ => index += 1,
        }
    }
    lines.push((start, bytes.len()));
    lines
}

pub fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fn hex_digit(value: u8, upper: bool) -> char {
    let digits = if upper {
        b"0123456789ABCDEF"
    } else {
        b"0123456789abcdef"
    };
    char::from(digits[usize::from(value & 0x0f)])
}

pub fn lower_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .flat_map(|byte| [hex_digit(byte >> 4, false), hex_digit(*byte, false)])
        .collect()
}

pub fn percent_encode_component(text: &str) -> String {
    text.bytes().fold(String::new(), |mut out, byte| {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push(hex_digit(byte >> 4, true));
            out.push(hex_digit(byte, true));
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_like_javascript() {
        assert_eq!(js_trim("\u{feff} a \u{a0}\n"), "a");
        assert_eq!(js_trim("\u{85}a"), "\u{85}a");
    }

    #[test]
    fn splits_lines_like_a_text_editor() {
        assert_eq!(line_ranges("a\n\nb"), [(0, 1), (2, 2), (3, 4)]);
        assert_eq!(line_ranges("a\r\nb\rc"), [(0, 1), (3, 4), (5, 6)]);
        assert_eq!(line_ranges(""), [(0, 0)]);
        assert_eq!(line_ranges("a\n"), [(0, 1), (2, 2)]);
    }

    #[test]
    fn counts_utf16_units() {
        assert_eq!(utf16_len("a😀"), 3);
    }

    #[test]
    fn encodes_components() {
        assert_eq!(percent_encode_component("gen-1/a b"), "gen-1%2Fa%20b");
    }
}
