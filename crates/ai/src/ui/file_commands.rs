use super::keys::KeyPress;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCommand {
    New,
    Open,
    Save,
    SaveAs,
}

pub fn file_command_of(event: &KeyPress, apple: bool) -> Option<FileCommand> {
    if !event.platform_modifier(apple) || event.alt {
        return None;
    }
    match event.lower_key().as_str() {
        "s" if event.shift => Some(FileCommand::SaveAs),
        "s" => Some(FileCommand::Save),
        "o" if !event.shift => Some(FileCommand::Open),
        "n" if !event.shift => Some(FileCommand::New),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_ctrl_o_ctrl_s_and_ctrl_shift_s_outside_macos() {
        assert_eq!(
            file_command_of(&KeyPress::ctrl("o"), false),
            Some(FileCommand::Open)
        );
        assert_eq!(
            file_command_of(&KeyPress::ctrl("s"), false),
            Some(FileCommand::Save)
        );
        assert_eq!(
            file_command_of(&KeyPress::ctrl("S").with_shift(), false),
            Some(FileCommand::SaveAs)
        );
    }

    #[test]
    fn maps_ctrl_n_and_cmd_n_to_new() {
        assert_eq!(
            file_command_of(&KeyPress::ctrl("n"), false),
            Some(FileCommand::New)
        );
        assert_eq!(
            file_command_of(&KeyPress::meta("n"), true),
            Some(FileCommand::New)
        );
        assert_eq!(
            file_command_of(&KeyPress::ctrl("N").with_shift(), false),
            None
        );
    }

    #[test]
    fn maps_cmd_on_macos() {
        assert_eq!(
            file_command_of(&KeyPress::meta("o"), true),
            Some(FileCommand::Open)
        );
        assert_eq!(
            file_command_of(&KeyPress::meta("s"), true),
            Some(FileCommand::Save)
        );
        assert_eq!(
            file_command_of(&KeyPress::meta("s").with_shift(), true),
            Some(FileCommand::SaveAs)
        );
    }

    #[test]
    fn ignores_the_other_platform_modifier() {
        assert_eq!(file_command_of(&KeyPress::meta("s"), false), None);
        assert_eq!(file_command_of(&KeyPress::ctrl("s"), true), None);
        assert_eq!(
            file_command_of(&KeyPress::ctrl("s").with_meta(), false),
            None
        );
    }

    #[test]
    fn ignores_alt_shift_o_plain_keys_and_other_letters() {
        assert_eq!(
            file_command_of(&KeyPress::ctrl("s").with_alt(), false),
            None
        );
        assert_eq!(
            file_command_of(&KeyPress::ctrl("O").with_shift(), false),
            None
        );
        assert_eq!(file_command_of(&KeyPress::new("s"), false), None);
        assert_eq!(file_command_of(&KeyPress::ctrl("k"), false), None);
    }
}
