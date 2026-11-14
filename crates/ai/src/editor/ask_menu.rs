use crate::quick_actions::QuickAction;
use crate::text::js_trim;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskRow {
    Action {
        action: QuickAction,
        match_span: Option<(usize, usize)>,
    },
    Free {
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AskMenu {
    pub query: String,
    pub browsing: bool,
    pub active: usize,
}

pub const EMPTY_ASK_MENU: AskMenu = AskMenu {
    query: String::new(),
    browsing: false,
    active: 0,
};

fn browse_row(action: &QuickAction) -> AskRow {
    AskRow::Action {
        action: action.clone(),
        match_span: None,
    }
}

fn word_start(label: &str, needle: &str) -> Option<usize> {
    let lower = label.to_lowercase();
    if lower.starts_with(needle) {
        return Some(0);
    }
    lower.find(&format!(" {needle}")).map(|found| found + 1)
}

fn match_rows(actions: &[QuickAction], query: &str) -> Vec<AskRow> {
    let needle = query.to_lowercase();
    let matched: Vec<(&QuickAction, usize)> = actions
        .iter()
        .filter_map(|action| word_start(&action.label, &needle).map(|from| (action, from)))
        .collect();
    let starts = matched.iter().filter(|(_, from)| *from == 0);
    let words = matched.iter().filter(|(_, from)| *from > 0);
    starts
        .chain(words)
        .map(|(action, from)| AskRow::Action {
            action: (*action).clone(),
            match_span: Some((*from, from + needle.len())),
        })
        .collect()
}

pub fn ask_rows(actions: &[QuickAction], query: &str, browsing: bool) -> Vec<AskRow> {
    let text = js_trim(query);
    if text.is_empty() {
        return if browsing {
            actions.iter().map(browse_row).collect()
        } else {
            Vec::new()
        };
    }
    let mut rows = match_rows(actions, text);
    rows.push(AskRow::Free {
        text: text.to_owned(),
    });
    rows
}

pub fn menu_rows(actions: &[QuickAction], menu: &AskMenu) -> Vec<AskRow> {
    ask_rows(actions, &menu.query, menu.browsing)
}

pub fn type_ask_menu(query: &str) -> AskMenu {
    AskMenu {
        query: query.to_owned(),
        browsing: false,
        active: 0,
    }
}

pub fn move_ask_menu(actions: &[QuickAction], menu: &AskMenu, step: i8) -> AskMenu {
    if menu.query.is_empty() && !menu.browsing {
        return AskMenu {
            browsing: true,
            active: 0,
            ..menu.clone()
        };
    }
    let count = menu_rows(actions, menu).len();
    if count == 0 {
        return menu.clone();
    }
    let next = (menu.active as isize + isize::from(step) + count as isize) % count as isize;
    AskMenu {
        active: next as usize,
        ..menu.clone()
    }
}

pub fn hover_ask_menu(menu: &AskMenu, active: usize) -> AskMenu {
    AskMenu {
        active,
        ..menu.clone()
    }
}

pub fn chosen_row(actions: &[QuickAction], menu: &AskMenu) -> Option<AskRow> {
    menu_rows(actions, menu).into_iter().nth(menu.active)
}

pub fn hint_visible(menu: &AskMenu) -> bool {
    menu.query.is_empty() && !menu.browsing
}

pub fn ask_hint(actions: &[QuickAction]) -> Option<String> {
    actions.first().map(|first| {
        format!(
            "Try “{}”, or press ↓ for all actions",
            first.label.to_lowercase()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quick_actions::get_quick_action;

    fn action(id: &str) -> QuickAction {
        get_quick_action(id).unwrap()
    }

    fn actions() -> Vec<QuickAction> {
        ["shorter", "clearer", "fix-grammar", "cut-known"]
            .iter()
            .map(|id| action(id))
            .collect()
    }

    fn labels(query: &str, browsing: bool) -> Vec<String> {
        ask_rows(&actions(), query, browsing)
            .into_iter()
            .map(|row| match row {
                AskRow::Action { action, .. } => action.label,
                AskRow::Free { text } => format!("free: {text}"),
            })
            .collect()
    }

    fn ids_of(rows: Vec<AskRow>) -> Vec<String> {
        rows.into_iter()
            .map(|row| match row {
                AskRow::Action { action, .. } => action.id,
                AskRow::Free { .. } => "free".to_owned(),
            })
            .collect()
    }

    #[test]
    fn shows_nothing_for_an_empty_query() {
        assert_eq!(ask_rows(&actions(), "", false), vec![]);
        assert_eq!(ask_rows(&actions(), "   ", false), vec![]);
    }

    #[test]
    fn lists_every_action_without_free_text_when_browsing_an_empty_query() {
        assert_eq!(
            labels("", true),
            [
                "Shorter",
                "Clearer",
                "Fix grammar",
                "Cut what the agent already knows"
            ]
        );
    }

    #[test]
    fn ignores_browsing_once_there_is_a_query() {
        assert_eq!(labels("sh", true), ["Shorter", "free: sh"]);
    }

    #[test]
    fn matches_the_start_of_a_label_case_insensitively_and_ends_with_the_free_text() {
        assert_eq!(labels("SH", false), ["Shorter", "free: SH"]);
    }

    #[test]
    fn matches_the_start_of_any_word_not_the_middle_of_one() {
        assert_eq!(labels("gram", false), ["Fix grammar", "free: gram"]);
        assert_eq!(labels("r", false), ["free: r"]);
    }

    #[test]
    fn puts_label_starts_before_word_starts_keeping_list_order_within_each() {
        let list: Vec<QuickAction> = [
            "more-formal",
            "cut-known",
            "fix-grammar",
            "make-instruction",
        ]
        .iter()
        .map(|id| action(id))
        .collect();
        assert_eq!(
            ids_of(ask_rows(&list, "m", false)),
            ["more-formal", "make-instruction", "free"]
        );
        assert_eq!(
            ids_of(ask_rows(&list, "a", false)),
            ["cut-known", "make-instruction", "free"]
        );
    }

    #[test]
    fn falls_back_to_free_text_when_nothing_matches() {
        assert_eq!(labels("shorter please", false), ["free: shorter please"]);
    }

    #[test]
    fn marks_the_matched_span_of_each_label() {
        let cut = action("cut-known");
        let from = cut.label.find("agent").unwrap();
        assert_eq!(
            ask_rows(&actions(), " agent ", false)[0],
            AskRow::Action {
                action: cut,
                match_span: Some((from, from + "agent".len())),
            }
        );
    }

    #[test]
    fn trims_the_free_text() {
        assert_eq!(
            ask_rows(&actions(), "  tone only ", false).last(),
            Some(&AskRow::Free {
                text: "tone only".into()
            })
        );
    }

    #[test]
    fn starts_at_rest_with_the_hint_visible() {
        assert_eq!(chosen_row(&actions(), &EMPTY_ASK_MENU), None);
        assert!(hint_visible(&EMPTY_ASK_MENU));
    }

    #[test]
    fn chooses_the_first_row_after_typing_and_hides_the_hint() {
        match chosen_row(&actions(), &type_ask_menu("cl")) {
            Some(AskRow::Action { action, .. }) => assert_eq!(action.id, "clearer"),
            other => panic!("unexpected {other:?}"),
        }
        assert!(!hint_visible(&type_ask_menu("cl")));
        assert!(!hint_visible(&type_ask_menu(" ")));
    }

    #[test]
    fn opens_every_action_on_a_move_from_an_empty_query() {
        let browsing = move_ask_menu(&actions(), &EMPTY_ASK_MENU, -1);
        assert_eq!(
            browsing,
            AskMenu {
                query: String::new(),
                browsing: true,
                active: 0
            }
        );
        assert!(!hint_visible(&browsing));
    }

    #[test]
    fn wraps_moves_at_both_ends() {
        let menu = type_ask_menu("c");
        assert_eq!(move_ask_menu(&actions(), &menu, -1).active, 2);
        let at_end = AskMenu { active: 2, ..menu };
        assert_eq!(move_ask_menu(&actions(), &at_end, 1).active, 0);
    }

    #[test]
    fn resets_the_browsing_and_the_highlight_on_typing() {
        assert_eq!(
            type_ask_menu("f"),
            AskMenu {
                query: "f".into(),
                browsing: false,
                active: 0
            }
        );
    }

    #[test]
    fn chooses_the_free_text_when_nothing_matches() {
        assert_eq!(
            chosen_row(&actions(), &type_ask_menu("tone only")),
            Some(AskRow::Free {
                text: "tone only".into()
            })
        );
    }

    #[test]
    fn follows_the_pointer() {
        assert_eq!(
            chosen_row(&actions(), &hover_ask_menu(&type_ask_menu("c"), 2)),
            Some(AskRow::Free { text: "c".into() })
        );
    }

    #[test]
    fn does_nothing_on_a_move_without_actions_or_rows() {
        let opened = move_ask_menu(&[], &EMPTY_ASK_MENU, 1);
        assert_eq!(
            opened,
            AskMenu {
                query: String::new(),
                browsing: true,
                active: 0
            }
        );
        assert_eq!(chosen_row(&[], &opened), None);
        assert_eq!(
            move_ask_menu(&actions(), &type_ask_menu(" "), 1),
            type_ask_menu(" ")
        );
    }

    #[test]
    fn hint_names_the_first_action() {
        assert_eq!(
            ask_hint(&actions()).as_deref(),
            Some("Try “shorter”, or press ↓ for all actions")
        );
    }

    #[test]
    fn hint_is_empty_without_actions() {
        assert_eq!(ask_hint(&[]), None);
    }
}
