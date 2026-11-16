//! The UI rules in docs/ui-rubric.md that can be read off the source.
//! Each test lists every violation it finds, so a failure says where to look.

use std::{fs, path::Path};

const LONGEST_FUNCTION: usize = 100;
const GRID: f32 = 4.;

struct Source {
    name: String,
    text: String,
}

fn sources() -> Vec<Source> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<Source> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? != "rs" {
                return None;
            }
            Some(Source {
                name: path.file_name()?.to_string_lossy().into_owned(),
                text: without_tests(&fs::read_to_string(&path).ok()?),
            })
        })
        .collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    files
}

fn without_tests(text: &str) -> String {
    match text.find("#[cfg(test)]\nmod tests") {
        Some(index) => text[..index].to_string(),
        None => text.to_string(),
    }
}

fn outside<'a>(files: &'a [Source], allowed: &'a [&str]) -> impl Iterator<Item = &'a Source> {
    files
        .iter()
        .filter(move |file| !allowed.contains(&file.name.as_str()))
}

fn matches(files: &[Source], allowed: &[&str], needles: &[&str]) -> Vec<String> {
    outside(files, allowed)
        .flat_map(|file| {
            file.text
                .lines()
                .enumerate()
                .filter(|(_, line)| needles.iter().any(|needle| line.contains(needle)))
                .map(|(index, line)| format!("{}:{}: {}", file.name, index + 1, line.trim()))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn report(rule: &str, violations: &[String]) {
    if !violations.is_empty() {
        panic!(
            "{rule}: {} violation(s)\n{}",
            violations.len(),
            violations.join("\n")
        );
    }
}

#[test]
fn colors_come_only_from_theme_roles() {
    let files = sources();
    let mut violations = matches(
        &files,
        &["theme.rs"],
        &[
            "hex(",
            "rgb(",
            "rgba(",
            "hsla(",
            "black()",
            "white()",
            "transparent_black",
            "transparent_white",
        ],
    );
    violations.extend(matches(&files, &["ui.rs"], &["opacity("]));
    report("C1 colors outside the theme", &violations);
}

#[test]
fn borders_only_in_the_shared_components() {
    let violations = matches(
        &sources(),
        &["ui.rs", "theme.rs"],
        &[".border_", "roles.hairline"],
    );
    report("C2 borders outside ui.rs", &violations);
}

#[test]
fn type_comes_from_the_type_scale() {
    let violations = matches(
        &sources(),
        &["ui.rs"],
        &[
            "text_size(",
            "text_xs()",
            "text_sm()",
            "text_base()",
            "text_lg()",
            "text_xl()",
            "text_2xl()",
            "font_weight(",
            "font_family(",
        ],
    );
    report("C3 type outside the type scale", &violations);
}

#[test]
fn the_type_scale_has_two_sizes_and_the_wordmark() {
    let ui = sources()
        .into_iter()
        .find(|file| file.name == "ui.rs")
        .unwrap();
    let mut sizes: Vec<String> = ui
        .text
        .match_indices("text_size(px(")
        .map(|(index, _)| {
            let rest = &ui.text[index + "text_size(px(".len()..];
            rest[..rest.find(')').unwrap()].to_string()
        })
        .chain(ui.text.match_indices("const TEXT_").map(|(index, _)| {
            let rest = &ui.text[index..];
            let start = rest.find("px(").unwrap() + 3;
            rest[start..start + rest[start..].find(')').unwrap()].to_string()
        }))
        .collect();
    sizes.sort();
    sizes.dedup();
    assert!(
        sizes.len() <= 3,
        "C3 the type scale has {} sizes: {sizes:?}",
        sizes.len()
    );
}

#[test]
fn pointer_and_hover_only_through_the_shared_components() {
    let violations = matches(
        &sources(),
        &["ui.rs"],
        &[
            ".cursor_pointer()",
            ".hover(",
            ".cursor(CursorStyle",
            "bg(roles.fill_hover)",
        ],
    );
    report("C8 hand-rolled interactive styling", &violations);
}

#[test]
fn every_click_target_is_a_shared_component() {
    let files = sources();
    let violations: Vec<String> = outside(&files, &["ui.rs"])
        .flat_map(|file| {
            let text = &file.text;
            ["on_click(", "on_mouse_down(MouseButton::Left, cx.listener"]
                .iter()
                .flat_map(|needle| text.match_indices(needle).map(|(index, _)| index))
                .filter(|index| !builds_with_ui(text, &chain_head(text, *index)))
                .map(|index| {
                    let line = text[..index].lines().count();
                    format!(
                        "{}:{}: {}",
                        file.name,
                        line,
                        text.lines().nth(line - 1).unwrap_or("").trim()
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    report("C6 click targets not built by ui.rs", &violations);
}

fn builds_with_ui(text: &str, head: &str) -> bool {
    if head.starts_with("ui::") {
        return true;
    }
    let name: String = head
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() || !head[name.len()..].starts_with('(') {
        return false;
    }
    let binding = format!("let {name} = |");
    text.match_indices(&binding).any(|(at, _)| {
        let rest = &text[at + binding.len()..];
        let body = rest[rest.find('|').map_or(0, |end| end + 1)..].trim_start();
        let body = body.strip_prefix('{').unwrap_or(body).trim_start();
        body.starts_with("ui::")
    })
}

fn chain_head(text: &str, index: usize) -> String {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut position = index;
    while position > 0 {
        position -= 1;
        match bytes[position] {
            b')' | b']' | b'}' => depth += 1,
            b'(' | b'[' | b'{' if depth > 0 => depth -= 1,
            b'(' | b'[' | b'{' | b';' | b'=' | b',' if depth == 0 => break,
            b'|' if depth == 0 => break,
            _ => {}
        }
    }
    let head = text[position + 1..index].trim_start();
    if head.starts_with("this") || head.starts_with("base") || head.starts_with("button") {
        let start = text[..position].rfind(".when").unwrap_or(0);
        if start > 0 && (head.starts_with("this")) {
            return chain_head(text, start);
        }
    }
    head.to_string()
}

#[test]
fn functions_stay_short() {
    let files = sources();
    let violations: Vec<String> = files
        .iter()
        .flat_map(|file| {
            let lines: Vec<&str> = file.text.lines().collect();
            lines
                .iter()
                .enumerate()
                .filter_map(|(start, line)| {
                    let trimmed = line.trim_start();
                    let is_fn = trimmed.starts_with("fn ")
                        || trimmed.starts_with("pub fn ")
                        || trimmed.starts_with("pub(crate) fn ");
                    if !is_fn {
                        return None;
                    }
                    let indent = &line[..line.len() - trimmed.len()];
                    let close = format!("{indent}}}");
                    let end = lines[start + 1..].iter().position(|line| *line == close)?;
                    let length = end + 2;
                    (length > LONGEST_FUNCTION).then(|| {
                        format!("{}:{}: {} lines: {}", file.name, start + 1, length, trimmed)
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    report("C9 functions over 100 lines", &violations);
}

#[test]
fn spacing_sits_on_the_grid() {
    let calls = [
        "p", "px", "py", "pt", "pb", "pl", "pr", "m", "mx", "my", "mt", "mb", "ml", "mr", "gap",
        "gap_x", "gap_y",
    ];
    let files = sources();
    let violations: Vec<String> = files
        .iter()
        .flat_map(|file| {
            file.text
                .lines()
                .enumerate()
                .flat_map(|(index, line)| {
                    calls
                        .iter()
                        .flat_map(|call| {
                            let needle = format!(".{call}(px(");
                            line.match_indices(&needle)
                                .filter_map(|(at, _)| {
                                    let rest = &line[at + needle.len()..];
                                    let value: f32 = rest[..rest.find(')')?].parse().ok()?;
                                    (value % GRID != 0.).then(|| {
                                        format!("{}:{}: {}", file.name, index + 1, line.trim())
                                    })
                                })
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    report("C10 spacing off the 4 px grid", &violations);
}
