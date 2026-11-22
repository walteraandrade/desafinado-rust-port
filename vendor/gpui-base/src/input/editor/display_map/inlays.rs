//! Text and rows the editor shows that are not part of the document.
use gpui::{HighlightStyle, SharedString};
use std::ops::Range;

/// Text drawn inside a line at a byte offset without being part of the document, such as a
/// suggested insertion.
///
/// It takes part in soft wrapping, is attached to the grapheme before its offset (the one
/// after it at a line start), and the caret, clicks and selections skip over it.
#[derive(Clone, Debug, PartialEq)]
pub struct Inlay {
    offset: usize,
    text: SharedString,
    style: HighlightStyle,
}

impl Inlay {
    pub fn new(offset: usize, text: impl Into<SharedString>, style: HighlightStyle) -> Self {
        Self {
            offset,
            text: text.into(),
            style,
        }
    }
    pub fn offset(&self) -> usize {
        self.offset
    }
    pub fn text(&self) -> &SharedString {
        &self.text
    }
    pub fn style(&self) -> &HighlightStyle {
        &self.style
    }
}

/// Empty rows reserved below the text of the line holding a byte offset, where the
/// application draws its own element, as Monaco's view zones.
///
/// The text below moves down by the rows, the caret skips them, and a click in them lands on
/// the end of the line above.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockZone {
    offset: usize,
    rows: usize,
}

impl BlockZone {
    pub fn new(offset: usize, rows: usize) -> Self {
        Self { offset, rows }
    }
    pub fn offset(&self) -> usize {
        self.offset
    }
    pub fn rows(&self) -> usize {
        self.rows
    }
}

/// Moves an offset through an edit that replaced `range` with `new_len` bytes. An offset
/// strictly inside the replaced range is gone.
pub(crate) fn shift_offset(offset: usize, range: &Range<usize>, new_len: usize) -> Option<usize> {
    if offset >= range.end {
        offset.checked_add(new_len)?.checked_sub(range.len())
    } else if offset <= range.start {
        Some(offset)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::shift_offset;

    #[test]
    fn an_offset_after_the_edit_moves_by_its_length_change() {
        assert_eq!(shift_offset(10, &(2..4), 5), Some(13));
        assert_eq!(shift_offset(10, &(2..8), 0), Some(4));
    }

    #[test]
    fn an_offset_before_the_edit_or_at_its_start_stays() {
        assert_eq!(shift_offset(1, &(2..4), 5), Some(1));
        assert_eq!(shift_offset(2, &(2..4), 0), Some(2));
    }

    #[test]
    fn an_insertion_at_the_offset_pushes_it_forward() {
        assert_eq!(shift_offset(4, &(4..4), 3), Some(7));
    }

    #[test]
    fn an_offset_inside_a_replaced_range_is_dropped() {
        assert_eq!(shift_offset(3, &(2..5), 1), None);
    }
}
