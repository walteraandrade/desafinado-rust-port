#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChange {
    pub from: usize,
    pub to: usize,
    pub insert: String,
}

fn shared_prefix(before: &str, after: &str) -> usize {
    before
        .chars()
        .zip(after.chars())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum()
}

fn shared_suffix(before: &str, after: &str, prefix: usize) -> usize {
    before[prefix..]
        .chars()
        .rev()
        .zip(after[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum()
}

pub fn minimal_change(before: &str, after: &str) -> TextChange {
    let prefix = shared_prefix(before, after);
    let suffix = shared_suffix(before, after, prefix);
    TextChange {
        from: prefix,
        to: before.len() - suffix,
        insert: after[prefix..after.len() - suffix].to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(before: &str, change: &TextChange) -> String {
        format!(
            "{}{}{}",
            &before[..change.from],
            change.insert,
            &before[change.to..]
        )
    }

    fn change(from: usize, to: usize, insert: &str) -> TextChange {
        TextChange {
            from,
            to,
            insert: insert.to_owned(),
        }
    }

    #[test]
    fn replaces_only_the_middle_that_differs() {
        assert_eq!(
            minimal_change("# A\n\nOne.\n\nEnd.\n", "# A\n\nOne, two.\n\nEnd.\n"),
            change(8, 8, ", two")
        );
    }

    #[test]
    fn covers_a_removal_as_an_empty_insert() {
        assert_eq!(minimal_change("abcdef", "abef"), change(2, 4, ""));
    }

    #[test]
    fn covers_an_addition_at_the_end_and_at_the_start() {
        assert_eq!(minimal_change("abc", "abcdef"), change(3, 3, "def"));
        assert_eq!(minimal_change("def", "abcdef"), change(0, 0, "abc"));
    }

    #[test]
    fn does_not_count_the_same_characters_twice_when_prefix_and_suffix_meet() {
        let result = minimal_change("aaa", "aaaa");
        assert_eq!(apply("aaa", &result), "aaaa");
        assert!(result.to - result.from <= 3);
    }

    #[test]
    fn changes_nothing_for_equal_texts() {
        assert_eq!(minimal_change("same", "same"), change(4, 4, ""));
    }

    #[test]
    fn replaces_everything_when_nothing_is_shared() {
        assert_eq!(minimal_change("abc", "xyz"), change(0, 3, "xyz"));
    }

    #[test]
    fn never_splits_a_multi_unit_character() {
        let before = "a\u{1F600}b";
        let after = "a\u{1F601}b";
        let result = minimal_change(before, after);
        assert_eq!(apply(before, &result), after);
        assert_eq!(result.from, 1);
        assert_eq!(result.to, 5);
        assert_eq!(result.insert, "\u{1F601}");
    }

    #[test]
    fn rebuilds_the_new_text() {
        for (before, after) in [
            ("", "abc"),
            ("abc", ""),
            ("one\ntwo\nthree", "one\n2\nthree"),
            ("x\u{1F600}", "\u{1F600}x"),
        ] {
            assert_eq!(apply(before, &minimal_change(before, after)), after);
        }
    }
}
