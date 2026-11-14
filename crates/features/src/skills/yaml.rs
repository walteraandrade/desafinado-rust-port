use regex::Regex;
use saphyr_parser::{Event, Parser, ScalarStyle};
use serde_json::{Map as JsonMap, Number, Value};
use std::collections::HashMap;
use std::sync::LazyLock;

pub const ALIAS_BUDGET: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Plain,
    QuoteSingle,
    QuoteDouble,
    BlockLiteral,
    BlockFolded,
}

impl Style {
    pub fn is_block(self) -> bool {
        matches!(self, Style::BlockLiteral | Style::BlockFolded)
    }
}

#[derive(Debug, Clone)]
pub struct Scalar {
    pub value: String,
    pub style: Style,
    pub tag: Option<String>,
    pub anchor: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone)]
pub struct Seq {
    pub items: Vec<Node>,
    pub anchor: usize,
    pub start: usize,
    pub end: usize,
    pub flow: bool,
}

#[derive(Debug, Clone)]
pub struct Map {
    pub pairs: Vec<(Node, Node)>,
    pub anchor: usize,
    pub start: usize,
    pub end: usize,
    pub flow: bool,
}

#[derive(Debug, Clone)]
pub enum Node {
    Scalar(Scalar),
    Seq(Seq),
    Map(Map),
    Alias { id: usize, start: usize, end: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YamlError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Parsed {
    pub root: Option<Node>,
    pub anchors: HashMap<usize, Node>,
}

fn line_column(text: &str, at: usize) -> (usize, usize) {
    let before = &text.as_bytes()[..at.min(text.len())];
    let line = before.iter().filter(|byte| **byte == b'\n').count() + 1;
    let line_start = before
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    (line, at.saturating_sub(line_start) + 1)
}

fn convert_style(style: ScalarStyle) -> Style {
    match style {
        ScalarStyle::Plain => Style::Plain,
        ScalarStyle::SingleQuoted => Style::QuoteSingle,
        ScalarStyle::DoubleQuoted => Style::QuoteDouble,
        ScalarStyle::Literal => Style::BlockLiteral,
        ScalarStyle::Folded => Style::BlockFolded,
    }
}

type Spanned<'a> = (Event<'a>, saphyr_parser::Span);

struct Builder<'a, 't> {
    events: Vec<Spanned<'a>>,
    cursor: usize,
    offsets: Vec<usize>,
    anchors: HashMap<usize, Node>,
    text: &'t str,
}

impl<'a, 't> Builder<'a, 't> {
    fn at(&self, index: usize) -> usize {
        self.offsets[index.min(self.offsets.len() - 1)]
    }

    fn error(&self, index: usize, message: &str) -> YamlError {
        let (line, column) = line_column(self.text, self.at(index));
        YamlError {
            message: message.to_string(),
            line,
            column,
        }
    }

    fn next(&mut self) -> Option<Spanned<'a>> {
        let event = self.events.get(self.cursor).cloned();
        self.cursor += 1;
        event
    }

    fn remember(&mut self, anchor: usize, node: &Node) {
        if anchor > 0 {
            self.anchors.insert(anchor, node.clone());
        }
    }

    fn node(&mut self) -> Result<Node, YamlError> {
        let Some((event, span)) = self.next() else {
            return Err(self.error(0, "Unexpected end of input"));
        };
        let start = self.at(span.start.index());
        let end = self.at(span.end.index());
        match event {
            Event::Scalar(value, style, anchor, tag) => {
                let node = Node::Scalar(Scalar {
                    value: value.into_owned(),
                    style: convert_style(style),
                    tag: tag.map(|tag| format!("{}{}", tag.handle, tag.suffix)),
                    anchor,
                    start,
                    end,
                });
                self.remember(anchor, &node);
                Ok(node)
            }
            Event::Alias(id) => {
                if self.anchors.contains_key(&id) {
                    Ok(Node::Alias { id, start, end })
                } else {
                    Err(self.error(
                        span.start.index(),
                        "Unresolved alias (the anchor must be set before the alias)",
                    ))
                }
            }
            Event::SequenceStart(anchor, _) => {
                let flow = end > start;
                let mut items = Vec::new();
                loop {
                    if matches!(self.events.get(self.cursor), Some((Event::SequenceEnd, _))) {
                        break;
                    }
                    items.push(self.node()?);
                }
                let close = self.next().map(|(_, span)| self.at(span.end.index()));
                let last = items.last().map_or(start, node_end);
                let node = Node::Seq(Seq {
                    items,
                    anchor,
                    start,
                    end: if flow { close.unwrap_or(last) } else { last },
                    flow,
                });
                self.remember(anchor, &node);
                Ok(node)
            }
            Event::MappingStart(anchor, _) => {
                let flow = end > start;
                let mut pairs = Vec::new();
                loop {
                    if matches!(self.events.get(self.cursor), Some((Event::MappingEnd, _))) {
                        break;
                    }
                    let key = self.node()?;
                    let value = self.node()?;
                    pairs.push((key, value));
                }
                let close = self.next().map(|(_, span)| self.at(span.end.index()));
                let last = pairs.last().map_or(start, |(_, value)| node_end(value));
                let node = Node::Map(Map {
                    pairs,
                    anchor,
                    start,
                    end: if flow { close.unwrap_or(last) } else { last },
                    flow,
                });
                self.remember(anchor, &node);
                Ok(node)
            }
            _ => Err(self.error(span.start.index(), "Unexpected event")),
        }
    }
}

