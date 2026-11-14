use crate::text::line_ranges;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedRange {
    pub from: usize,
    pub to: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionRange {
    pub anchor: usize,
    pub head: usize,
}

pub fn selected_ranges(ranges: &[SelectionRange]) -> Vec<SelectedRange> {
    let mut selected: Vec<SelectedRange> = ranges
        .iter()
        .filter(|range| range.anchor != range.head)
        .map(|range| SelectedRange {
            from: range.anchor.min(range.head),
            to: range.anchor.max(range.head),
        })
        .collect();
    selected.sort_by_key(|range| range.from);
    selected
}

pub fn selected_empty_lines(doc: &str, ranges: &[SelectionRange]) -> Vec<usize> {
    let lines = line_ranges(doc);
    selected_ranges(ranges)
        .iter()
        .flat_map(|range| {
            lines
                .iter()
                .filter(|(from, to)| from == to && *from >= range.from && *from < range.to)
                .map(|(from, _)| *from)
                .collect::<Vec<_>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn range(anchor: usize, head: usize) -> SelectionRange {
        SelectionRange { anchor, head }
    }

    #[test]
    fn returns_nothing_for_a_bare_cursor() {
        assert_eq!(selected_ranges(&[range(1, 1)]), vec![]);
    }

    #[test]
    fn returns_each_non_empty_range_in_order() {
        let ranges = [range(8, 13), range(1, 1), range(4, 0)];
        assert_eq!(
            selected_ranges(&ranges),
            vec![
                SelectedRange { from: 0, to: 4 },
                SelectedRange { from: 8, to: 13 }
            ]
        );
    }

    #[test]
    fn returns_the_start_of_each_empty_line_a_range_covers() {
        assert_eq!(selected_empty_lines("one\n\ntwo\n\n", &[range(0, 8)]), [4]);
    }

    #[test]
    fn leaves_out_an_empty_line_where_the_range_only_ends() {
        assert_eq!(
            selected_empty_lines("one\n\ntwo\n\n", &[range(0, 4)]),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn marks_an_empty_line_inside_the_selection() {
        assert_eq!(selected_empty_lines("a\n\nb", &[range(0, 4)]), [2]);
    }
}
