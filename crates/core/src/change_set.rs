use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChange {
    pub from: usize,
    pub to: usize,
    pub insert: String,
}

impl TextChange {
    pub fn new(from: usize, to: usize, insert: impl Into<String>) -> Self {
        Self {
            from,
            to,
            insert: insert.into(),
        }
    }

    pub fn insert(at: usize, insert: impl Into<String>) -> Self {
        Self::new(at, at, insert)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assoc {
    Before,
    After,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangedRange {
    pub from_a: usize,
    pub to_a: usize,
    pub from_b: usize,
    pub to_b: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSet {
    length: usize,
    changes: Vec<TextChange>,
}

pub type ChangeDesc = ChangeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Op {
    Retain(usize),
    Delete(usize),
    Insert(String),
}

impl Op {
    fn size(&self) -> usize {
        match self {
            Self::Retain(count) | Self::Delete(count) => *count,
            Self::Insert(text) => text.len(),
        }
    }

    fn split(self, at: usize) -> (Self, Self) {
        match self {
            Self::Retain(count) => (Self::Retain(at), Self::Retain(count - at)),
            Self::Delete(count) => (Self::Delete(at), Self::Delete(count - at)),
            Self::Insert(text) => {
                let (head, tail) = text.split_at(at);
                (
                    Self::Insert(head.to_string()),
                    Self::Insert(tail.to_string()),
                )
            }
        }
    }
}

fn normalize(mut specs: Vec<TextChange>) -> Vec<TextChange> {
    specs.retain(|change| !(change.from == change.to && change.insert.is_empty()));
    specs.sort_by_key(|change| change.from);
    specs.into_iter().fold(Vec::new(), |mut merged, change| {
        match merged.last_mut() {
            Some(last) if change.from <= last.to => {
                last.to = last.to.max(change.to);
                last.insert.push_str(&change.insert);
            }
            _ => merged.push(change),
        }
        merged
    })
}

fn pop_front(ops: &mut VecDeque<Op>, size: usize) -> Op {
    let front = ops.pop_front().unwrap_or(Op::Retain(size));
    if front.size() > size {
        let (head, tail) = front.split(size);
        ops.push_front(tail);
        head
    } else {
        front
    }
}

impl ChangeSet {
    pub fn empty(length: usize) -> Self {
        Self {
            length,
            changes: Vec::new(),
        }
    }

    pub fn of(changes: &[TextChange], length: usize) -> Self {
        Self {
            length,
            changes: normalize(changes.to_vec()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn length(&self) -> usize {
        self.length
    }

    pub fn new_length(&self) -> usize {
        self.changes.iter().fold(self.length, |length, change| {
            length - (change.to - change.from) + change.insert.len()
        })
    }

    pub fn changes(&self) -> &[TextChange] {
        &self.changes
    }

    pub fn map_pos(&self, pos: usize, assoc: Assoc) -> usize {
        let mut pos_a = 0usize;
        let mut pos_b = 0usize;
        for change in &self.changes {
            if change.from > pos_a {
                if change.from > pos {
                    return pos_b + (pos - pos_a);
                }
                pos_b += change.from - pos_a;
                pos_a = change.from;
            }
            let deleted = change.to - change.from;
            let inserted = change.insert.len();
            let end_a = change.to;
            if end_a > pos || (end_a == pos && assoc == Assoc::Before && deleted == 0) {
                return if pos == pos_a || assoc == Assoc::Before {
                    pos_b
                } else {
                    pos_b + inserted
                };
            }
            pos_b += inserted;
            pos_a = end_a;
        }
        pos_b + pos.saturating_sub(pos_a)
    }

    pub fn changed_ranges(&self) -> Vec<ChangedRange> {
        let mut shift: isize = 0;
        self.changes
            .iter()
            .map(|change| {
                let from_b = (change.from as isize + shift) as usize;
                shift += change.insert.len() as isize - (change.to - change.from) as isize;
                ChangedRange {
                    from_a: change.from,
                    to_a: change.to,
                    from_b,
                    to_b: from_b + change.insert.len(),
                }
            })
            .collect()
    }

    pub fn apply(&self, doc: &str) -> String {
        let mut result = String::with_capacity(doc.len());
        let mut position = 0;
        for change in &self.changes {
            result.push_str(doc.get(position..change.from).unwrap_or(""));
            result.push_str(&change.insert);
            position = change.to;
        }
        result.push_str(doc.get(position..).unwrap_or(""));
        result
    }

    fn ops(&self) -> VecDeque<Op> {
        let mut ops = VecDeque::new();
        let mut position = 0;
        for change in &self.changes {
            if change.from > position {
                ops.push_back(Op::Retain(change.from - position));
            }
            if change.to > change.from {
                ops.push_back(Op::Delete(change.to - change.from));
            }
            if !change.insert.is_empty() {
                ops.push_back(Op::Insert(change.insert.clone()));
            }
            position = change.to;
        }
        if self.length > position {
            ops.push_back(Op::Retain(self.length - position));
        }
        ops
    }

    fn from_ops(ops: Vec<Op>, length: usize) -> Self {
        let mut changes: Vec<TextChange> = Vec::new();
        let mut open: Option<TextChange> = None;
        let mut position = 0;
        for op in ops {
            match op {
                Op::Retain(count) => {
                    changes.extend(open.take());
                    position += count;
                }
                Op::Delete(count) => {
                    let change = open.get_or_insert_with(|| TextChange::insert(position, ""));
                    change.to += count;
                    position += count;
                }
                Op::Insert(text) => {
                    open.get_or_insert_with(|| TextChange::insert(position, ""))
                        .insert
                        .push_str(&text);
                }
            }
        }
        changes.extend(open);
        Self { length, changes }
    }

    #[must_use]
    pub fn compose(&self, other: &ChangeSet) -> ChangeSet {
        let mut first = self.ops();
        let mut second = other.ops();
        let mut output: Vec<Op> = Vec::new();
        loop {
            if let Some(Op::Insert(_)) = second.front() {
                output.extend(second.pop_front());
                continue;
            }
            if let Some(Op::Delete(_)) = first.front() {
                output.extend(first.pop_front());
                continue;
            }
            let (Some(a), Some(b)) = (first.front(), second.front()) else {
                break;
            };
            let size = a.size().min(b.size());
            match (pop_front(&mut first, size), pop_front(&mut second, size)) {
                (Op::Retain(_), Op::Retain(_)) => output.push(Op::Retain(size)),
                (Op::Retain(_), Op::Delete(_)) => output.push(Op::Delete(size)),
                (Op::Insert(text), Op::Retain(_)) => output.push(Op::Insert(text)),
                _ => {}
            }
        }
        ChangeSet::from_ops(output, self.length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(changes: &[TextChange], length: usize) -> ChangeSet {
        ChangeSet::of(changes, length)
    }

    #[test]
    fn maps_positions_after_an_insertion() {
        let changes = of(&[TextChange::insert(5, "abc")], 100);
        assert_eq!(changes.map_pos(20, Assoc::After), 23);
        assert_eq!(changes.map_pos(3, Assoc::After), 3);
    }

    #[test]
    fn maps_an_insertion_point_by_association() {
        let changes = of(&[TextChange::insert(20, "abc")], 100);
        assert_eq!(changes.map_pos(20, Assoc::After), 23);
        assert_eq!(changes.map_pos(20, Assoc::Before), 20);
    }

    #[test]
    fn maps_positions_around_a_deletion() {
        let changes = of(&[TextChange::new(10, 20, "")], 100);
        assert_eq!(changes.map_pos(20, Assoc::Before), 10);
        assert_eq!(changes.map_pos(15, Assoc::After), 10);
        assert_eq!(changes.map_pos(30, Assoc::After), 20);
    }

    #[test]
    fn merges_touching_changes_into_one_range() {
        let changes = of(
            &[TextChange::new(0, 5, "a"), TextChange::new(5, 10, "bb")],
            20,
        );
        assert_eq!(changes.changed_ranges().len(), 1);
        assert_eq!(
            changes.apply(&"x".repeat(20)),
            format!("abb{}", "x".repeat(10))
        );
    }

    #[test]
    fn reports_changed_ranges_in_both_coordinate_systems() {
        let changes = of(
            &[TextChange::new(0, 10, "x"), TextChange::new(20, 30, "yy")],
            60,
        );
        assert_eq!(
            changes.changed_ranges(),
            vec![
                ChangedRange {
                    from_a: 0,
                    to_a: 10,
                    from_b: 0,
                    to_b: 1
                },
                ChangedRange {
                    from_a: 20,
                    to_a: 30,
                    from_b: 11,
                    to_b: 13
                },
            ]
        );
        assert_eq!(changes.new_length(), 60 - 9 - 8);
    }

    #[test]
    fn composes_two_change_sets() {
        let doc = "0123456789";
        let first = of(&[TextChange::insert(0, "ab")], doc.len());
        let second = of(&[TextChange::new(5, 6, "Z")], first.new_length());
        let composed = first.compose(&second);
        assert_eq!(composed.apply(doc), second.apply(&first.apply(doc)));
        assert_eq!(composed.map_pos(8, Assoc::After), 10);
    }

    #[test]
    fn composition_cancels_text_the_second_change_deletes_again() {
        let doc = "0123456789";
        let first = of(&[TextChange::insert(3, "abc")], doc.len());
        let second = of(&[TextChange::new(2, 8, "")], first.new_length());
        let composed = first.compose(&second);
        assert_eq!(composed.apply(doc), second.apply(&first.apply(doc)));
    }

    #[test]
    fn an_empty_change_set_is_empty() {
        assert!(ChangeSet::empty(10).is_empty());
        assert!(of(&[TextChange::insert(2, "")], 10).is_empty());
    }
}
