use similar::{Algorithm, DiffOp, capture_diff_slices};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LineRun {
    Same { lines: Vec<String> },
    Changed { removed: String, added: String },
}

fn without_trailing_newlines(text: &str) -> &str {
    text.trim_end_matches('\n')
}

fn lines_of(text: &str) -> Vec<&str> {
    without_trailing_newlines(text).split('\n').collect()
}

pub fn line_runs(before: &str, after: &str) -> Vec<LineRun> {
    let old = lines_of(before);
    let new = lines_of(after);
    let mut runs = Vec::new();
    let mut removed: Vec<&str> = Vec::new();
    let mut added: Vec<&str> = Vec::new();
    let flush = |runs: &mut Vec<LineRun>, removed: &mut Vec<&str>, added: &mut Vec<&str>| {
        if !removed.is_empty() || !added.is_empty() {
            runs.push(LineRun::Changed {
                removed: removed.join("\n"),
                added: added.join("\n"),
            });
            removed.clear();
            added.clear();
        }
    };
    for op in capture_diff_slices(Algorithm::Myers, &old, &new) {
        match op {
            DiffOp::Equal { old_index, len, .. } => {
                flush(&mut runs, &mut removed, &mut added);
                runs.push(LineRun::Same {
                    lines: old[old_index..old_index + len]
                        .iter()
                        .map(|line| line.to_string())
                        .collect(),
                });
            }
            DiffOp::Delete {
                old_index, old_len, ..
            } => removed.extend(&old[old_index..old_index + old_len]),
            DiffOp::Insert {
                new_index, new_len, ..
            } => added.extend(&new[new_index..new_index + new_len]),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                removed.extend(&old[old_index..old_index + old_len]);
                added.extend(&new[new_index..new_index + new_len]);
            }
        }
    }
    flush(&mut runs, &mut removed, &mut added);
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_trailing_newlines_on_both_sides() {
        assert_eq!(line_runs("a\nb\n\n\n", "a\nb"), line_runs("a\nb", "a\nb\n"));
    }

    #[test]
    fn keeps_blank_lines_that_are_not_trailing() {
        assert_eq!(
            line_runs("a\n\nb\n", "a\n\nc\n\n"),
            line_runs("a\n\nb", "a\n\nc")
        );
    }
}
