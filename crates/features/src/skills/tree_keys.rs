use super::library_tree::{TreeItem, TreeKind};

#[derive(Debug, Clone, Copy, Default)]
pub struct TreeKeyEvent<'a> {
    pub key: &'a str,
    pub ctrl_key: bool,
    pub meta_key: bool,
    pub alt_key: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeAction {
    None,
    Focus { id: String },
    Toggle { id: String, expand: bool },
    Open { id: String, focus_text: bool },
    Remove { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeKeyResult {
    pub action: TreeAction,
    pub typed: String,
    pub handled: bool,
}

fn item_at(items: &[TreeItem], index: isize) -> Option<&TreeItem> {
    usize::try_from(index).ok().and_then(|at| items.get(at))
}

fn focus(item: Option<&TreeItem>) -> TreeAction {
    item.map_or(TreeAction::None, |found| TreeAction::Focus {
        id: found.node.id.clone(),
    })
}

fn activate(item: &TreeItem, focus_text: bool) -> TreeAction {
    if item.node.kind == TreeKind::File {
        if item.node.openable == Some(true) {
            TreeAction::Open {
                id: item.node.id.clone(),
                focus_text,
            }
        } else {
            TreeAction::None
        }
    } else if item.expandable {
        TreeAction::Toggle {
            id: item.node.id.clone(),
            expand: !item.expanded,
        }
    } else {
        TreeAction::None
    }
}

fn matches_from<'a>(items: &'a [TreeItem], from: usize, prefix: &str) -> Option<&'a TreeItem> {
    let split = from.min(items.len());
    items[split..]
        .iter()
        .chain(items[..split].iter())
        .find(|item| item.node.name.to_lowercase().starts_with(prefix))
}

fn type_ahead(items: &[TreeItem], at: isize, character: &str, typed: &str) -> TreeKeyResult {
    let buffer = format!("{typed}{character}").to_lowercase();
    let first = buffer.chars().next();
    let cycling = buffer.chars().all(|letter| Some(letter) == first);
    let prefix = if cycling {
        first.map(String::from).unwrap_or_default()
    } else {
        buffer.clone()
    };
    let from = if cycling {
        usize::try_from(at + 1).unwrap_or(0)
    } else {
        usize::try_from(at.max(0)).unwrap_or(0)
    };
    TreeKeyResult {
        action: focus(matches_from(items, from, &prefix)),
        typed: buffer,
        handled: true,
    }
}

fn is_character(key: &str) -> bool {
    key.chars().count() == 1
}

fn toggle(item: &TreeItem, expand: bool) -> TreeAction {
    TreeAction::Toggle {
        id: item.node.id.clone(),
        expand,
    }
}

fn navigate(items: &[TreeItem], at: isize, key: &str) -> Option<TreeAction> {
    let current = item_at(items, at);
    match key {
        "ArrowDown" => Some(focus(item_at(items, at + 1))),
        "ArrowUp" if at == -1 => Some(focus(items.first())),
        "ArrowUp" => Some(focus(item_at(items, at - 1))),
        "Home" => Some(focus(items.first())),
        "End" => Some(focus(items.last())),
        "ArrowRight" => Some(match current {
            Some(item) if item.expandable => {
                if item.expanded {
                    focus(item_at(items, at + 1))
                } else {
                    toggle(item, true)
                }
            }
            _ => TreeAction::None,
        }),
        "ArrowLeft" => Some(match current {
            None => TreeAction::None,
            Some(item) if item.expanded => toggle(item, false),
            Some(item) => item
                .parent_id
                .clone()
                .map_or(TreeAction::None, |id| TreeAction::Focus { id }),
        }),
        "Enter" => Some(current.map_or(TreeAction::None, |item| activate(item, true))),
        "Delete" => Some(match current {
            Some(item) if item.node.kind == TreeKind::Root && item.node.removable == Some(true) => {
                TreeAction::Remove {
                    id: item.node.id.clone(),
                }
            }
            _ => TreeAction::None,
        }),
        _ => None,
    }
}

