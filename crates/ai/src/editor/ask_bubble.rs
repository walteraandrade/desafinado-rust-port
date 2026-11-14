#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BubbleRange {
    pub from: usize,
    pub to: usize,
}

impl BubbleRange {
    pub const fn is_empty(self) -> bool {
        self.from == self.to
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BubbleState {
    pub ask_open: bool,
    pub dismissed: Option<BubbleRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BubbleChange {
    pub selection: BubbleRange,
    pub moved: bool,
    pub opened: bool,
    pub closed: bool,
}

pub fn next_bubble(state: BubbleState, change: BubbleChange) -> BubbleState {
    let ask_open = if change.closed {
        false
    } else if change.opened {
        true
    } else {
        state.ask_open && !change.moved
    };
    let dismissed = if change.closed {
        Some(change.selection)
    } else {
        state
            .dismissed
            .filter(|dismissed| *dismissed == change.selection)
    };
    BubbleState {
        ask_open,
        dismissed,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BubbleView {
    pub focused: bool,
    pub pointer_down: bool,
    pub selection: BubbleRange,
    pub has_target: bool,
}

pub fn bubble_visible(state: BubbleState, view: BubbleView) -> bool {
    if !view.focused || !view.has_target {
        return false;
    }
    state.ask_open
        || (!view.pointer_down
            && !view.selection.is_empty()
            && state.dismissed != Some(view.selection))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY: BubbleRange = BubbleRange { from: 4, to: 4 };
    const WORDS: BubbleRange = BubbleRange { from: 2, to: 9 };

    fn change(selection: BubbleRange) -> BubbleChange {
        BubbleChange {
            selection,
            moved: false,
            opened: false,
            closed: false,
        }
    }

    fn view(selection: BubbleRange) -> BubbleView {
        BubbleView {
            focused: true,
            pointer_down: false,
            selection,
            has_target: true,
        }
    }

    #[test]
    fn shows_for_a_selection_without_being_asked() {
        assert!(bubble_visible(BubbleState::default(), view(WORDS)));
        assert!(!bubble_visible(BubbleState::default(), view(EMPTY)));
    }

    #[test]
    fn waits_for_the_pointer_to_lift_and_needs_focus_and_a_target() {
        let state = BubbleState::default();
        assert!(!bubble_visible(
            state,
            BubbleView {
                pointer_down: true,
                ..view(WORDS)
            }
        ));
        assert!(!bubble_visible(
            state,
            BubbleView {
                focused: false,
                ..view(WORDS)
            }
        ));
        assert!(!bubble_visible(
            state,
            BubbleView {
                has_target: false,
                ..view(WORDS)
            }
        ));
    }

    #[test]
    fn opens_on_request_at_the_cursor_and_closes_when_the_cursor_moves() {
        let opened = next_bubble(
            BubbleState::default(),
            BubbleChange {
                opened: true,
                ..change(EMPTY)
            },
        );
        assert!(bubble_visible(opened, view(EMPTY)));
        let moved = next_bubble(
            opened,
            BubbleChange {
                moved: true,
                ..change(BubbleRange { from: 5, to: 5 })
            },
        );
        assert!(!moved.ask_open);
    }

    #[test]
    fn stays_dismissed_for_the_same_selection_until_it_changes() {
        let closed = next_bubble(
            BubbleState::default(),
            BubbleChange {
                closed: true,
                ..change(WORDS)
            },
        );
        assert!(!bubble_visible(closed, view(WORDS)));
        let same = next_bubble(closed, change(WORDS));
        assert!(!bubble_visible(same, view(WORDS)));
        let other = BubbleRange { from: 2, to: 10 };
        let changed = next_bubble(
            same,
            BubbleChange {
                moved: true,
                ..change(other)
            },
        );
        assert!(bubble_visible(changed, view(other)));
    }
}
