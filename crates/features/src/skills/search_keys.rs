#[derive(Debug, Clone, Copy, Default)]
pub struct SearchKeyEvent<'a> {
    pub key: &'a str,
    pub ctrl_key: bool,
    pub meta_key: bool,
    pub alt_key: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct SearchKeyState<'a> {
    pub count: usize,
    pub active: usize,
    pub text: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKeyAction {
    None,
    Move { to: usize },
    Open { index: usize },
    Clear,
    Leave,
}

pub fn search_key(event: &SearchKeyEvent, state: &SearchKeyState) -> SearchKeyAction {
    if event.ctrl_key || event.meta_key || event.alt_key {
        return SearchKeyAction::None;
    }
    let SearchKeyState {
        count,
        active,
        text,
    } = *state;
    match event.key {
        "ArrowDown" if count > 0 => SearchKeyAction::Move {
            to: (active + 1) % count,
        },
        "ArrowUp" if count > 0 => SearchKeyAction::Move {
            to: (active + count - 1) % count,
        },
        "Enter" if count > 0 => SearchKeyAction::Open { index: active },
        "Escape" if text.is_empty() => SearchKeyAction::Leave,
        "Escape" => SearchKeyAction::Clear,
        _ => SearchKeyAction::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: &str, state: SearchKeyState) -> SearchKeyAction {
        search_key(
            &SearchKeyEvent {
                key,
                ..SearchKeyEvent::default()
            },
            &state,
        )
    }

    fn some() -> SearchKeyState<'static> {
        SearchKeyState {
            count: 3,
            active: 0,
            text: "pr",
        }
    }

    fn with_active(active: usize) -> SearchKeyState<'static> {
        SearchKeyState { active, ..some() }
    }

    #[test]
    fn moves_down_and_wraps_to_the_first_row() {
        assert_eq!(press("ArrowDown", some()), SearchKeyAction::Move { to: 1 });
        assert_eq!(
            press("ArrowDown", with_active(2)),
            SearchKeyAction::Move { to: 0 }
        );
    }

    #[test]
    fn moves_up_and_wraps_to_the_last_row() {
        assert_eq!(
            press("ArrowUp", with_active(2)),
            SearchKeyAction::Move { to: 1 }
        );
        assert_eq!(press("ArrowUp", some()), SearchKeyAction::Move { to: 2 });
    }

    #[test]
    fn leaves_the_arrows_to_the_field_when_there_are_no_rows() {
        let none = SearchKeyState {
            count: 0,
            active: 0,
            text: "zz",
        };
        assert_eq!(press("ArrowDown", none), SearchKeyAction::None);
        assert_eq!(press("ArrowUp", none), SearchKeyAction::None);
    }

    #[test]
    fn opens_the_highlighted_row_on_enter() {
        assert_eq!(
            press("Enter", with_active(1)),
            SearchKeyAction::Open { index: 1 }
        );
    }

    #[test]
    fn does_nothing_on_enter_without_rows() {
        assert_eq!(
            press(
                "Enter",
                SearchKeyState {
                    count: 0,
                    active: 0,
                    text: "zz"
                }
            ),
            SearchKeyAction::None
        );
    }

    #[test]
    fn clears_the_text_on_the_first_esc() {
        assert_eq!(press("Escape", some()), SearchKeyAction::Clear);
    }

    #[test]
    fn leaves_for_the_text_on_esc_with_no_text() {
        assert_eq!(
            press(
                "Escape",
                SearchKeyState {
                    count: 0,
                    active: 0,
                    text: ""
                }
            ),
            SearchKeyAction::Leave
        );
    }

    #[test]
    fn ignores_keys_with_a_modifier() {
        let ctrl = SearchKeyEvent {
            key: "ArrowDown",
            ctrl_key: true,
            ..SearchKeyEvent::default()
        };
        assert_eq!(search_key(&ctrl, &some()), SearchKeyAction::None);
        let meta = SearchKeyEvent {
            key: "Enter",
            meta_key: true,
            ..SearchKeyEvent::default()
        };
        assert_eq!(search_key(&meta, &some()), SearchKeyAction::None);
    }

    #[test]
    fn ignores_other_keys() {
        assert_eq!(press("a", some()), SearchKeyAction::None);
    }
}
