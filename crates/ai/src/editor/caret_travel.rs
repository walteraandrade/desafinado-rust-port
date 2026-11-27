//! How the AI caret travels. It takes off from the writer's caret when it
//! first shows, glides when it moves to another place in the text, and stays
//! on the text while the view scrolls or the lines reflow.
//!
//! The glide never animates the caret's position. It animates how much of the
//! caret's total travel has been covered, and the caret is drawn that far
//! short of where its place in the text is this frame.

use std::time::Duration;

use ginga::{Motion, Transition};

/// A point in the pane, in pixels.
pub type Spot = (f32, f32);

#[derive(Debug, Clone, Copy, PartialEq)]
enum Seen {
    /// No AI caret: the next one takes off from the writer's caret.
    Never,
    /// There is an AI caret, but the last frame could not draw it.
    Hidden,
    /// The last frame had the caret at this place in the text.
    At { pos: usize, spot: Spot },
}

#[derive(Debug, Clone, Copy)]
pub struct CaretTravel {
    seen: Seen,
    /// The sum of every move since the caret showed: what the glide heads for.
    travelled: Spot,
    /// How much of that travel the glide has covered.
    covered: Motion<Spot>,
}

impl Default for CaretTravel {
    fn default() -> Self {
        Self {
            seen: Seen::Never,
            travelled: (0., 0.),
            covered: Motion::at((0., 0.)),
        }
    }
}

/// One frame of the AI caret.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaretFrame {
    /// Where to draw the caret.
    pub spot: Spot,
    /// The caret is still on its way: draw another frame.
    pub moving: bool,
}

impl CaretTravel {
    /// There is no AI caret any more.
    pub fn land(&mut self) {
        self.seen = Seen::Never;
    }

    /// The AI caret is out of view this frame.
    pub fn hide(&mut self) {
        if self.seen != Seen::Never {
            self.seen = Seen::Hidden;
        }
    }

    /// The caret is at `pos`, whose place in the pane is `at`, at the time
    /// `now`. A caret that first shows starts from `takeoff`, the writer's
    /// caret, when there is one to start from. A move takes `transition`.
    pub fn step(
        &mut self,
        pos: usize,
        at: Spot,
        takeoff: Option<Spot>,
        transition: Transition,
        now: Duration,
    ) -> CaretFrame {
        let from = match self.seen {
            Seen::At { pos: last, spot } => (last != pos).then_some(spot),
            Seen::Never => {
                self.start_over();
                takeoff
            }
            Seen::Hidden => {
                self.start_over();
                None
            }
        };
        if let Some(from) = from {
            self.travelled.0 += at.0 - from.0;
            self.travelled.1 += at.1 - from.1;
        }
        self.seen = Seen::At { pos, spot: at };
        let covered = self.covered.toward(self.travelled, transition, now);
        CaretFrame {
            spot: (
                at.0 - (self.travelled.0 - covered.value.0),
                at.1 - (self.travelled.1 - covered.value.1),
            ),
            moving: !covered.settled,
        }
    }