pub fn node_start(node: &Node) -> usize {
    match node {
        Node::Scalar(scalar) => scalar.start,
        Node::Seq(seq) => seq.start,
        Node::Map(map) => map.start,
        Node::Alias { start, .. } => *start,
    }
}

pub fn node_end(node: &Node) -> usize {
    match node {
        Node::Scalar(scalar) => scalar.end,
        Node::Seq(seq) => seq.end,
        Node::Map(map) => map.end,
        Node::Alias { end, .. } => *end,
    }
}

pub fn parse(text: &str) -> Result<Parsed, YamlError> {
    let offsets: Vec<usize> = text
        .char_indices()
        .map(|(byte, _)| byte)
        .chain(std::iter::once(text.len()))
        .collect();
    let mut events = Vec::new();
    for item in Parser::new_from_str(text) {
        match item {
            Ok(pair) => events.push(pair),
            Err(failure) => {
                let marker = failure.marker();
                let last_line = text.bytes().filter(|byte| *byte == b'\n').count() + 1;
                return Err(YamlError {
                    message: failure.info().replace('\n', " "),
                    line: marker.line().min(last_line),
                    column: marker.col() + 1,
                });
            }
        }
    }
    let mut builder = Builder {
        events,
        cursor: 0,
        offsets,
        anchors: HashMap::new(),
        text,
    };
    builder.next();
    let root = match builder.events.get(builder.cursor) {
        Some((Event::DocumentStart(_), _)) => {
            builder.next();
            let node = builder.node()?;
            builder.next();
            Some(node)
        }
        _ => None,
    };
    if let Some((event, span)) = builder.events.get(builder.cursor)
        && !matches!(event, Event::StreamEnd)
    {
        let index = span.start.index();
        return Err(builder.error(index, "Source contains multiple documents"));
    }
    let parsed = Parsed {
        root,
        anchors: builder.anchors,
    };
    if let Some(root) = &parsed.root
        && let Some(at) = find_duplicate_key(root)
    {
        let (line, column) = line_column(text, at);
        return Err(YamlError {
            message: "Map keys must be unique".to_string(),
            line,
            column,
        });
    }
    Ok(parsed)
}

fn find_duplicate_key(node: &Node) -> Option<usize> {
    match node {
        Node::Scalar(_) | Node::Alias { .. } => None,
        Node::Seq(seq) => seq.items.iter().find_map(find_duplicate_key),
        Node::Map(map) => {
            let mut seen: Vec<Value> = Vec::new();
            for (key, value) in &map.pairs {
                if let Node::Scalar(scalar) = key {
                    let resolved = resolve_scalar(scalar);
                    if seen.contains(&resolved) {
                        return Some(scalar.start);
                    }
                    seen.push(resolved);
                }
                if let Some(at) = find_duplicate_key(key).or_else(|| find_duplicate_key(value)) {
                    return Some(at);
                }
            }
            None
        }
    }
}

static NULL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:~|[Nn]ull|NULL)?$").expect("valid pattern"));
static BOOL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:[Tt]rue|TRUE|[Ff]alse|FALSE)$").expect("valid pattern"));
static INT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[-+]?[0-9]+$").expect("valid pattern"));
static INT_OCT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^0o[0-7]+$").expect("valid pattern"));
static INT_HEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^0x[0-9a-fA-F]+$").expect("valid pattern"));
static FLOAT_SPECIAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:[-+]?\.(?:inf|Inf|INF)|\.nan|\.NaN|\.NAN)$").expect("valid pattern")
});
static FLOAT_EXP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[-+]?(?:\.[0-9]+|[0-9]+(?:\.[0-9]*)?)[eE][-+]?[0-9]+$").expect("valid pattern")
});
static FLOAT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[-+]?(?:\.[0-9]+|[0-9]+\.[0-9]*)$").expect("valid pattern"));

