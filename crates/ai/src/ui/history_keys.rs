use super::keys::KeyPress;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryCommand {
    Undo,
    Redo,
}

pub fn history_command_of(event: &KeyPress, apple: bool) -> Option<HistoryCommand> {
    if !event.platform_modifier(apple) || event.alt {
        return None;
    }
    match event.lower_key().as_str() {
        "z" if event.shift => Some(HistoryCommand::Redo),
        "z" => Some(HistoryCommand::Undo),
        "y" if !apple && !event.shift => Some(HistoryCommand::Redo),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_ctrl_z_ctrl_shift_z_and_ctrl_y_outside_macos() {
        assert_eq!(
            history_command_of(&KeyPress::ctrl("z"), false),
            Some(HistoryCommand::Undo)
        );
        assert_eq!(
            history_command_of(&KeyPress::ctrl("Z").with_shift(), false),
            Some(HistoryCommand::Redo)
        );
        assert_eq!(
            history_command_of(&KeyPress::ctrl("y"), false),
            Some(HistoryCommand::Redo)
        );
    }

    #[test]
    fn maps_cmd_z_and_cmd_shift_z_on_macos_with_no_cmd_y() {
        assert_eq!(
            history_command_of(&KeyPress::meta("z"), true),
            Some(HistoryCommand::Undo)
        );
        assert_eq!(
            history_command_of(&KeyPress::meta("z").with_shift(), true),
            Some(HistoryCommand::Redo)
        );
        assert_eq!(history_command_of(&KeyPress::meta("y"), true), None);
    }

    #[test]
    fn ignores_the_other_platform_modifier_alt_and_plain_keys() {
        assert_eq!(history_command_of(&KeyPress::meta("z"), false), None);
        assert_eq!(history_command_of(&KeyPress::ctrl("z"), true), None);
        assert_eq!(
            history_command_of(&KeyPress::ctrl("z").with_alt(), false),
            None
        );
        assert_eq!(history_command_of(&KeyPress::new("z"), false), None);
        assert_eq!(history_command_of(&KeyPress::ctrl("a"), false), None);
    }

    #[test]
    fn leaves_ctrl_shift_y_alone() {
        assert_eq!(
            history_command_of(&KeyPress::ctrl("y").with_shift(), false),
            None
        );
    }
}
