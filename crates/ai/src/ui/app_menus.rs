use super::platform::format_shortcut;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppMenuState {
    pub apple: bool,
    pub document_ready: bool,
    pub review_ready: bool,
    pub review_running: bool,
    pub open_suggestions: usize,
    pub problems: Vec<String>,
}

impl Default for AppMenuState {
    fn default() -> Self {
        Self {
            apple: false,
            document_ready: true,
            review_ready: true,
            review_running: false,
            open_suggestions: 0,
            problems: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    StartNew,
    Open,
    Save,
    SaveAs,
    ShowProblems,
    OpenPreferences,
    Review,
    StopReview,
    AcceptAll,
    RejectAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuTone {
    Danger,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub label: String,
    pub shortcut: Option<String>,
    pub disabled: bool,
    pub tone: Option<MenuTone>,
    pub separated: bool,
    pub action: MenuAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    pub label: String,
    pub items: Vec<MenuItem>,
    pub badge: Option<String>,
}

fn problem_count(count: usize) -> String {
    if count == 1 {
        "1 problem".to_owned()
    } else {
        format!("{count} problems")
    }
}

fn item(label: &str, action: MenuAction) -> MenuItem {
    MenuItem {
        label: label.to_owned(),
        shortcut: None,
        disabled: false,
        tone: None,
        separated: false,
        action,
    }
}

impl MenuItem {
    fn shortcut(self, shortcut: String) -> Self {
        Self {
            shortcut: Some(shortcut),
            ..self
        }
    }

    fn disabled(self, disabled: bool) -> Self {
        Self { disabled, ..self }
    }

    fn separated(self) -> Self {
        Self {
            separated: true,
            ..self
        }
    }
}

fn file_menu(state: &AppMenuState) -> Menu {
    let apple = state.apple;
    let blocked = !state.document_ready;
    let problem_items = if state.problems.is_empty() {
        Vec::new()
    } else {
        vec![MenuItem {
            tone: Some(MenuTone::Danger),
            ..item(
                &format!("Frontmatter: {}", problem_count(state.problems.len())),
                MenuAction::ShowProblems,
            )
            .separated()
        }]
    };
    let mut items = vec![
        item("New", MenuAction::StartNew)
            .shortcut(format_shortcut("N", apple))
            .disabled(blocked),
        item("Open…", MenuAction::Open)
            .shortcut(format_shortcut("O", apple))
            .disabled(blocked),
        item("Save", MenuAction::Save)
            .shortcut(format_shortcut("S", apple))
            .disabled(blocked),
        item("Save as…", MenuAction::SaveAs)
            .shortcut(if apple { "⇧⌘S" } else { "Ctrl+Shift+S" }.to_owned())
            .disabled(blocked),
    ];
    items.extend(problem_items);
    items.push(
        item("Preferences…", MenuAction::OpenPreferences)
            .shortcut(format_shortcut(",", apple))
            .separated(),
    );
    Menu {
        label: "File".to_owned(),
        badge: (!state.problems.is_empty()).then(|| state.problems.len().to_string()),
        items,
    }
}

fn edit_menu(state: &AppMenuState) -> Menu {
    let no_suggestions = state.open_suggestions == 0;
    let review = if state.review_running {
        item("Stop Review", MenuAction::StopReview)
    } else {
        item("Review…", MenuAction::Review).disabled(!state.review_ready)
    };
    Menu {
        label: "Edit".to_owned(),
        badge: None,
        items: vec![
            review,
            item("Accept all suggestions", MenuAction::AcceptAll)
                .disabled(no_suggestions)
                .separated(),
            item("Reject all suggestions", MenuAction::RejectAll).disabled(no_suggestions),
        ],
    }
}

pub fn app_menus(state: &AppMenuState) -> Vec<Menu> {
    vec![file_menu(state), edit_menu(state)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(menu: &Menu) -> Vec<&str> {
        menu.items.iter().map(|i| i.label.as_str()).collect()
    }

    fn shortcuts(menu: &Menu) -> Vec<Option<&str>> {
        menu.items.iter().map(|i| i.shortcut.as_deref()).collect()
    }

    #[test]
    fn builds_file_and_edit_with_their_shortcuts() {
        let menus = app_menus(&AppMenuState::default());
        let (file, edit) = (&menus[0], &menus[1]);
        assert_eq!(file.label, "File");
        assert_eq!(
            labels(file),
            ["New", "Open…", "Save", "Save as…", "Preferences…"]
        );
        assert_eq!(
            shortcuts(file),
            [
                Some("Ctrl+N"),
                Some("Ctrl+O"),
                Some("Ctrl+S"),
                Some("Ctrl+Shift+S"),
                Some("Ctrl+,")
            ]
        );
        assert_eq!(edit.label, "Edit");
        assert_eq!(
            labels(edit),
            [
                "Review…",
                "Accept all suggestions",
                "Reject all suggestions"
            ]
        );
    }

    #[test]
    fn uses_macos_shortcuts() {
        let menus = app_menus(&AppMenuState {
            apple: true,
            ..AppMenuState::default()
        });
        assert_eq!(
            shortcuts(&menus[0]),
            [Some("⌘N"), Some("⌘O"), Some("⌘S"), Some("⇧⌘S"), Some("⌘,")]
        );
    }

    #[test]
    fn disables_the_file_commands_until_the_document_is_ready() {
        let menus = app_menus(&AppMenuState {
            document_ready: false,
            ..AppMenuState::default()
        });
        let disabled: Vec<bool> = menus[0].items.iter().map(|i| i.disabled).collect();
        assert_eq!(disabled, [true, true, true, true, false]);
    }

    #[test]
    fn wires_save_and_save_as() {
        let menus = app_menus(&AppMenuState::default());
        assert_eq!(menus[0].items[2].action, MenuAction::Save);
        assert_eq!(menus[0].items[3].action, MenuAction::SaveAs);
    }

    #[test]
    fn lists_frontmatter_problems_in_file_and_badges_the_menu() {
        let menus = app_menus(&AppMenuState {
            problems: vec!["a".into(), "b".into()],
            ..AppMenuState::default()
        });
        assert_eq!(menus[0].badge.as_deref(), Some("2"));
        let danger = menus[0]
            .items
            .iter()
            .find(|i| i.tone == Some(MenuTone::Danger))
            .unwrap();
        assert_eq!(danger.label, "Frontmatter: 2 problems");
        assert_eq!(danger.action, MenuAction::ShowProblems);
        let one = app_menus(&AppMenuState {
            problems: vec!["a".into()],
            ..AppMenuState::default()
        });
        assert!(
            one[0]
                .items
                .iter()
                .any(|i| i.label == "Frontmatter: 1 problem")
        );
    }

    #[test]
    fn turns_review_into_stop_review_while_a_pass_runs() {
        let menus = app_menus(&AppMenuState {
            review_running: true,
            ..AppMenuState::default()
        });
        assert_eq!(menus[1].items[0].label, "Stop Review");
        assert_eq!(menus[1].items[0].action, MenuAction::StopReview);
    }

    #[test]
    fn enables_accept_all_and_reject_all_only_with_open_suggestions() {
        let idle = app_menus(&AppMenuState::default());
        assert!(idle[1].items[1..].iter().all(|i| i.disabled));
        let busy = app_menus(&AppMenuState {
            open_suggestions: 3,
            ..AppMenuState::default()
        });
        assert!(!busy[1].items[1..].iter().any(|i| i.disabled));
    }

    #[test]
    fn disables_review_until_the_editor_can_review() {
        let menus = app_menus(&AppMenuState {
            review_ready: false,
            ..AppMenuState::default()
        });
        assert!(menus[1].items[0].disabled);
    }
}
