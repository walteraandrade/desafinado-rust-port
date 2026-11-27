//! How the Quick action rows in Preferences come, go and move: rows that are
//! added arrive, removed rows collapse in place, and rows whose place changes
//! glide there.

use std::time::Duration;

use ginga::{InOut, Phase, PresenceList, Transition};

/// A Quick action row as it is drawn at one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct RowFrame {
    pub id: String,
    /// The label the row had: what a leaving row shows.
    pub label: String,
    pub phase: Phase,
    pub progress: f32,
    /// How far below its place to draw the row.
    pub offset: f32,
}

/// The rows of one list, keyed by Quick action id.
#[derive(Debug, Clone)]
pub struct QuickActionRows {
    rows: PresenceList<String, String>,
    pitch: f32,
}

impl QuickActionRows {
    /// Rows already in place: nothing arrives. `pitch` is a row's height.
    pub fn new(
        rows: impl IntoIterator<Item = (String, String)>,
        pitch: f32,
        now: Duration,
    ) -> Self {
        let mut list = PresenceList::new();
        list.sync(rows, InOut::INSTANT, now);
        let mut this = Self { rows: list, pitch };
        this.place(Transition::Instant, now);
        this
    }

    /// The list now holds `rows`, in that order. A removed row keeps its
    /// place while it collapses, so the rows below follow the layout up
    /// instead of also gliding.
    pub fn change(
        &mut self,
        rows: impl IntoIterator<Item = (String, String)>,
        in_out: InOut,
        glide: Transition,
        now: Duration,
    ) {
        self.place(Transition::Instant, now);
        self.rows.sync(rows, in_out, now);
        self.place(glide, now);
    }

    /// The rows to draw at `now`, and whether any of them still moves. A
    /// row that has left hands its place over at once: its height is
    /// already nothing.
    pub fn frame(&mut self, now: Duration) -> (Vec<RowFrame>, bool) {
        self.place(Transition::Instant, now);
        let mut moving = false;
        let frames = self
            .rows
            .frame(now)
            .map(|row| {
                moving |= row.moving();
                RowFrame {
                    id: row.key.clone(),
                    label: row.content.clone(),
                    phase: row.phase,
                    progress: row.progress,
                    offset: row.offset,
                }
            })
            .collect();
        (frames, moving)
    }

    fn place(&mut self, transition: Transition, now: Duration) {
        let pitch = self.pitch;
        self.rows
            .place_all(|index, _| index as f32 * pitch, transition, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ginga::{Easing, Tween};

    const PITCH: f32 = 40.;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn rows(ids: &[&str]) -> Vec<(String, String)> {
        ids.iter()
            .map(|id| ((*id).to_owned(), id.to_uppercase()))
            .collect()
    }

    fn ways() -> (InOut, Transition) {
        let fade = Tween::new(ms(100), Easing::Linear);
        (InOut::new(fade, fade), fade.into())
    }

    fn drawn(list: &mut QuickActionRows, now: Duration) -> Vec<(String, Phase, f32)> {
        list.frame(now)
            .0
            .into_iter()
            .map(|row| (row.id, row.phase, row.offset))
            .collect()
    }

    #[test]
    fn the_first_rows_are_in_place_and_still() {
        let mut list = QuickActionRows::new(rows(&["a", "b"]), PITCH, ms(0));
        let (frames, moving) = list.frame(ms(0));
        assert!(!moving);
        assert!(
            frames
                .iter()
                .all(|row| row.phase == Phase::Present && row.offset == 0.)
        );
    }

    #[test]
    fn a_removed_row_collapses_and_the_rows_below_do_not_also_glide() {
        let (in_out, glide) = ways();
        let mut list = QuickActionRows::new(rows(&["a", "b", "c"]), PITCH, ms(0));
        list.change(rows(&["b", "c"]), in_out, glide, ms(1_000));
        let (frames, moving) = list.frame(ms(1_050));
        assert!(moving);
        assert_eq!(frames[0].phase, Phase::Exiting);
        assert_eq!(frames[0].label, "A");
        assert!(frames.iter().all(|row| row.offset == 0.));
        assert_eq!(
            drawn(&mut list, ms(1_100)),
            [
                ("b".to_owned(), Phase::Present, 0.),
                ("c".to_owned(), Phase::Present, 0.),
            ]
        );
    }

    #[test]
    fn a_swap_glides_both_rows_and_a_new_one_arrives() {
        let (in_out, glide) = ways();
        let mut list = QuickActionRows::new(rows(&["a", "b"]), PITCH, ms(0));
        list.change(rows(&["b", "a", "c"]), in_out, glide, ms(1_000));
        assert_eq!(
            drawn(&mut list, ms(1_050)),
            [
                ("b".to_owned(), Phase::Present, 20.),
                ("a".to_owned(), Phase::Present, -20.),
                ("c".to_owned(), Phase::Entering, 0.),
            ]
        );
        let (_, moving) = list.frame(ms(1_100));
        assert!(!moving);
    }

    #[test]
    fn a_change_right_after_a_row_left_glides_from_where_the_rows_are() {
        let (in_out, glide) = ways();
        let mut list = QuickActionRows::new(rows(&["a", "b", "c"]), PITCH, ms(0));
        list.change(rows(&["b", "c"]), in_out, glide, ms(1_000));
        list.change(rows(&["c", "b"]), in_out, glide, ms(1_200));
        assert_eq!(
            drawn(&mut list, ms(1_250)),
            [
                ("c".to_owned(), Phase::Present, 20.),
                ("b".to_owned(), Phase::Present, -20.),
            ]
        );
    }

    #[test]
    fn nothing_moves_when_the_transitions_are_instant() {
        let mut list = QuickActionRows::new(rows(&["a", "b", "c"]), PITCH, ms(0));
        list.change(
            rows(&["c", "b"]),
            InOut::INSTANT,
            Transition::Instant,
            ms(1_000),
        );
        let (frames, moving) = list.frame(ms(1_000));
        assert!(!moving);
        assert_eq!(
            frames.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["c", "b"]
        );
    }
}
