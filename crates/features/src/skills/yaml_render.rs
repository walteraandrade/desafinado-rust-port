use regex::Regex;
use std::sync::LazyLock;

use super::yaml::{Style, reads_as_non_string};
use crate::text::is_js_space;

const INDENT: &str = "  ";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Carried {
    pub style: Option<Style>,
    pub comment: Option<String>,
    pub comment_before: Option<String>,
    pub anchor: Option<String>,
}

static UNSAFE_PLAIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"^[\n\t ,\[\]{}#&*!|>'"%@`]|^[?-]$|^[?-][ \t]|[\n:][ \t]|[ \t]\n|[\n\t ]#|[\n\t:]$"#,
    )
    .expect("valid pattern")
});

static KEEP_CHOMPING_HEADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[^\n:]+: [|>][^\s]*\+").expect("valid pattern"));

pub fn has_keep_chomping_header(rendered: &str) -> bool {
    KEEP_CHOMPING_HEADER.is_match(rendered)
}

fn forces_double_quotes(value: &str) -> bool {
    value
        .chars()
        .any(|c| matches!(c, '\u{0}'..='\u{8}' | '\u{b}'..='\u{1f}' | '\u{7f}'..='\u{9f}'))
}

pub fn escape_double(value: &str) -> String {
    let body: String = value
        .chars()
        .map(|c| match c {
            '"' => "\\\"".to_string(),
            '\\' => "\\\\".to_string(),
            '\n' => "\\n".to_string(),
            '\r' => "\\r".to_string(),
            '\t' => "\\t".to_string(),
            '\u{8}' => "\\b".to_string(),
            '\u{c}' => "\\f".to_string(),
            c if (c as u32) < 0x20
                || matches!(
                    c,
                    '\u{7f}'..='\u{9f}' | '\u{2028}' | '\u{2029}' | '\u{feff}'
                ) =>
            {
                format!("\\u{:04x}", c as u32)
            }
            c => c.to_string(),
        })
        .collect();
    format!("\"{body}\"")
}

fn quoted_string(value: &str) -> String {
    let has_double = value.contains('"');
    let has_single = value.contains('\'');
    if has_double && !has_single {
        single_quoted(value)
    } else {
        escape_double(value)
    }
}

fn single_quoted(value: &str) -> String {
    if value.contains('\n') {
        escape_double(value)
    } else {
        format!("'{}'", value.replace('\'', "''"))
    }
}

fn after_newline_runs(text: &str, addition: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        result.push(c);
        if c == '\n' && chars.peek() != Some(&'\n') {
            result.push_str(addition);
        }
    }
    result
}

fn plain_string(value: &str) -> String {
    if UNSAFE_PLAIN.is_match(value) {
        return if value.contains('\n') {
            block_string(value, Style::BlockLiteral, None)
        } else {
            quoted_string(value)
        };
    }
    let text = after_newline_runs(value, &format!("\n{INDENT}"));
    if reads_as_non_string(&text) {
        quoted_string(value)
    } else {
        text
    }
}

fn is_blank(value: &str) -> bool {
    value.chars().all(is_js_space)
}

fn ends_with_indented_blank(value: &str) -> bool {
    let trimmed = value.trim_end_matches([' ', '\t']);
    trimmed.len() < value.len() && trimmed.ends_with('\n')
}

fn has_more_indented_line(value: &str) -> bool {
    value.split('\n').any(|line| line.starts_with([' ', '\t']))
}

fn block_string(value: &str, style: Style, comment: Option<&str>) -> String {
    if ends_with_indented_blank(value) || is_blank(value) {
        return quoted_string(value);
    }
    let literal = style == Style::BlockLiteral || has_more_indented_line(value);
    let tail_start = value.trim_end_matches(['\n', '\t', ' ']).len();
    let tail = &value[tail_start..];
    let chomp = match tail.find('\n') {
        None => "-",
        Some(position) if position + 1 == tail.len() && value.len() != tail.len() => "",
        Some(_) => "+",
    };
    let body_with_tail = &value[..tail_start];
    let tail = tail.strip_suffix('\n').unwrap_or(tail);
    let tail = after_newline_runs(tail, INDENT);
    let leading = body_with_tail
        .char_indices()
        .find(|(_, c)| *c != ' ' && *c != '\n')
        .map_or(body_with_tail.len(), |(index, _)| index);
    let starts_with_space = body_with_tail[..leading].contains(' ');
    let last_newline = body_with_tail[..leading].rfind('\n');
    let start_len = match last_newline {
        Some(position) if position < leading => position + 1,
        _ => 0,
    };
    let start = after_newline_runs(&body_with_tail[..start_len], INDENT);
    let rest = &body_with_tail[start_len..];
    let mut header = format!("{}{chomp}", if starts_with_space { "2" } else { "" });
    if let Some(comment) = comment {
        let flat = comment.replace(['\r', '\n'], " ");
        header.push_str(" #");
        header.push_str(&flat);
    }
    if literal {
        let body = after_newline_runs(rest, INDENT);
        format!("|{header}\n{INDENT}{start}{body}{tail}")
    } else {
        let body = after_newline_runs(&double_runs(rest), INDENT);
        format!(">{header}\n{INDENT}{start}{body}{tail}")
    }
}

fn double_runs(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut run = 0usize;
    for c in text.chars() {
        if c == '\n' {
            run += 1;
        } else {
            if run > 0 {
                result.push_str(&"\n".repeat(run + 1));
                run = 0;
            }
            result.push(c);
        }
    }
    if run > 0 {
        result.push_str(&"\n".repeat(run + 1));
    }
    result
}

fn stringify_string(value: &str, style: Style, header_comment: Option<&str>) -> String {
    let style = if style != Style::QuoteDouble && forces_double_quotes(value) {
        Style::QuoteDouble
    } else {
        style
    };
    match style {
        Style::BlockLiteral | Style::BlockFolded => block_string(value, style, header_comment),
        Style::QuoteDouble => escape_double(value),
        Style::QuoteSingle => single_quoted(value),
        Style::Plain => plain_string(value),
    }
}

pub fn style_for(value: &str, previous: Option<Style>) -> Style {
    match previous {
        Some(style) if style.is_block() => style,
        _ if value.contains('\n') => Style::BlockLiteral,
        other => other.unwrap_or(Style::Plain),
    }
}

fn comment_lines(comment: &str) -> String {
    comment
        .split('\n')
        .map(|line| format!("{INDENT}#{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn props_of(carried: &Carried) -> String {
    carried
        .anchor
        .as_ref()
        .map_or(String::new(), |anchor| format!("&{anchor} "))
}

fn assemble(field: &str, text: &str, carried: &Carried, trailing_comment: bool) -> String {
    let props = props_of(carried);
    let mut line = match (&carried.comment_before, trailing_comment) {
        (Some(before), true) => {
            format!("{field}:\n{}\n{INDENT}{props}{text}", comment_lines(before))
        }
        _ => format!("{field}: {props}{text}"),
    };
    if let (Some(comment), true) = (&carried.comment, trailing_comment) {
        line.push_str(" #");
        line.push_str(comment);
    }
    line.push('\n');
    line
}

pub fn render_styled(field: &str, value: &str, style: Style, carried: &Carried) -> String {
    if style.is_block() {
        let comment = [
            carried.comment_before.as_deref(),
            carried.comment.as_deref(),
        ]
        .into_iter()
        .flatten()
        .map(|text| text.replace('\n', " "))
        .collect::<Vec<_>>()
        .join(" ");
        let header_comment = (!comment.is_empty()).then_some(comment.as_str());
        let text = stringify_string(value, style, header_comment);
        let is_block_text = text.starts_with(['|', '>']);
        let quoted = Carried {
            comment_before: None,
            ..carried.clone()
        };
        return assemble(field, &text, &quoted, !is_block_text);
    }
    let text = stringify_string(value, style, None);
    let is_block_text = text.starts_with(['|', '>']);
    assemble(field, &text, carried, !is_block_text)
}

pub fn render_double_json(field: &str, value: &str, carried: &Carried) -> String {
    assemble(field, &escape_double(value), carried, true)
}