pub fn reads_as_non_string(text: &str) -> bool {
    [
        &*NULL,
        &*BOOL,
        &*INT,
        &*INT_OCT,
        &*INT_HEX,
        &*FLOAT_SPECIAL,
        &*FLOAT_EXP,
        &*FLOAT,
    ]
    .iter()
    .any(|pattern| pattern.is_match(text))
}

fn number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        return Value::from(value as i64);
    }
    Number::from_f64(value).map_or(Value::Null, Value::Number)
}

fn integer(digits: &str, radix: u32) -> Value {
    let (sign, body) = match digits.as_bytes().first() {
        Some(b'-') => (-1.0, &digits[1..]),
        Some(b'+') => (1.0, &digits[1..]),
        _ => (1.0, digits),
    };
    match i64::from_str_radix(body, radix) {
        Ok(parsed) if sign < 0.0 => Value::from(-parsed),
        Ok(parsed) => Value::from(parsed),
        Err(_) => number(
            sign * body.chars().fold(0.0, |total, c| {
                total * f64::from(radix) + f64::from(c.to_digit(radix).unwrap_or(0))
            }),
        ),
    }
}

pub fn resolve_scalar(scalar: &Scalar) -> Value {
    let text = scalar.value.as_str();
    if scalar.style != Style::Plain {
        return Value::String(scalar.value.clone());
    }
    if matches!(scalar.tag.as_deref(), Some("!!str")) {
        return Value::String(scalar.value.clone());
    }
    if NULL.is_match(text) {
        Value::Null
    } else if BOOL.is_match(text) {
        Value::Bool(text.starts_with(['t', 'T']))
    } else if INT.is_match(text) {
        integer(text, 10)
    } else if INT_OCT.is_match(text) {
        integer(&text[2..], 8)
    } else if INT_HEX.is_match(text) {
        integer(&text[2..], 16)
    } else if FLOAT_SPECIAL.is_match(text) {
        Value::Null
    } else if FLOAT_EXP.is_match(text) || FLOAT.is_match(text) {
        text.parse::<f64>().map_or(Value::Null, number)
    } else {
        Value::String(scalar.value.clone())
    }
}

fn key_text(value: Value) -> String {
    match value {
        Value::String(text) => text,
        other => other.to_string(),
    }
}

fn expand(node: &Node, parsed: &Parsed, budget: &mut usize) -> Result<Value, String> {
    if *budget == 0 {
        return Err("Excessive alias count indicates a resource exhaustion attack".to_string());
    }
    *budget -= 1;
    match node {
        Node::Scalar(scalar) => Ok(resolve_scalar(scalar)),
        Node::Seq(seq) => seq
            .items
            .iter()
            .map(|item| expand(item, parsed, budget))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Node::Map(map) => {
            let mut object = JsonMap::new();
            for (key, value) in &map.pairs {
                let key = key_text(expand(key, parsed, budget)?);
                object.insert(key, expand(value, parsed, budget)?);
            }
            Ok(Value::Object(object))
        }
        Node::Alias { id, .. } => match parsed.anchors.get(id) {
            Some(target) => expand(target, parsed, budget),
            None => Err("Unresolved alias".to_string()),
        },
    }
}

pub fn to_value(node: &Node, parsed: &Parsed) -> Result<Value, String> {
    expand(node, parsed, &mut { ALIAS_BUDGET })
}

pub fn quoted_end(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    let Some(&quote) = bytes.get(start) else {
        return start;
    };
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' if quote == b'"' => index += 2,
            byte if byte == quote => {
                if quote == b'\'' && bytes.get(index + 1) == Some(&b'\'') {
                    index += 2;
                } else {
                    return index + 1;
                }
            }
            _ => index += 1,
        }
    }
    bytes.len()
}

pub fn skip_props(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    let mut index = start;
    while matches!(bytes.get(index), Some(b'&' | b'!')) {
        while index < bytes.len() && !matches!(bytes[index], b' ' | b'\t' | b'\n') {
            index += 1;
        }
        while matches!(bytes.get(index), Some(b' ' | b'\t')) {
            index += 1;
        }
    }
    index
}
