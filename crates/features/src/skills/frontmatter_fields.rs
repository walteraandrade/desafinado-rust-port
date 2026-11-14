use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value};

use super::frontmatter_block::{Frontmatter, find_frontmatter, is_delimited_block, opening_end};
use super::yaml::{
    Map, Node, Parsed, Scalar, Style, node_start, parse, quoted_end, resolve_scalar, skip_props,
    to_value,
};
use super::yaml_render::{
    Carried, has_keep_chomping_header, render_double_json, render_styled, style_for,
};
use crate::text::js_trim;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FrontmatterProblemRule {
    FrontmatterNotClosed,
    FrontmatterNotRecognized,
    YamlSyntaxError,
    FrontmatterNotAMapping,
}

impl FrontmatterProblemRule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FrontmatterNotClosed => "frontmatter-not-closed",
            Self::FrontmatterNotRecognized => "frontmatter-not-recognized",
            Self::YamlSyntaxError => "yaml-syntax-error",
            Self::FrontmatterNotAMapping => "frontmatter-not-a-mapping",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrontmatterProblem {
    pub rule: FrontmatterProblemRule,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
}

pub type FrontmatterFields = JsonMap<String, Value>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillField {
    Name,
    Description,
}

impl SkillField {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Description => "description",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillFieldValues {
    pub name: Option<String>,
    pub description: Option<String>,
}

impl SkillFieldValues {
    pub fn name(value: &str) -> Self {
        Self {
            name: Some(value.to_string()),
            description: None,
        }
    }

    pub fn description(value: &str) -> Self {
        Self {
            name: None,
            description: Some(value.to_string()),
        }
    }

    pub fn both(name: &str, description: &str) -> Self {
        Self {
            name: Some(name.to_string()),
            description: Some(description.to_string()),
        }
    }

