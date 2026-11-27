//! A list of rows of one height that arrive, leave and move, for a view to
//! draw: where each row is drawn, and how much of a leaving row is left.
//!
//! The rows are placed as if the leaving rows were already gone, and each row
//! is drawn from where it is laid out, below the leaving rows still shrinking
//! above it. On a delete the rows below close up on the curve the deleted row
//! shrinks on, from the same moment, so the two cancel out and the rows below
//! keep to its bottom edge.

use std::time::Duration;

use ginga::{InOut, Phase, PresenceList, Transition};

/// How the rows of a [`ListRows`] move.
#[derive(Debug, Clone, Copy)]
pub struct ListMotion {
    /// How a row arrives and leaves.
    pub in_out: InOut,
    /// How a row glides to a new place when the list is reordered or grows.
    pub glide: Transition,
}

impl ListMotion {
    /// How the rows below a deleted row close up: on the curve the deleted
    /// row shrinks on.
    #[must_use]
    pub fn close_up(&self) -> Transition {
        self.in_out.exit
    }
}

/// A row to draw this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListRow<'a, K, T> {
    /// The row's key.
    pub key: &'a K,
    /// What it shows. While it leaves, what it was when it left.
    pub content: &'a T,
    /// Where it is in its life.
    pub phase: Phase,
    /// How much of it is here: `0.0` gone, `1.0` fully here.
    pub progress: f32,
    /// How far from where it is laid out to draw it, along the list.
    pub offset: f32,
    /// How high it is drawn: the row's height, or less while it leaves.
    pub height: f32,
    /// Its place among the rows that stay, or `None` while it leaves.
    pub index: Option<usize>,
}

/// Rows of one height that arrive, leave and glide to their places.
#[derive(Debug, Clone)]
pub struct ListRows<K, T> {
    rows: PresenceList<K, T>,
    keys: Vec<K>,
    height: f32,
}

/// How high a row `height` high is drawn while it leaves, `progress` of it left.
#[must_use]
pub fn leaving_height(height: f32, progress: f32) -> f32 {
    height * progress.clamp(0., 1.)
}

impl<K: PartialEq + Clone, T> ListRows<K, T> {
    /// A list that holds `items` from the start: nothing arrives.
    pub fn new(items: impl IntoIterator<Item = (K, T)>, height: f32, now: Duration) -> Self {
        let mut list = Self {
            rows: PresenceList::new(),
            keys: Vec::new(),
            height,
        };
        list.change(
            items,
            ListMotion {
                in_out: InOut::INSTANT,
                glide: Transition::Instant,
            },
            now,
        );
        list
    }

    /// Makes the list hold `items`, in that order, from `now`. A row whose key
    /// is new arrives, one whose key is gone leaves, and the rest glide to
    /// their new places, or close up behind a row that leaves.
    pub fn change(
        &mut self,
        items: impl IntoIterator<Item = (K, T)>,
        motion: ListMotion,
        now: Duration,
    ) {
        let items: Vec<(K, T)> = items.into_iter().collect();
        let removed = self
            .keys
            .iter()
            .any(|key| !items.iter().any(|(item, _)| item == key));
        self.keys = items.iter().map(|(key, _)| key.clone()).collect();
        self.rows.sync(items, motion.in_out, now);
        let transition = if removed {
            motion.close_up()
        } else {
            motion.glide
        };
        let height = self.height;
        let mut top = 0.;
        self.rows.place_all(
            |_, row| {
                let at = top;
                if row.phase != Phase::Exiting {
                    top += height;
                }
                at
            },
            transition,
            now,
        );
    }