    fn start_over(&mut self) {
        self.travelled = (0., 0.);
        self.covered.jump_to((0., 0.));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ginga::{Easing, Tween};

    const WRITER: Spot = (40., 100.);
    const FIRST: Spot = (200., 300.);
    const SECOND: Spot = (80., 500.);

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn linear() -> Transition {
        Tween::new(ms(100), Easing::Linear).into()
    }

    fn at_rest(spot: Spot) -> CaretFrame {
        CaretFrame {
            spot,
            moving: false,
        }
    }

    fn on_its_way(spot: Spot) -> CaretFrame {
        CaretFrame { spot, moving: true }
    }

    #[test]
    fn a_new_caret_takes_off_from_the_writers_caret_and_glides_to_its_place() {
        let mut travel = CaretTravel::default();
        let step =
            |travel: &mut CaretTravel, now| travel.step(10, FIRST, Some(WRITER), linear(), ms(now));
        assert_eq!(step(&mut travel, 0), on_its_way(WRITER));
        assert_eq!(step(&mut travel, 50), on_its_way((120., 200.)));
        assert_eq!(step(&mut travel, 100), at_rest(FIRST));
    }

    #[test]
    fn a_move_to_another_place_starts_where_the_caret_was_drawn() {
        let mut travel = CaretTravel::default();
        assert_eq!(
            travel.step(10, FIRST, None, linear(), ms(0)),
            at_rest(FIRST)
        );
        assert_eq!(
            travel.step(90, SECOND, None, linear(), ms(1_000)),
            on_its_way(FIRST)
        );
        assert_eq!(
            travel.step(90, SECOND, None, linear(), ms(1_050)),
            on_its_way((140., 400.))
        );
        assert_eq!(
            travel.step(90, SECOND, None, linear(), ms(1_100)),
            at_rest(SECOND)
        );
    }

    #[test]
    fn a_move_in_the_middle_of_a_glide_does_not_jump() {
        let mut travel = CaretTravel::default();
        travel.step(10, FIRST, Some(WRITER), linear(), ms(0));
        let flying = travel.step(10, FIRST, None, linear(), ms(50));
        let turned = travel.step(90, SECOND, None, linear(), ms(50));
        assert_eq!(turned, flying);
        assert_eq!(
            travel.step(90, SECOND, None, linear(), ms(150)),
            at_rest(SECOND)
        );
    }

    #[test]
    fn scrolling_carries_the_caret_with_the_text_and_adds_no_travel() {
        let mut travel = CaretTravel::default();
        travel.step(10, FIRST, None, linear(), ms(0));
        assert_eq!(
            travel.step(10, (200., 120.), None, linear(), ms(16)),
            at_rest((200., 120.))
        );
    }

    #[test]
    fn scrolling_in_the_middle_of_a_glide_keeps_the_distance_left() {
        let mut travel = CaretTravel::default();
        travel.step(10, FIRST, Some(WRITER), linear(), ms(0));
        let scrolled = (FIRST.0, FIRST.1 - 180.);
        assert_eq!(
            travel.step(10, scrolled, None, linear(), ms(50)),
            on_its_way((120., 20.))
        );
    }

    #[test]
    fn with_nowhere_to_take_off_from_the_caret_shows_in_place() {
        let mut travel = CaretTravel::default();
        assert_eq!(
            travel.step(10, FIRST, None, linear(), ms(0)),
            at_rest(FIRST)
        );
    }

    #[test]
    fn with_reduced_motion_the_caret_shows_in_place() {
        let mut travel = CaretTravel::default();
        let frame = travel.step(10, FIRST, Some(WRITER), Transition::Instant, ms(0));
        assert_eq!(frame, at_rest(FIRST));
    }

    #[test]
    fn a_caret_back_in_view_shows_in_place() {
        let mut travel = CaretTravel::default();
        travel.step(10, FIRST, None, linear(), ms(0));
        travel.hide();
        assert_eq!(
            travel.step(90, SECOND, Some(WRITER), linear(), ms(500)),
            at_rest(SECOND)
        );
    }

    #[test]
    fn the_next_caret_after_one_is_gone_takes_off_again() {
        let mut travel = CaretTravel::default();
        travel.step(10, FIRST, None, linear(), ms(0));
        travel.land();
        travel.hide();
        assert_eq!(
            travel.step(90, SECOND, Some(WRITER), linear(), ms(500)),
            on_its_way(WRITER)
        );
    }

    #[test]
    fn the_real_trip_covers_most_of_the_way_at_once_and_lands_in_560_ms() {
        let trip: Transition = Tween::new(ms(560), Easing::OutExpo).into();
        let mut travel = CaretTravel::default();
        let rows: Vec<String> = [0, 140, 280, 420, 560]
            .into_iter()
            .map(|now| {
                let frame = travel.step(10, FIRST, Some(WRITER), trip, ms(now));
                let state = if frame.moving { "moving" } else { "at rest" };
                format!(
                    "{now:>3} ms  {:>5.1} {:>5.1}  {state}",
                    frame.spot.0, frame.spot.1
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                "  0 ms   40.0 100.0  moving",
                "140 ms  171.7 264.6  moving",
                "280 ms  195.0 293.8  moving",
                "420 ms  199.1 298.9  moving",
                "560 ms  200.0 300.0  at rest",
            ]
        );
    }
}
