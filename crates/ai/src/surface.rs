//! A surface that comes and goes, such as a menu or a popover, kept in line
//! with what should be open.
//!
//! Each frame the view says what should be on screen, or nothing. What it
//! says is kept as the surface's face, so a surface that leaves is drawn as it
//! looked when it closed, not from the state that closed it.

use std::time::Duration;

use ginga::{InOut, Presence, Shown};

/// A menu or a popover on screen, which may be leaving.
#[derive(Debug, Clone)]
pub struct Surface<T> {
    shown: Presence<T>,
}

impl<T> Default for Surface<T> {
    fn default() -> Self {
        Self {
            shown: Presence::absent(),
        }
    }
}

impl<T> Surface<T> {
    /// Brings the surface in line with `wanted` at `now`.
    ///
    /// `None` plays the way out of what is shown, which keeps the face it had.
    /// A face that `same` says is the same surface takes the place of the one
    /// shown, and turns around if it was leaving, from as far in as it still
    /// is. Any other face replaces what is shown at once, leaving or not, so
    /// two surfaces never show.
    pub fn follow(
        &mut self,
        wanted: Option<T>,
        same: impl FnOnce(&T, &T) -> bool,
        in_out: InOut,
        now: Duration,
    ) {
        let Some(face) = wanted else {
            self.shown.hide(in_out.exit, now);
            return;
        };
        if self
            .shown
            .sample(now)
            .is_some_and(|held| !same(held.content, &face))
        {
            self.shown = Presence::absent();
        }
        self.shown.show(face, in_out.enter, now);
    }

    /// Takes away at once a surface that is leaving, so it does not cover
    /// what opens in its place. One that is wanted stays.
    pub fn drop_leaving(&mut self) {
        if !self.shown.is_wanted() {
            self.shown = Presence::absent();
        }
    }

    /// What to draw at `now`, or `None` when nothing is here.
    pub fn frame(&mut self, now: Duration) -> Option<Shown<'_, T>> {
        self.shown.frame(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ginga::{Easing, Phase, Tween};

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Menu {
        File,
        Edit,
    }

    /// A menu and the label of its first row.
    type Face = (Menu, &'static str);

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn in_out() -> InOut {
        let tween = Tween::new(ms(100), Easing::Linear);
        InOut::new(tween, tween)
    }

    fn follow(surface: &mut Surface<Face>, wanted: Option<Face>, now: u64) {
        surface.follow(wanted, |a, b| a.0 == b.0, in_out(), ms(now));
    }

    fn at(surface: &mut Surface<Face>, now: u64) -> Option<(Face, Phase, f32)> {
        surface
            .frame(ms(now))
            .map(|shown| (*shown.content, shown.phase, shown.progress))
    }

    fn open_file_menu() -> Surface<Face> {
        let mut surface = Surface::default();
        follow(&mut surface, Some((Menu::File, "Save")), 0);
        follow(&mut surface, Some((Menu::File, "Save")), 100);
        surface
    }

    #[test]
    fn a_closed_menu_plays_its_way_out_and_is_gone() {
        let mut surface = open_file_menu();
        follow(&mut surface, None, 1_000);
        let (_, phase, progress) = at(&mut surface, 1_050).unwrap();
        assert_eq!(phase, Phase::Exiting);
        assert!((progress - 0.5).abs() < 1e-3);
        assert_eq!(at(&mut surface, 1_100), None);
    }

    #[test]
    fn a_leaving_menu_keeps_the_face_it_had_when_it_closed() {
        let mut surface = open_file_menu();
        follow(&mut surface, None, 1_000);
        follow(&mut surface, None, 1_020);
        assert_eq!(at(&mut surface, 1_050).unwrap().0, (Menu::File, "Save"));
    }

    #[test]
    fn an_open_menu_takes_its_new_face() {
        let mut surface = open_file_menu();
        follow(&mut surface, Some((Menu::File, "Save as…")), 1_000);
        let (face, phase, _) = at(&mut surface, 1_000).unwrap();
        assert_eq!((face, phase), ((Menu::File, "Save as…"), Phase::Present));
    }

    #[test]
    fn the_same_menu_reopened_turns_around_from_where_it_is() {
        let mut surface = open_file_menu();
        follow(&mut surface, None, 1_000);
        follow(&mut surface, Some((Menu::File, "Save")), 1_050);
        let (_, phase, progress) = at(&mut surface, 1_050).unwrap();
        assert_eq!(phase, Phase::Entering);
        assert!((progress - 0.5).abs() < 1e-3);
        // The rest of the way in, a half, takes the whole tween.
        let (_, phase, progress) = at(&mut surface, 1_100).unwrap();
        assert!((progress - 0.75).abs() < 1e-3);
        assert_eq!(phase, Phase::Entering);
        assert_eq!(at(&mut surface, 1_150).unwrap().1, Phase::Present);
    }

    #[test]
    fn another_menu_replaces_a_leaving_one_at_once() {
        let mut surface = open_file_menu();
        follow(&mut surface, None, 1_000);
        follow(&mut surface, Some((Menu::Edit, "Undo")), 1_050);
        for t in [1_050, 1_080, 1_200] {
            let (face, phase, _) = at(&mut surface, t).unwrap();
            assert_eq!(face.0, Menu::Edit, "at {t} ms");
            assert_ne!(phase, Phase::Exiting, "at {t} ms");
        }
    }

    #[test]
    fn another_menu_replaces_an_open_one_and_arrives() {
        let mut surface = open_file_menu();
        follow(&mut surface, Some((Menu::Edit, "Undo")), 1_000);
        let (face, phase, progress) = at(&mut surface, 1_000).unwrap();
        assert_eq!((face.0, phase), (Menu::Edit, Phase::Entering));
        assert!(progress.abs() < 1e-3);
    }

    #[test]
    fn a_leaving_surface_dropped_is_gone_at_once() {
        let mut surface = open_file_menu();
        follow(&mut surface, None, 1_000);
        surface.drop_leaving();
        assert_eq!(at(&mut surface, 1_010), None);
    }

    #[test]
    fn a_wanted_surface_is_not_dropped() {
        let mut surface = open_file_menu();
        surface.drop_leaving();
        assert_eq!(at(&mut surface, 1_000).unwrap().1, Phase::Present);
    }
}
