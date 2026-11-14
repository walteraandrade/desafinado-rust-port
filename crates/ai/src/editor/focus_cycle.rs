pub fn cycle_index(count: usize, current: isize, backward: bool) -> isize {
    if count == 0 {
        return -1;
    }
    let count = count as isize;
    if current < 0 {
        return if backward { count - 1 } else { 0 };
    }
    (current + if backward { count - 1 } else { 1 }) % count
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub alt: bool,
    pub ctrl: bool,
    pub meta: bool,
    pub shift: bool,
}

pub fn tab_cycle_target(
    count: usize,
    current: isize,
    key: &str,
    modifiers: Modifiers,
) -> Option<usize> {
    if key != "Tab" || modifiers.alt || modifiers.ctrl || modifiers.meta {
        return None;
    }
    usize::try_from(cycle_index(count, current, modifiers.shift)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_forward_to_the_next_item() {
        assert_eq!(cycle_index(5, 1, false), 2);
    }

    #[test]
    fn wraps_from_the_last_item_to_the_first() {
        assert_eq!(cycle_index(5, 4, false), 0);
    }

    #[test]
    fn moves_backward_to_the_previous_item() {
        assert_eq!(cycle_index(5, 3, true), 2);
    }

    #[test]
    fn wraps_from_the_first_item_to_the_last() {
        assert_eq!(cycle_index(5, 0, true), 4);
    }

    #[test]
    fn enters_at_the_first_item_when_focus_is_outside() {
        assert_eq!(cycle_index(5, -1, false), 0);
    }

    #[test]
    fn enters_at_the_last_item_when_focus_is_outside_going_backward() {
        assert_eq!(cycle_index(5, -1, true), 4);
    }

    #[test]
    fn stays_on_a_single_item() {
        assert_eq!(cycle_index(1, 0, false), 0);
        assert_eq!(cycle_index(1, 0, true), 0);
    }

    #[test]
    fn returns_minus_one_when_there_is_nothing_to_focus() {
        assert_eq!(cycle_index(0, -1, false), -1);
    }

    #[test]
    fn tab_cycles_only_a_plain_tab_or_shift_tab() {
        let plain = Modifiers::default();
        assert_eq!(tab_cycle_target(3, 2, "Tab", plain), Some(0));
        assert_eq!(
            tab_cycle_target(
                3,
                0,
                "Tab",
                Modifiers {
                    shift: true,
                    ..plain
                }
            ),
            Some(2)
        );
        assert_eq!(tab_cycle_target(3, 0, "Enter", plain), None);
        assert_eq!(
            tab_cycle_target(
                3,
                0,
                "Tab",
                Modifiers {
                    ctrl: true,
                    ..plain
                }
            ),
            None
        );
        assert_eq!(tab_cycle_target(0, -1, "Tab", plain), None);
    }
}
