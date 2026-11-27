pub fn next_enabled(enabled: &[bool], from: Option<usize>, forward: bool) -> Option<usize> {
    let count = enabled.len();
    if count == 0 {
        return None;
    }
    let start = match (from, forward) {
        (Some(index), _) => index as isize,
        (None, true) => -1,
        (None, false) => count as isize,
    };
    let step = if forward { 1 } else { -1 };
    (1..=count as isize)
        .map(|offset| (start + step * offset).rem_euclid(count as isize) as usize)
        .find(|index| enabled[*index])
}

pub fn shifted(index: usize, count: usize, forward: bool) -> usize {
    if count == 0 {
        return 0;
    }
    if forward {
        (index + 1) % count
    } else {
        (index + count - 1) % count
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridStep {
    Up,
    Down,
    Left,
    Right,
    Next,
    Previous,
}

/// One step of keyboard focus in rows of controls, where `rows[r]` is the number of controls in
/// row `r`. Up and Down keep the column when the next row is wide enough, Left and Right stay in
/// the row, and none of them wraps; Next and Previous walk every control in order and wrap.
/// With nothing focused, a forward step lands on the first control and a backward one on the last.
pub fn grid_step(
    rows: &[usize],
    at: Option<(usize, usize)>,
    step: GridStep,
) -> Option<(usize, usize)> {
    let cells: Vec<(usize, usize)> = rows
        .iter()
        .enumerate()
        .flat_map(|(row, &count)| (0..count).map(move |column| (row, column)))
        .collect();
    let forward = matches!(step, GridStep::Down | GridStep::Right | GridStep::Next);
    let Some((row, column)) =
        at.filter(|&(row, column)| column < rows.get(row).copied().unwrap_or(0))
    else {
        return if forward { cells.first() } else { cells.last() }.copied();
    };
    let landing = |row: usize| (row, column.min(rows[row] - 1));
    match step {
        GridStep::Up => (0..row).rev().find(|&r| rows[r] > 0).map(landing),
        GridStep::Down => (row + 1..rows.len()).find(|&r| rows[r] > 0).map(landing),
        GridStep::Left => Some((row, column.saturating_sub(1))),
        GridStep::Right => Some((row, (column + 1).min(rows[row] - 1))),
        GridStep::Next | GridStep::Previous => {
            let index = cells.iter().position(|&cell| cell == (row, column))?;
            Some(cells[shifted(index, cells.len(), forward)])
        }
    }
    .or(Some((row, column)))
}

/// The scroll offset that brings an item into view, or `None` when it is already in view.
/// Offsets are negative as the content scrolls down, down to `-max`; `view` and `item` are
/// (top, bottom) in the same window coordinates, with the item where the current offset puts it.
pub fn revealed_offset(offset: f32, view: (f32, f32), item: (f32, f32), max: f32) -> Option<f32> {
    let (view_top, view_bottom) = view;
    let (top, bottom) = item;
    let next = if top < view_top {
        offset + (view_top - top)
    } else if bottom > view_bottom {
        offset - (bottom - view_bottom).min(top - view_top)
    } else {
        return None;
    };
    Some(next.clamp(-max.max(0.), 0.))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_the_first_or_last_enabled_item() {
        assert_eq!(next_enabled(&[false, true, true], None, true), Some(1));
        assert_eq!(next_enabled(&[true, true, false], None, false), Some(1));
    }

    #[test]
    fn skips_disabled_items_and_wraps() {
        let enabled = [true, false, true];
        assert_eq!(next_enabled(&enabled, Some(0), true), Some(2));
        assert_eq!(next_enabled(&enabled, Some(2), true), Some(0));
        assert_eq!(next_enabled(&enabled, Some(0), false), Some(2));
    }

    #[test]
    fn finds_nothing_when_every_item_is_disabled() {
        assert_eq!(next_enabled(&[false, false], None, true), None);
        assert_eq!(next_enabled(&[], None, true), None);
    }

    #[test]
    fn moves_between_menus_in_a_ring() {
        assert_eq!(shifted(1, 2, true), 0);
        assert_eq!(shifted(0, 2, false), 1);
    }

    #[test]
    fn up_and_down_keep_the_column_when_the_row_is_wide_enough() {
        let rows = [1, 3, 2];
        assert_eq!(grid_step(&rows, Some((1, 2)), GridStep::Down), Some((2, 1)));
        assert_eq!(grid_step(&rows, Some((2, 1)), GridStep::Up), Some((1, 1)));
        assert_eq!(grid_step(&rows, Some((1, 2)), GridStep::Up), Some((0, 0)));
    }

    #[test]
    fn arrows_stop_at_the_edges() {
        let rows = [2, 2];
        assert_eq!(grid_step(&rows, Some((0, 0)), GridStep::Up), Some((0, 0)));
        assert_eq!(grid_step(&rows, Some((1, 1)), GridStep::Down), Some((1, 1)));
        assert_eq!(grid_step(&rows, Some((0, 0)), GridStep::Left), Some((0, 0)));
        assert_eq!(
            grid_step(&rows, Some((0, 1)), GridStep::Right),
            Some((0, 1))
        );
    }

    #[test]
    fn left_and_right_stay_in_the_row() {
        let rows = [3, 1];
        assert_eq!(
            grid_step(&rows, Some((0, 1)), GridStep::Right),
            Some((0, 2))
        );
        assert_eq!(grid_step(&rows, Some((0, 1)), GridStep::Left), Some((0, 0)));
    }

    #[test]
    fn tab_walks_every_control_and_wraps() {
        let rows = [1, 2];
        assert_eq!(grid_step(&rows, Some((0, 0)), GridStep::Next), Some((1, 0)));
        assert_eq!(grid_step(&rows, Some((1, 0)), GridStep::Next), Some((1, 1)));
        assert_eq!(grid_step(&rows, Some((1, 1)), GridStep::Next), Some((0, 0)));
        assert_eq!(
            grid_step(&rows, Some((0, 0)), GridStep::Previous),
            Some((1, 1))
        );
    }

    #[test]
    fn skips_empty_rows() {
        let rows = [1, 0, 2];
        assert_eq!(grid_step(&rows, Some((0, 0)), GridStep::Down), Some((2, 0)));
        assert_eq!(grid_step(&rows, Some((2, 1)), GridStep::Up), Some((0, 0)));
    }

    #[test]
    fn starts_at_the_first_or_last_control() {
        let rows = [2, 1];
        assert_eq!(grid_step(&rows, None, GridStep::Down), Some((0, 0)));
        assert_eq!(grid_step(&rows, None, GridStep::Previous), Some((1, 0)));
        assert_eq!(grid_step(&rows, Some((5, 0)), GridStep::Next), Some((0, 0)));
        assert_eq!(grid_step(&[], None, GridStep::Next), None);
    }

    #[test]
    fn leaves_an_item_in_view_where_it_is() {
        assert_eq!(
            revealed_offset(-40., (100., 500.), (200., 240.), 800.),
            None
        );
    }

    #[test]
    fn scrolls_up_to_an_item_above_the_view() {
        assert_eq!(
            revealed_offset(-300., (100., 500.), (60., 100.), 800.),
            Some(-260.)
        );
    }

    #[test]
    fn scrolls_down_until_an_item_below_the_view_shows_whole() {
        assert_eq!(
            revealed_offset(0., (100., 500.), (520., 560.), 800.),
            Some(-60.)
        );
    }

    #[test]
    fn shows_the_top_of_an_item_taller_than_the_view() {
        assert_eq!(
            revealed_offset(0., (100., 200.), (300., 600.), 800.),
            Some(-200.)
        );
    }

    #[test]
    fn never_scrolls_past_the_ends() {
        assert_eq!(
            revealed_offset(-20., (100., 500.), (0., 40.), 800.),
            Some(0.)
        );
        assert_eq!(
            revealed_offset(-790., (100., 500.), (900., 940.), 800.),
            Some(-800.)
        );
    }
}
