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
}
