#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub value: String,
    pub added: bool,
    pub removed: bool,
}

fn is_word_char(c: char) -> bool {
    matches!(c,
        'a'..='z' | 'A'..='Z' | '0'..='9' | '_'
        | '\u{AD}'
        | '\u{C0}'..='\u{D6}'
        | '\u{D8}'..='\u{F6}'
        | '\u{F8}'..='\u{2C6}'
        | '\u{2C8}'..='\u{2D7}'
        | '\u{2DE}'..='\u{2FF}'
        | '\u{1E00}'..='\u{1EFF}')
}

fn is_inline_space(c: char) -> bool {
    crate::text::is_js_whitespace(c) && c != '\n' && c != '\r'
}

fn tokenize(value: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < value.len() {
        let rest = &value[index..];
        let length = if rest.starts_with("\r\n") {
            2
        } else if rest.starts_with('\n') {
            1
        } else {
            let first = rest.chars().next().unwrap_or(' ');
            let class: fn(char) -> bool = if is_word_char(first) {
                is_word_char
            } else if is_inline_space(first) {
                is_inline_space
            } else {
                |_| false
            };
            let run: usize = rest
                .chars()
                .take_while(|c| class(*c))
                .map(char::len_utf8)
                .sum();
            if run == 0 { first.len_utf8() } else { run }
        };
        tokens.push(&rest[..length]);
        index += length;
    }
    tokens
}

#[derive(Clone, Copy)]
struct Component {
    count: usize,
    added: bool,
    removed: bool,
    previous: Option<usize>,
}

#[derive(Clone, Copy)]
struct Path {
    old_pos: isize,
    last: Option<usize>,
}

fn extract_common(
    arena: &mut Vec<Component>,
    base: &mut Path,
    new_tokens: &[&str],
    old_tokens: &[&str],
    diagonal: isize,
) -> isize {
    let new_len = new_tokens.len() as isize;
    let old_len = old_tokens.len() as isize;
    let mut old_pos = base.old_pos;
    let mut new_pos = old_pos - diagonal;
    let mut common = 0;
    while new_pos + 1 < new_len
        && old_pos + 1 < old_len
        && old_tokens[(old_pos + 1) as usize] == new_tokens[(new_pos + 1) as usize]
    {
        new_pos += 1;
        old_pos += 1;
        common += 1;
    }
    if common > 0 {
        arena.push(Component {
            count: common,
            added: false,
            removed: false,
            previous: base.last,
        });
        base.last = Some(arena.len() - 1);
    }
    base.old_pos = old_pos;
    new_pos
}

fn add_to_path(
    arena: &mut Vec<Component>,
    path: Path,
    added: bool,
    removed: bool,
    old_pos_increment: isize,
) -> Path {
    let component = match path.last.map(|index| arena[index]) {
        Some(last) if last.added == added && last.removed == removed => Component {
            count: last.count + 1,
            added,
            removed,
            previous: last.previous,
        },
        _ => Component {
            count: 1,
            added,
            removed,
            previous: path.last,
        },
    };
    arena.push(component);
    Path {
        old_pos: path.old_pos + old_pos_increment,
        last: Some(arena.len() - 1),
    }
}

fn build_values(
    arena: &[Component],
    last: Option<usize>,
    new_tokens: &[&str],
    old_tokens: &[&str],
) -> Vec<Part> {
    let mut components = Vec::new();
    let mut cursor = last;
    while let Some(index) = cursor {
        components.push(arena[index]);
        cursor = arena[index].previous;
    }
    components.reverse();
    let mut new_pos = 0;
    let mut old_pos = 0;
    components
        .into_iter()
        .map(|component| {
            if component.removed {
                let value = old_tokens[old_pos..old_pos + component.count].concat();
                old_pos += component.count;
                Part {
                    value,
                    added: false,
                    removed: true,
                }
            } else {
                let value = new_tokens[new_pos..new_pos + component.count].concat();
                new_pos += component.count;
                if !component.added {
                    old_pos += component.count;
                }
                Part {
                    value,
                    added: component.added,
                    removed: false,
                }
            }
        })
        .collect()
}

pub fn diff_words_with_space(old: &str, new: &str) -> Vec<Part> {
    let old_tokens = tokenize(old);
    let new_tokens = tokenize(new);
    let new_len = new_tokens.len() as isize;
    let old_len = old_tokens.len() as isize;
    let mut arena: Vec<Component> = Vec::new();
    let offset = new_len + old_len + 1;
    let mut best: Vec<Option<Path>> = vec![None; (2 * offset + 3) as usize];
    let slot = |diagonal: isize| (diagonal + offset) as usize;

    let mut start = Path {
        old_pos: -1,
        last: None,
    };
    let new_pos = extract_common(&mut arena, &mut start, &new_tokens, &old_tokens, 0);
    if start.old_pos + 1 >= old_len && new_pos + 1 >= new_len {
        return build_values(&arena, start.last, &new_tokens, &old_tokens);
    }
    best[slot(0)] = Some(start);

    let mut edit_length: isize = 1;
    let max_edit_length = new_len + old_len;
    let mut min_diagonal = isize::MIN;
    let mut max_diagonal = isize::MAX;

    while edit_length <= max_edit_length {
        let mut diagonal = min_diagonal.max(-edit_length);
        let upper = max_diagonal.min(edit_length);
        while diagonal <= upper {
            let remove_path = best[slot(diagonal - 1)];
            let add_path = best[slot(diagonal + 1)];
            if remove_path.is_some() {
                best[slot(diagonal - 1)] = None;
            }
            let can_add = add_path
                .map(|path| {
                    let add_new_pos = path.old_pos - diagonal;
                    0 <= add_new_pos && add_new_pos < new_len
                })
                .unwrap_or(false);
            let can_remove = remove_path
                .map(|path| path.old_pos + 1 < old_len)
                .unwrap_or(false);
            if !can_add && !can_remove {
                best[slot(diagonal)] = None;
                diagonal += 2;
                continue;
            }
            let prefer_add = match (remove_path, add_path) {
                (Some(remove), Some(add)) => can_add && remove.old_pos < add.old_pos,
                _ => false,
            };
            let mut base = match (can_remove, add_path, remove_path) {
                (false, Some(add), _) => add_to_path(&mut arena, add, true, false, 0),
                (true, Some(add), _) if prefer_add => add_to_path(&mut arena, add, true, false, 0),
                (_, _, Some(remove)) => add_to_path(&mut arena, remove, false, true, 1),
                _ => unreachable!("a path can add or remove"),
            };
            let new_pos = extract_common(&mut arena, &mut base, &new_tokens, &old_tokens, diagonal);
            if base.old_pos + 1 >= old_len && new_pos + 1 >= new_len {
                return build_values(&arena, base.last, &new_tokens, &old_tokens);
            }
            best[slot(diagonal)] = Some(base);
            if base.old_pos + 1 >= old_len {
                max_diagonal = max_diagonal.min(diagonal - 1);
            }
            if new_pos + 1 >= new_len {
                min_diagonal = min_diagonal.max(diagonal + 1);
            }
            diagonal += 2;
        }
        edit_length += 1;
    }
    Vec::new()
}
