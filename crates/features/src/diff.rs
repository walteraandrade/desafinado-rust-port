use similar::{Algorithm, DiffOp, capture_diff_slices};

use crate::text::{count_words, is_js_space};

pub const BLOCK_LEVEL_THRESHOLD: f64 = 0.4;

#[derive(Clone, Debug, PartialEq)]
pub enum DiffHunk {
    Equal {
        text: String,
        from: usize,
        to: usize,
    },
    Deleted {
        text: String,
        from: usize,
        to: usize,
    },
    Inserted {
        text: String,
        at: usize,
    },
}

impl DiffHunk {
    pub fn text(&self) -> &str {
        match self {
            DiffHunk::Equal { text, .. }
            | DiffHunk::Deleted { text, .. }
            | DiffHunk::Inserted { text, .. } => text,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffMode {
    Word,
    Block,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdaptiveDiff {
    pub mode: DiffMode,
    pub hunks: Vec<DiffHunk>,
    pub changed_share: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    Equal,
    Removed,
    Added,
}

#[derive(Clone, Copy, Debug)]
pub struct Token<'a> {
    pub text: &'a str,
    pub from: usize,
    pub to: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Space,
    Word,
    Other,
}

fn class_of(c: char) -> Class {
    if is_js_space(c) {
        Class::Space
    } else if c.is_alphanumeric() || c == '_' {
        Class::Word
    } else {
        Class::Other
    }
}

pub fn tokenize(text: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((from, c)) = chars.next() {
        let class = class_of(c);
        let mut to = from + c.len_utf8();
        if class != Class::Other {
            while let Some(&(index, next)) = chars.peek() {
                if class_of(next) != class {
                    break;
                }
                to = index + next.len_utf8();
                chars.next();
            }
        }
        tokens.push(Token {
            text: &text[from..to],
            from,
            to,
        });
    }
    tokens
}

pub fn diff_tokens(old: &[Token<'_>], new: &[Token<'_>]) -> Vec<(Edit, usize, usize)> {
    let old_text: Vec<&str> = old.iter().map(|token| token.text).collect();
    let new_text: Vec<&str> = new.iter().map(|token| token.text).collect();
    capture_diff_slices(Algorithm::Myers, &old_text, &new_text)
        .into_iter()
        .flat_map(|op| match op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => vec![(Edit::Equal, old_index, new_index, len, len)],
            DiffOp::Delete {
                old_index,
                old_len,
                new_index,
            } => vec![(Edit::Removed, old_index, new_index, old_len, 0)],
            DiffOp::Insert {
                old_index,
                new_index,
                new_len,
            } => vec![(Edit::Added, old_index, new_index, 0, new_len)],
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => vec![
                (Edit::Removed, old_index, new_index, old_len, 0),
                (Edit::Added, old_index + old_len, new_index, 0, new_len),
            ],
        })
        .map(
            |(edit, old_index, new_index, old_len, new_len)| match edit {
                Edit::Equal => (edit, old_index, old_len),
                Edit::Removed => (edit, old_index, old_len),
                Edit::Added => (edit, new_index, new_len),
            },
        )
        .collect()
}

#[derive(Clone, Debug)]
pub struct DiffPart {
    pub edit: Edit,
    pub text: String,
}

fn span_text(text: &str, tokens: &[Token<'_>], index: usize, len: usize) -> String {
    match (tokens.get(index), tokens.get(index + len - 1)) {
        (Some(first), Some(last)) => text[first.from..last.to].to_string(),
        _ => String::new(),
    }
}

pub fn diff_words_with_space(old: &str, new: &str) -> Vec<DiffPart> {
    let old_tokens = tokenize(old);
    let new_tokens = tokenize(new);
    diff_tokens(&old_tokens, &new_tokens)
        .into_iter()
        .map(|(edit, index, len)| DiffPart {
            edit,
            text: match edit {
                Edit::Added => span_text(new, &new_tokens, index, len),
                _ => span_text(old, &old_tokens, index, len),
            },
        })
        .collect()
}

pub fn significant_tokens(text: &str) -> Vec<Token<'_>> {
    tokenize(text)
        .into_iter()
        .filter(|token| !token.text.chars().all(is_js_space))
        .collect()
}

#[derive(Clone, Debug)]
enum Chunk {
    Equal(String),
    Change { deleted: String, inserted: String },
}

fn is_blank(text: &str) -> bool {
    text.chars().all(is_js_space)
}

fn to_chunk(part: DiffPart) -> Chunk {
    match part.edit {
        Edit::Added => Chunk::Change {
            deleted: String::new(),
            inserted: part.text,
        },
        Edit::Removed => Chunk::Change {
            deleted: part.text,
            inserted: String::new(),
        },
        Edit::Equal => Chunk::Equal(part.text),
    }
}

fn join_changes(previous: (&str, &str), separator: &str, next: (&str, &str)) -> Chunk {
    let join = |left: &str, right: &str| {
        format!(
            "{left}{}{right}",
            if left.is_empty() { "" } else { separator }
        )
    };
    Chunk::Change {
        deleted: join(previous.0, next.0),
        inserted: join(previous.1, next.1),
    }
}

fn push_chunk(mut chunks: Vec<Chunk>, chunk: Chunk) -> Vec<Chunk> {
    let Chunk::Change { deleted, inserted } = &chunk else {
        chunks.push(chunk);
        return chunks;
    };
    if let Some(Chunk::Change {
        deleted: last_deleted,
        inserted: last_inserted,
    }) = chunks.last()
    {
        let joined = join_changes((last_deleted, last_inserted), "", (deleted, inserted));
        chunks.pop();
        chunks.push(joined);
        return chunks;
    }
    let n = chunks.len();
    if n >= 2
        && let (
            Chunk::Equal(blank),
            Chunk::Change {
                deleted: bd,
                inserted: bi,
            },
        ) = (&chunks[n - 1], &chunks[n - 2])
        && is_blank(blank)
    {
        let joined = join_changes((bd, bi), blank, (deleted, inserted));
        chunks.truncate(n - 2);
        chunks.push(joined);
        return chunks;
    }
    chunks.push(chunk);
    chunks
}

fn to_hunks(chunks: Vec<Chunk>) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut offset = 0;
    for chunk in chunks {
        match chunk {
            Chunk::Equal(text) => {
                let to = offset + text.len();
                hunks.push(DiffHunk::Equal {
                    text,
                    from: offset,
                    to,
                });
                offset = to;
            }
            Chunk::Change { deleted, inserted } => {
                let to = offset + deleted.len();
                if !deleted.is_empty() {
                    hunks.push(DiffHunk::Deleted {
                        text: deleted,
                        from: offset,
                        to,
                    });
                }
                if !inserted.is_empty() {
                    hunks.push(DiffHunk::Inserted {
                        text: inserted,
                        at: to,
                    });
                }
                offset = to;
            }
        }
    }
    hunks
}

fn sum_words(hunks: &[DiffHunk], pick: fn(&DiffHunk) -> bool) -> usize {
    hunks
        .iter()
        .filter(|hunk| pick(hunk))
        .map(|hunk| count_words(hunk.text()))
        .sum()
}

pub fn changed_share(hunks: &[DiffHunk]) -> f64 {
    let changed = sum_words(hunks, |h| matches!(h, DiffHunk::Deleted { .. }))
        + sum_words(hunks, |h| matches!(h, DiffHunk::Inserted { .. }));
    let total = changed + 2 * sum_words(hunks, |h| matches!(h, DiffHunk::Equal { .. }));
    if total == 0 {
        0.0
    } else {
        changed as f64 / total as f64
    }
}

fn block_hunks(original: &str, replacement: &str) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    if !original.is_empty() {
        hunks.push(DiffHunk::Deleted {
            text: original.to_string(),
            from: 0,
            to: original.len(),
        });
    }
    if !replacement.is_empty() {
        hunks.push(DiffHunk::Inserted {
            text: replacement.to_string(),
            at: original.len(),
        });
    }
    hunks
}

pub fn adaptive_diff(original: &str, replacement: &str) -> AdaptiveDiff {
    let chunks = diff_words_with_space(original, replacement)
        .into_iter()
        .map(to_chunk)
        .fold(Vec::new(), push_chunk);
    let hunks = to_hunks(chunks);
    let share = changed_share(&hunks);
    if share > BLOCK_LEVEL_THRESHOLD {
        AdaptiveDiff {
            mode: DiffMode::Block,
            hunks: block_hunks(original, replacement),
            changed_share: share,
        }
    } else {
        AdaptiveDiff {
            mode: DiffMode::Word,
            hunks,
            changed_share: share,
        }
    }
}