pub fn tree_key(
    items: &[TreeItem],
    focused_id: Option<&str>,
    event: &TreeKeyEvent,
    typed: &str,
) -> TreeKeyResult {
    let unhandled = TreeKeyResult {
        action: TreeAction::None,
        typed: String::new(),
        handled: false,
    };
    if event.ctrl_key || event.meta_key || event.alt_key || items.is_empty() {
        return unhandled;
    }
    let at = items
        .iter()
        .position(|item| Some(item.node.id.as_str()) == focused_id)
        .map_or(-1, |index| index as isize);
    let current = item_at(items, at);
    if event.key == " " && typed.is_empty() {
        return TreeKeyResult {
            action: current.map_or(TreeAction::None, |item| activate(item, false)),
            typed: String::new(),
            handled: true,
        };
    }
    if let Some(action) = navigate(items, at, event.key) {
        return TreeKeyResult {
            action,
            typed: String::new(),
            handled: true,
        };
    }
    if is_character(event.key) {
        type_ahead(items, at, event.key, typed)
    } else {
        unhandled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::library_tree::TreeNode;

    fn item(
        id: &str,
        kind: TreeKind,
        name: &str,
        level: usize,
        parent_id: Option<&str>,
        configure: impl FnOnce(&mut TreeNode, &mut bool, &mut bool),
    ) -> TreeItem {
        let mut node = TreeNode::new(id, kind, name);
        let mut expandable = false;
        let mut expanded = false;
        configure(&mut node, &mut expandable, &mut expanded);
        TreeItem {
            node,
            level,
            parent_id: parent_id.map(str::to_string),
            expandable,
            expanded,
        }
    }

    fn items() -> Vec<TreeItem> {
        vec![
            item(
                "r",
                TreeKind::Root,
                "Personal",
                1,
                None,
                |node, expandable, expanded| {
                    *expandable = true;
                    *expanded = true;
                    node.removable = Some(true);
                },
            ),
            item(
                "s1",
                TreeKind::Skill,
                "alpha",
                2,
                Some("r"),
                |_, expandable, expanded| {
                    *expandable = true;
                    *expanded = true;
                },
            ),
            item(
                "f1",
                TreeKind::File,
                "SKILL.md",
                3,
                Some("s1"),
                |node, _, _| {
                    node.openable = Some(true);
                },
            ),
            item(
                "f2",
                TreeKind::File,
                "run.sh",
                3,
                Some("s1"),
                |node, _, _| {
                    node.openable = Some(false);
                },
            ),
            item(
                "s2",
                TreeKind::Skill,
                "beta",
                2,
                Some("r"),
                |_, expandable, _| {
                    *expandable = true;
                },
            ),
            item(
                "s3",
                TreeKind::Skill,
                "bravo",
                2,
                Some("r"),
                |_, expandable, _| {
                    *expandable = true;
                },
            ),
            item("r2", TreeKind::Root, "Project", 1, None, |_, _, _| {}),
        ]
    }

    fn press_with(
        key: &str,
        focused: Option<&str>,
        typed: &str,
        modifiers: TreeKeyEvent,
    ) -> TreeKeyResult {
        tree_key(&items(), focused, &TreeKeyEvent { key, ..modifiers }, typed)
    }

    fn press(key: &str, focused: Option<&str>) -> TreeKeyResult {
        press_with(key, focused, "", TreeKeyEvent::default())
    }

    fn focus_id(id: &str) -> TreeAction {
        TreeAction::Focus { id: id.to_string() }
    }

    fn toggle_id(id: &str, expand: bool) -> TreeAction {
        TreeAction::Toggle {
            id: id.to_string(),
            expand,
        }
    }

    fn open_id(id: &str, focus_text: bool) -> TreeAction {
        TreeAction::Open {
            id: id.to_string(),
            focus_text,
        }
    }

    #[test]
    fn arrow_down_goes_to_the_next_item() {
        assert_eq!(press("ArrowDown", Some("s1")).action, focus_id("f1"));
    }

    #[test]
    fn arrow_up_goes_to_the_previous_item() {
        assert_eq!(press("ArrowUp", Some("f1")).action, focus_id("s1"));
    }

    #[test]
    fn arrows_stop_at_the_ends() {
        assert_eq!(press("ArrowUp", Some("r")).action, TreeAction::None);
        assert_eq!(press("ArrowDown", Some("r2")).action, TreeAction::None);
    }

    #[test]
    fn home_and_end_go_to_the_first_and_last_item() {
        assert_eq!(press("Home", Some("s3")).action, focus_id("r"));
        assert_eq!(press("End", Some("r")).action, focus_id("r2"));
    }

    #[test]
    fn arrows_start_from_the_first_item_when_nothing_is_focused() {
        assert_eq!(press("ArrowDown", None).action, focus_id("r"));
        assert_eq!(press("ArrowUp", None).action, focus_id("r"));
    }

    #[test]
    fn arrows_do_nothing_in_an_empty_tree() {
        assert_eq!(
            tree_key(
                &[],
                None,
                &TreeKeyEvent {
                    key: "ArrowDown",
                    ..TreeKeyEvent::default()
                },
                ""
            )
            .action,
            TreeAction::None
        );
    }

    #[test]
    fn arrow_right_unfolds_a_folded_node() {
        assert_eq!(
            press("ArrowRight", Some("s2")).action,
            toggle_id("s2", true)
        );
    }

    #[test]
    fn arrow_right_moves_to_the_first_child_of_an_unfolded_node() {
        assert_eq!(press("ArrowRight", Some("s1")).action, focus_id("f1"));
    }

    #[test]
    fn arrow_right_does_nothing_on_a_file_or_a_node_with_no_children() {
        assert_eq!(press("ArrowRight", Some("f1")).action, TreeAction::None);
        assert_eq!(press("ArrowRight", Some("r2")).action, TreeAction::None);
    }

    #[test]
    fn arrow_left_folds_an_unfolded_node() {
        assert_eq!(
            press("ArrowLeft", Some("s1")).action,
            toggle_id("s1", false)
        );
    }

    #[test]
    fn arrow_left_moves_a_file_or_folded_node_to_its_parent() {
        assert_eq!(press("ArrowLeft", Some("f1")).action, focus_id("s1"));
        assert_eq!(press("ArrowLeft", Some("s2")).action, focus_id("r"));
    }

    #[test]
    fn arrow_left_does_nothing_on_a_top_level_node_that_is_folded() {
        assert_eq!(press("ArrowLeft", Some("r2")).action, TreeAction::None);
    }

    #[test]
    fn enter_opens_a_markdown_file_and_puts_the_cursor_in_the_text() {
        assert_eq!(press("Enter", Some("f1")).action, open_id("f1", true));
    }

    #[test]
    fn space_opens_it_and_keeps_the_focus_in_the_tree() {
        assert_eq!(press(" ", Some("f1")).action, open_id("f1", false));
    }

    #[test]
    fn enter_and_space_do_not_open_a_file_that_is_not_markdown() {
        assert_eq!(press("Enter", Some("f2")).action, TreeAction::None);
        assert_eq!(press(" ", Some("f2")).action, TreeAction::None);
    }

    #[test]
    fn enter_and_space_fold_or_unfold_a_node() {
        assert_eq!(press("Enter", Some("s1")).action, toggle_id("s1", false));
        assert_eq!(press(" ", Some("s2")).action, toggle_id("s2", true));
    }

    #[test]
    fn enter_does_nothing_on_a_node_with_no_children() {
        assert_eq!(press("Enter", Some("r2")).action, TreeAction::None);
    }

    #[test]
    fn delete_removes_a_root_that_can_be_removed() {
        assert_eq!(
            press("Delete", Some("r")).action,
            TreeAction::Remove {
                id: "r".to_string()
            }
        );
    }

    #[test]
    fn delete_does_nothing_on_a_root_that_cannot_be_removed() {
        assert_eq!(press("Delete", Some("r2")).action, TreeAction::None);
    }

    #[test]
    fn delete_does_nothing_on_a_skill_or_a_file() {
        assert_eq!(press("Delete", Some("s1")).action, TreeAction::None);
        assert_eq!(press("Delete", Some("f1")).action, TreeAction::None);
    }

    #[test]
    fn type_ahead_moves_to_the_next_item_whose_name_starts_with_the_letter() {
        let result = press("b", Some("r"));
        assert_eq!(result.action, focus_id("s2"));
        assert_eq!(result.typed, "b");
    }

    #[test]
    fn type_ahead_is_case_insensitive() {
        assert_eq!(press("A", Some("r")).action, focus_id("s1"));
    }

    #[test]
    fn type_ahead_repeating_one_letter_cycles_through_the_matches() {
        assert_eq!(
            press_with("b", Some("s2"), "b", TreeKeyEvent::default()).action,
            focus_id("s3")
        );
        assert_eq!(
            press_with("b", Some("s3"), "bb", TreeKeyEvent::default()).action,
            focus_id("s2")
        );
    }

    #[test]
    fn type_ahead_more_letters_narrow_the_match_starting_at_the_focused_item() {
        let result = press_with("r", Some("s2"), "b", TreeKeyEvent::default());
        assert_eq!(result.action, focus_id("s3"));
        assert_eq!(result.typed, "br");
    }

    #[test]
    fn type_ahead_keeps_the_focus_when_nothing_matches() {
        assert_eq!(press("z", Some("s1")).action, TreeAction::None);
    }

    #[test]
    fn type_ahead_space_is_a_letter_while_a_word_is_being_typed() {
        let result = press_with(" ", Some("s2"), "be", TreeKeyEvent::default());
        assert_eq!(result.action, TreeAction::None);
        assert_eq!(result.typed, "be ");
    }

    #[test]
    fn type_ahead_ignores_keys_with_ctrl_meta_or_alt() {
        let ctrl = TreeKeyEvent {
            ctrl_key: true,
            ..TreeKeyEvent::default()
        };
        let meta = TreeKeyEvent {
            meta_key: true,
            ..TreeKeyEvent::default()
        };
        assert_eq!(
            press_with("b", Some("r"), "", ctrl).action,
            TreeAction::None
        );
        assert_eq!(
            press_with("b", Some("r"), "", meta).action,
            TreeAction::None
        );
    }

    #[test]
    fn type_ahead_leaves_the_buffer_empty_after_a_navigation_key() {
        assert_eq!(
            press_with("ArrowDown", Some("r"), "be", TreeKeyEvent::default()).typed,
            ""
        );
    }

    #[test]
    fn type_ahead_ignores_other_named_keys() {
        assert_eq!(press("Tab", Some("r")).action, TreeAction::None);
        assert_eq!(press("Escape", Some("r")).action, TreeAction::None);
    }

    #[test]
    fn handled_is_true_for_keys_the_tree_answers_to_even_when_nothing_moves() {
        assert!(press("ArrowDown", Some("r2")).handled);
        assert!(press("ArrowUp", Some("r")).handled);
        assert!(press(" ", Some("f2")).handled);
        assert!(press("z", Some("s1")).handled);
    }

    #[test]
    fn handled_is_false_for_tab_escape_and_keys_with_modifiers() {
        assert!(!press("Tab", Some("r")).handled);
        assert!(!press("Escape", Some("r")).handled);
        let ctrl = TreeKeyEvent {
            ctrl_key: true,
            ..TreeKeyEvent::default()
        };
        assert!(!press_with("b", Some("r"), "", ctrl).handled);
    }
}