    /// The rows to draw at `now`, in order, and whether any of them is still
    /// on its way, so the view asks for another frame.
    pub fn frame(&mut self, now: Duration) -> (Vec<ListRow<'_, K, T>>, bool) {
        let height = self.height;
        let mut leaving_above = 0.;
        let mut index = 0;
        let mut moving = false;
        let shown = self
            .rows
            .frame(now)
            .map(|row| {
                moving |= row.moving();
                let offset = row.offset - leaving_above;
                let (drawn, at) = if row.phase == Phase::Exiting {
                    let drawn = leaving_height(height, row.progress);
                    leaving_above += drawn;
                    (drawn, None)
                } else {
                    index += 1;
                    (height, Some(index - 1))
                };
                ListRow {
                    key: row.key,
                    content: row.content,
                    phase: row.phase,
                    progress: row.progress,
                    offset,
                    height: drawn,
                    index: at,
                }
            })
            .collect();
        (shown, moving)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ginga::{Easing, Tween};

    const ROW: f32 = 40.;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Rows arrive and leave in 100 ms, and glide in 100 ms on another curve,
    /// so a close-up on the glide instead of the way out shows.
    fn linear() -> ListMotion {
        let tween = Tween::new(ms(100), Easing::Linear);
        ListMotion {
            in_out: InOut::new(tween, tween),
            glide: Tween::new(ms(100), Easing::In).into(),
        }
    }

    fn list(keys: &[&'static str]) -> ListRows<&'static str, ()> {
        ListRows::new(keys.iter().map(|key| (*key, ())), ROW, ms(0))
    }

    fn change(rows: &mut ListRows<&'static str, ()>, keys: &[&'static str], now: Duration) {
        rows.change(keys.iter().map(|key| (*key, ())), linear(), now);
    }

    /// Each row's key, where it is drawn (laid out below the rows drawn above
    /// it, plus its offset) and how high.
    fn drawn(
        rows: &mut ListRows<&'static str, ()>,
        now: Duration,
    ) -> Vec<(&'static str, f32, f32)> {
        let mut laid_out = 0.;
        rows.frame(now)
            .0
            .into_iter()
            .map(|row| {
                let top = laid_out + row.offset;
                laid_out += row.height;
                (*row.key, top, row.height)
            })
            .collect()
    }

    #[test]
    fn the_first_fill_is_still() {
        let mut rows = list(&["a", "b"]);
        let (shown, moving) = rows.frame(ms(0));
        assert!(!moving);
        assert!(
            shown
                .iter()
                .all(|row| row.phase == Phase::Present && row.offset.abs() < 1e-6)
        );
    }

    #[test]
    fn the_rows_below_a_deleted_row_keep_to_its_bottom_edge() {
        let mut rows = list(&["a", "b", "c"]);
        change(&mut rows, &["a", "c"], ms(1_000));
        for t in [1_000, 1_030, 1_060, 1_090] {
            let rows = drawn(&mut rows, ms(t));
            let (_, b_top, b_height) = rows[1];
            let (_, c_top, _) = rows[2];
            assert_eq!(rows[1].0, "b");
            assert!(
                (c_top - (b_top + b_height)).abs() < 1e-3,
                "at {t} ms: {rows:?}"
            );
        }
        assert_eq!(
            drawn(&mut rows, ms(1_100)),
            [("a", 0., ROW), ("c", ROW, ROW)]
        );
    }

    #[test]
    fn a_deleted_row_shrinks_on_the_list_s_way_out() {
        let mut rows = list(&["a", "b"]);
        change(&mut rows, &["b"], ms(1_000));
        let (shown, moving) = rows.frame(ms(1_050));
        assert!(moving);
        assert_eq!(shown[0].phase, Phase::Exiting);
        assert!((shown[0].height - ROW / 2.).abs() < 1e-3);
        assert_eq!(shown[0].index, None);
        assert_eq!(shown[1].index, Some(0));
    }

    #[test]
    fn a_swap_glides_both_rows_to_their_places() {
        let mut rows = list(&["a", "b"]);
        change(&mut rows, &["b", "a"], ms(1_000));
        assert_eq!(
            drawn(&mut rows, ms(1_000)),
            [("b", ROW, ROW), ("a", 0., ROW)]
        );
        let midway = drawn(&mut rows, ms(1_050));
        assert!(midway[0].1 > 0. && midway[0].1 < ROW, "{midway:?}");
        assert!((midway[0].1 + midway[1].1 - ROW).abs() < 1e-3, "{midway:?}");
        assert_eq!(
            drawn(&mut rows, ms(1_100)),
            [("b", 0., ROW), ("a", ROW, ROW)]
        );
    }

    #[test]
    fn a_new_row_arrives_and_the_rows_below_glide_down() {
        let mut rows = list(&["a", "c"]);
        change(&mut rows, &["a", "b", "c"], ms(1_000));
        let (shown, _) = rows.frame(ms(1_000));
        assert_eq!(shown[1].phase, Phase::Entering);
        assert_eq!(drawn(&mut rows, ms(1_000))[2], ("c", ROW, ROW));
        assert_eq!(drawn(&mut rows, ms(1_100))[2], ("c", 2. * ROW, ROW));
    }

    #[test]
    fn a_later_change_before_the_next_frame_keeps_the_close_up() {
        let mut rows = list(&["a", "b", "c"]);
        change(&mut rows, &["a", "c"], ms(1_000));
        change(&mut rows, &["a", "c"], ms(1_001));
        let rows = drawn(&mut rows, ms(1_050));
        let (_, b_top, b_height) = rows[1];
        assert!((rows[2].1 - (b_top + b_height)).abs() < 1e-3, "{rows:?}");
    }

    #[test]
    fn with_no_motion_a_deleted_row_is_gone_at_once() {
        let mut rows = list(&["a", "b"]);
        rows.change(
            [("b", ())],
            ListMotion {
                in_out: InOut::INSTANT,
                glide: Transition::Instant,
            },
            ms(1_000),
        );
        assert_eq!(drawn(&mut rows, ms(1_000)), [("b", 0., ROW)]);
    }

    #[test]
    fn a_leaving_row_keeps_its_place_when_the_list_is_reordered_across_it() {
        let mut rows = list(&["a", "b", "c", "d"]);
        change(&mut rows, &["a", "c", "d"], ms(1_000));
        let before = drawn(&mut rows, ms(1_020));
        change(&mut rows, &["c", "a", "d"], ms(1_020));
        let after = drawn(&mut rows, ms(1_020));
        let top_of =
            |rows: &[(&str, f32, f32)], key| rows.iter().find(|row| row.0 == key).unwrap().1;
        assert!(
            (top_of(&before, "b") - top_of(&after, "b")).abs() < 1e-3,
            "{before:?} {after:?}"
        );
    }
}
