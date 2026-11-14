use super::keys::KeyPress;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LibraryFocus {
    pub open: bool,
    pub focused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryKeyAnswer {
    OpenAndFocus,
    Focus,
    FoldAndReturn,
}

pub const fn library_key_answer(focus: LibraryFocus) -> LibraryKeyAnswer {
    if !focus.open {
        LibraryKeyAnswer::OpenAndFocus
    } else if focus.focused {
        LibraryKeyAnswer::FoldAndReturn
    } else {
        LibraryKeyAnswer::Focus
    }
}

pub const fn rail_answer(open: bool) -> LibraryKeyAnswer {
    if open {
        LibraryKeyAnswer::FoldAndReturn
    } else {
        LibraryKeyAnswer::OpenAndFocus
    }
}

pub const ECHO_WINDOW_MS: u64 = 150;

pub fn is_echo_of_key(last_key_at: Option<u64>, now: u64) -> bool {
    last_key_at.is_some_and(|at| now.saturating_sub(at) < ECHO_WINDOW_MS)
}

pub const fn is_modal_open(open_dialogs: usize) -> bool {
    open_dialogs > 0
}

fn is_mod_shortcut(event: &KeyPress, apple: bool, key: &str) -> bool {
    event.lower_key() == key && event.platform_modifier(apple) && !event.alt && !event.shift
}

pub fn is_library_shortcut(event: &KeyPress, apple: bool) -> bool {
    is_mod_shortcut(event, apple, "b")
}

pub fn is_search_shortcut(event: &KeyPress, apple: bool) -> bool {
    is_mod_shortcut(event, apple, "p")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn focus(open: bool, focused: bool) -> LibraryFocus {
        LibraryFocus { open, focused }
    }

    #[test]
    fn opens_and_focuses_a_folded_library() {
        assert_eq!(
            library_key_answer(focus(false, false)),
            LibraryKeyAnswer::OpenAndFocus
        );
    }

    #[test]
    fn focuses_the_tree_of_an_open_library_when_the_focus_is_elsewhere() {
        assert_eq!(
            library_key_answer(focus(true, false)),
            LibraryKeyAnswer::Focus
        );
    }

    #[test]
    fn folds_an_open_focused_library_and_returns_to_the_text() {
        assert_eq!(
            library_key_answer(focus(true, true)),
            LibraryKeyAnswer::FoldAndReturn
        );
    }

    #[test]
    fn opens_a_folded_library_even_if_a_stale_focus_flag_is_set() {
        assert_eq!(
            library_key_answer(focus(false, true)),
            LibraryKeyAnswer::OpenAndFocus
        );
    }

    #[test]
    fn accepts_ctrl_b_off_apple_platforms() {
        assert!(is_library_shortcut(&KeyPress::ctrl("b"), false));
        assert!(is_library_shortcut(&KeyPress::ctrl("B"), false));
    }

    #[test]
    fn accepts_cmd_b_on_apple_platforms_only() {
        assert!(is_library_shortcut(&KeyPress::meta("b"), true));
        assert!(!is_library_shortcut(&KeyPress::meta("b"), false));
    }

    #[test]
    fn leaves_ctrl_b_to_the_text_on_apple_platforms() {
        assert!(!is_library_shortcut(&KeyPress::ctrl("b"), true));
    }

    #[test]
    fn ignores_a_bare_b_other_modifiers_and_other_keys() {
        assert!(!is_library_shortcut(&KeyPress::new("b"), false));
        assert!(!is_library_shortcut(
            &KeyPress::ctrl("b").with_shift(),
            false
        ));
        assert!(!is_library_shortcut(&KeyPress::ctrl("b").with_alt(), false));
        assert!(!is_library_shortcut(&KeyPress::ctrl("p"), false));
    }

    #[test]
    fn rail_folds_an_open_library_whatever_holds_the_focus() {
        assert_eq!(rail_answer(true), LibraryKeyAnswer::FoldAndReturn);
    }

    #[test]
    fn rail_opens_a_folded_library_and_focuses_the_tree() {
        assert_eq!(rail_answer(false), LibraryKeyAnswer::OpenAndFocus);
    }

    #[test]
    fn is_an_echo_within_the_window_after_a_handled_key() {
        assert!(is_echo_of_key(Some(1000), 1000));
        assert!(is_echo_of_key(Some(1000), 1000 + ECHO_WINDOW_MS - 1));
        assert!(!is_echo_of_key(Some(1000), 1000 + ECHO_WINDOW_MS));
    }

    #[test]
    fn is_not_an_echo_without_a_handled_key() {
        assert!(!is_echo_of_key(None, 1000));
    }

    #[test]
    fn sees_an_open_dialog() {
        assert!(is_modal_open(1));
        assert!(!is_modal_open(0));
    }

    #[test]
    fn search_accepts_ctrl_p_off_apple_platforms_and_cmd_p_on_them() {
        assert!(is_search_shortcut(&KeyPress::ctrl("p"), false));
        assert!(is_search_shortcut(&KeyPress::meta("P"), true));
    }

    #[test]
    fn search_leaves_ctrl_p_to_the_text_on_apple_platforms_and_cmd_p_elsewhere() {
        assert!(!is_search_shortcut(&KeyPress::ctrl("p"), true));
        assert!(!is_search_shortcut(&KeyPress::meta("p"), false));
    }

    #[test]
    fn search_ignores_alt_shift_and_other_letters() {
        assert!(!is_search_shortcut(&KeyPress::ctrl("p").with_alt(), false));
        assert!(!is_search_shortcut(
            &KeyPress::ctrl("p").with_shift(),
            false
        ));
        assert!(!is_search_shortcut(&KeyPress::ctrl("b"), false));
    }
}
