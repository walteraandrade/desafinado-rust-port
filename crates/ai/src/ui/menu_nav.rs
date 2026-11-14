pub fn next_enabled(enabled: &[bool], from: isize, step: isize) -> isize {
    let count = enabled.len() as isize;
    if count == 0 {
        return -1;
    }
    let start = if from < 0 {
        if step == 1 { -1 } else { count }
    } else {
        from
    };
    (0..count)
        .map(|offset| (((start + step * (offset + 1)) % count) + count) % count)
        .find(|index| enabled[*index as usize])
        .unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENABLED: [bool; 4] = [true, false, true, true];

    #[test]
    fn moves_down_to_the_next_enabled_item_and_wraps() {
        assert_eq!(next_enabled(&ENABLED, 0, 1), 2);
        assert_eq!(next_enabled(&ENABLED, 3, 1), 0);
    }

    #[test]
    fn moves_up_skipping_disabled_items_and_wraps() {
        assert_eq!(next_enabled(&ENABLED, 2, -1), 0);
        assert_eq!(next_enabled(&ENABLED, 0, -1), 3);
    }

    #[test]
    fn starts_from_the_first_or_last_enabled_item_when_none_is_active() {
        assert_eq!(next_enabled(&[false, true, true], -1, 1), 1);
        assert_eq!(next_enabled(&[true, true, false], -1, -1), 1);
    }

    #[test]
    fn returns_minus_one_when_nothing_is_enabled() {
        assert_eq!(next_enabled(&[false, false], 0, 1), -1);
        assert_eq!(next_enabled(&[], -1, 1), -1);
    }
}
