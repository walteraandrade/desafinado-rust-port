use super::keys::KeyPress;

pub fn is_preferences_shortcut(event: &KeyPress) -> bool {
    event.key == "," && (event.ctrl || event.meta) && !event.alt && !event.shift
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ctrl_comma_and_cmd_comma() {
        assert!(is_preferences_shortcut(&KeyPress::ctrl(",")));
        assert!(is_preferences_shortcut(&KeyPress::meta(",")));
    }

    #[test]
    fn ignores_a_bare_comma_and_other_modifiers() {
        assert!(!is_preferences_shortcut(&KeyPress::new(",")));
        assert!(!is_preferences_shortcut(&KeyPress::ctrl(",").with_shift()));
        assert!(!is_preferences_shortcut(&KeyPress::ctrl(",").with_alt()));
    }

    #[test]
    fn ignores_other_keys() {
        assert!(!is_preferences_shortcut(&KeyPress::ctrl(".")));
    }
}