    fn entries(&self) -> Vec<(SkillField, &str)> {
        [
            (SkillField::Name, self.name.as_deref()),
            (SkillField::Description, self.description.as_deref()),
        ]
        .into_iter()
        .filter_map(|(field, value)| value.map(|text| (field, text)))
        .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FrontmatterReading {
    Absent,
    Present(FrontmatterFields),
    Invalid(FrontmatterProblem),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub from: usize,
    pub to: usize,
    pub insert: String,
}

fn problem(rule: FrontmatterProblemRule, message: &str) -> FrontmatterProblem {
    FrontmatterProblem {
        rule,
        message: message.to_string(),
        line: None,
        column: None,
    }
}

fn not_closed() -> FrontmatterProblem {
    problem(
        FrontmatterProblemRule::FrontmatterNotClosed,
        "frontmatter not closed",
    )
}

fn not_recognized() -> FrontmatterProblem {
    problem(
        FrontmatterProblemRule::FrontmatterNotRecognized,
        "frontmatter not recognized",
    )
}

fn not_a_mapping() -> FrontmatterProblem {
    problem(
        FrontmatterProblemRule::FrontmatterNotAMapping,
        "frontmatter must be a mapping of keys to values",
    )
}

fn line_number_at(text: &str, offset: usize) -> usize {
    text.as_bytes()[..offset]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}

fn parse_mapping(yaml: &str, first_line: usize) -> Result<Parsed, FrontmatterProblem> {
    let parsed = parse(yaml).map_err(|error| FrontmatterProblem {
        rule: FrontmatterProblemRule::YamlSyntaxError,
        message: error.message,
        line: Some(error.line + first_line - 1),
        column: Some(error.column),
    })?;
    match &parsed.root {
        None | Some(Node::Map(_)) => Ok(parsed),
        Some(_) => Err(not_a_mapping()),
    }
}

fn root_map(parsed: &Parsed) -> Option<&Map> {
    match &parsed.root {
        Some(Node::Map(map)) => Some(map),
        _ => None,
    }
}

pub fn read_yaml_fields(
    yaml: &str,
    first_line: usize,
) -> Result<FrontmatterFields, FrontmatterProblem> {
    let parsed = parse_mapping(yaml, first_line)?;
    let Some(root) = &parsed.root else {
        return Ok(JsonMap::new());
    };
    match to_value(root, &parsed) {
        Ok(Value::Object(fields)) => Ok(fields),
        Ok(_) => Err(not_a_mapping()),
        Err(message) => Err(problem(FrontmatterProblemRule::YamlSyntaxError, &message)),
    }
}

fn closing_line_start(text: &str, frontmatter: &Frontmatter) -> usize {
    text.as_bytes()[..frontmatter.to]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1)
}

fn is_unclosed(text: &str) -> bool {
    if opening_end(text).is_none() {
        return false;
    }
    let trimmed = text.trim_end_matches('\n');
    let closed = format!("{trimmed}\n---");
    find_frontmatter(&closed)
        .is_some_and(|found| !js_trim(&closed[found.yaml_from..found.yaml_to]).is_empty())
}

pub fn read_frontmatter(text: &str) -> FrontmatterReading {
    let Some(frontmatter) = find_frontmatter(text) else {
        return if is_unclosed(text) {
            FrontmatterReading::Invalid(not_closed())
        } else {
            FrontmatterReading::Absent
        };
    };
    let yaml = &text[frontmatter.yaml_from..closing_line_start(text, &frontmatter)];
    match read_yaml_fields(yaml, line_number_at(text, frontmatter.yaml_from)) {
        Ok(fields) => FrontmatterReading::Present(fields),
        Err(failure) => FrontmatterReading::Invalid(failure),
    }
}

struct Located {
    string_value: Option<String>,
    carried: Carried,
    key_end: usize,
    value_end: usize,
}

fn line_end(text: &str, at: usize) -> usize {
    let at = at.min(text.len());
    text.as_bytes()[at..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(text.len(), |index| at + index + 1)
}

fn line_end_after(block: &str, offset: usize) -> usize {
    let from = offset.saturating_sub(1).min(block.len());
    block.as_bytes()[from..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(block.len(), |index| from + index + 1)
}

fn without_comment(line: &str) -> &str {
    let mut previous_blank = true;
    for (index, c) in line.char_indices() {
        if c == '#' && previous_blank {
            return &line[..index];
        }
        previous_blank = c.is_whitespace();
    }
    line
}

fn comment_of(line: &str) -> Option<String> {
    let cut = without_comment(line).len();
    (cut < line.len()).then(|| line[cut + 1..].trim_end().to_string())
}

fn anchor_in_gap(gap: &str) -> Option<String> {
    gap.split('\n').find_map(|line| {
        without_comment(line)
            .split_whitespace()
            .find_map(|token| token.strip_prefix('&').map(str::to_string))
    })
}

fn anchor_in_props(text: &str, start: usize) -> Option<String> {
    let end = skip_props(text, start);
    text[start..end]
        .split_whitespace()
        .find_map(|token| token.strip_prefix('&').map(str::to_string))
}

fn token_end(text: &str, scalar: &Scalar) -> usize {
    match scalar.style {
        Style::QuoteSingle | Style::QuoteDouble => quoted_end(text, skip_props(text, scalar.start)),
        Style::Plain => scalar.end,
        Style::BlockLiteral | Style::BlockFolded => {
            let bytes = text.as_bytes();
            let mut end = scalar.end.min(text.len());
            while end > scalar.start && matches!(bytes[end - 1], b' ' | b'\t' | b'\n' | b'\r') {
                end -= 1;
            }
            end
        }
    }
}

fn content_end(text: &str, node: &Node) -> usize {
    match node {
        Node::Scalar(scalar) => token_end(text, scalar),
        Node::Alias { end, .. } => *end,
        Node::Seq(seq) if !seq.flow => seq
            .items
            .last()
            .map_or(seq.start, |item| content_end(text, item)),
        Node::Map(map) if !map.flow => map
            .pairs
            .last()
            .map_or(map.start, |(_, value)| content_end(text, value)),
        Node::Seq(seq) => seq.end,
        Node::Map(map) => map.end,
    }
}

fn block_header_comment(text: &str, key_end: usize) -> Option<String> {
    let first = &text[key_end..line_end(text, key_end)];
    let indicator = first.find(['|', '>'])?;
    let after = first[indicator + 1..].trim_start_matches(|c: char| "-+0123456789".contains(c));
    let rest = after.trim();
    rest.strip_prefix('#')
        .map(|comment| comment.trim_end().to_string())
}

fn comment_before_of(gap: &str) -> Option<String> {
    if !gap.contains('\n') {
        return None;
    }
    let comments: Vec<String> = gap.split('\n').filter_map(comment_of).collect();
    (!comments.is_empty()).then(|| comments.join("\n"))
}

fn analyze(text: &str, key_end: usize, value: &Node) -> Located {
    let key_line_end = line_end(text, key_end);
    match value {
        Node::Scalar(scalar) => {
            let string_value = match resolve_scalar(scalar) {
                Value::String(content) => Some(content),
                _ => None,
            };
            if scalar.style == Style::Plain && scalar.value.is_empty() {
                let end = line_end(text, key_end.max(scalar.end));
                let after_colon = text[key_end..end]
                    .find(':')
                    .map_or("", |colon| &text[key_end + colon + 1..end]);
                let comment = after_colon
                    .trim()
                    .strip_prefix('#')
                    .map(|comment| comment.trim_end().to_string());
                return Located {
                    string_value,
                    carried: Carried {
                        style: Some(Style::Plain),
                        comment,
                        comment_before: None,
                        anchor: anchor_in_gap(&text[key_end..end]),
                    },
                    key_end,
                    value_end: end,
                };
            }
            let end = token_end(text, scalar);
            let value_end = line_end(text, end).max(key_line_end);
            if scalar.style.is_block() {
                let gap_end = scalar.start.max(key_end).min(text.len());
                return Located {
                    string_value,
                    carried: Carried {
                        style: Some(scalar.style),
                        comment: block_header_comment(text, key_end),
                        comment_before: None,
                        anchor: anchor_in_gap(&text[key_end..gap_end]),
                    },
                    key_end,
                    value_end,
                };
            }
            let gap_end = scalar.start.max(key_end);
            let gap = &text[key_end..gap_end];
            let comment = text[end..line_end(text, end)]
                .trim()
                .strip_prefix('#')
                .map(|comment| comment.trim_end().to_string());
            Located {
                string_value,
                carried: Carried {
                    style: Some(scalar.style),
                    comment,
                    comment_before: comment_before_of(gap),
                    anchor: anchor_in_props(text, scalar.start).or_else(|| anchor_in_gap(gap)),
                },
                key_end,
                value_end,
            }
        }
        Node::Alias { .. } => Located {
            string_value: None,
            carried: Carried::default(),
            key_end,
            value_end: line_end(text, content_end(text, value)).max(key_line_end),
        },
        Node::Seq(_) | Node::Map(_) => {
            let gap_end = node_start(value).max(key_end).min(text.len());
            Located {
                string_value: None,
                carried: Carried {
                    anchor: anchor_in_gap(&text[key_end..gap_end]),
                    ..Carried::default()
                },
                key_end,
                value_end: line_end(text, content_end(text, value)).max(key_line_end),
            }
        }
    }
}

fn locate(block: &str, parsed: &Parsed, field: SkillField) -> Option<Located> {
    let map = root_map(parsed)?;
    let (key, value) = map
        .pairs
        .iter()
        .find(|(key, _)| matches!(key, Node::Scalar(scalar) if scalar.value == field.as_str()))?;
    let Node::Scalar(key) = key else {
        return None;
    };
    let key_end = token_end(block, key);
    Some(analyze(block, key_end, value))
}

fn is_safe_to_write(rendered: &str, field: SkillField, value: &str) -> bool {
    let Ok(parsed) = parse(rendered) else {
        return false;
    };
    let reads_back = root_map(&parsed).is_some_and(|map| {
        map.pairs.iter().any(|(key, node)| {
            matches!(key, Node::Scalar(scalar) if scalar.value == field.as_str())
                && matches!(node, Node::Scalar(scalar) if resolve_scalar(scalar) == Value::String(value.to_string()))
        })
    });
    reads_back && !has_keep_chomping_header(rendered)
}

fn render_pair(field: SkillField, value: &str, carried: &Carried) -> String {
    let name = field.as_str();
    let preferred = render_styled(name, value, style_for(value, carried.style), carried);
    if is_safe_to_write(&preferred, field, value) {
        preferred
    } else {
        render_double_json(name, value, carried)
    }
}

fn replacement(block: &str, located: &Located, field: SkillField, value: &str) -> Vec<TextEdit> {
    if located.string_value.as_deref() == Some(value) {
        return Vec::new();
    }
    let rendered = render_pair(field, value, &located.carried);
    let rendered = &rendered[field.as_str().len()..];
    let keeps_line_end = block[located.key_end..located.value_end].ends_with('\n');
    vec![TextEdit {
        from: located.key_end,
        to: located.value_end,
        insert: if keeps_line_end {
            rendered.to_string()
        } else {
            rendered[..rendered.len() - 1].to_string()
        },
    }]
}

fn insertion(entries: &[(SkillField, &str)], at: usize) -> Vec<TextEdit> {
    if entries.is_empty() {
        return Vec::new();
    }
    vec![TextEdit {
        from: at,
        to: at,
        insert: entries
            .iter()
            .map(|(field, value)| render_pair(*field, value, &Carried::default()))
            .collect(),
    }]
}

fn block_edits(block: &str, parsed: &Parsed, values: &SkillFieldValues) -> Vec<TextEdit> {
    let entries = values.entries();
    let name = locate(block, parsed, SkillField::Name);
    let targets: Vec<_> = entries
        .iter()
        .map(|(field, value)| (*field, *value, locate(block, parsed, *field)))
        .collect();
    let belongs_after_name = |field: SkillField| field == SkillField::Description && name.is_some();
    let missing: Vec<(SkillField, &str)> = targets
        .iter()
        .filter(|(_, _, located)| located.is_none())
        .map(|(field, value, _)| (*field, *value))
        .collect();
    let leading: Vec<_> = missing
        .iter()
        .filter(|(field, _)| !belongs_after_name(*field))
        .copied()
        .collect();
    let after_name: Vec<_> = missing
        .iter()
        .filter(|(field, _)| belongs_after_name(*field))
        .copied()
        .collect();
    let mut edits: Vec<TextEdit> = targets
        .iter()
        .flat_map(|(field, value, located)| {
            located
                .as_ref()
                .map_or_else(Vec::new, |found| replacement(block, found, *field, value))
        })
        .collect();
    edits.extend(insertion(&leading, 0));
    edits.extend(insertion(
        &after_name,
        name.as_ref()
            .map_or(0, |found| line_end_after(block, found.value_end)),
    ));
    edits
}

pub fn apply_edits(text: &str, edits: &[TextEdit]) -> String {
    let mut ordered: Vec<&TextEdit> = edits.iter().collect();
    ordered.sort_by_key(|edit| std::cmp::Reverse(edit.from));
    ordered.iter().fold(text.to_string(), |result, edit| {
        format!(
            "{}{}{}",
            &result[..edit.from],
            edit.insert,
            &result[edit.to..]
        )
    })
}

fn creation_edits(text: &str, values: &SkillFieldValues) -> Vec<TextEdit> {
    let block: String = values
        .entries()
        .iter()
        .map(|(field, value)| render_pair(*field, value, &Carried::default()))
        .collect();
    if block.is_empty() {
        return Vec::new();
    }
    let separator = if text.is_empty() || text.starts_with('\n') {
        ""
    } else {
        "\n"
    };
    vec![TextEdit {
        from: 0,
        to: 0,
        insert: format!("---\n{block}---\n{separator}"),
    }]
}

pub fn plan_skill_field_edits(
    text: &str,
    values: &SkillFieldValues,
) -> Result<Vec<TextEdit>, FrontmatterProblem> {
    let Some(frontmatter) = find_frontmatter(text) else {
        return if is_unclosed(text) {
            Err(not_closed())
        } else if is_delimited_block(text) {
            Err(not_recognized())
        } else {
            Ok(creation_edits(text, values))
        };
    };
    let block = &text[frontmatter.yaml_from..closing_line_start(text, &frontmatter)];
    let parsed = parse_mapping(block, line_number_at(text, frontmatter.yaml_from))?;
    let mut edits: Vec<TextEdit> = block_edits(block, &parsed, values)
        .into_iter()
        .map(|edit| TextEdit {
            from: edit.from + frontmatter.yaml_from,
            to: edit.to + frontmatter.yaml_from,
            insert: edit.insert,
        })
        .collect();
    edits.sort_by_key(|edit| edit.from);
    Ok(edits)
}

pub fn set_skill_fields(
    text: &str,
    values: &SkillFieldValues,
) -> Result<String, FrontmatterProblem> {
    plan_skill_field_edits(text, values).map(|edits| apply_edits(text, &edits))
}
