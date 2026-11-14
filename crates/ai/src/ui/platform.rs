pub fn is_apple_platform(platform: &str) -> bool {
    let lower = platform.to_lowercase();
    ["mac", "iphone", "ipad"]
        .iter()
        .any(|name| lower.contains(name))
}

pub fn format_shortcut(key: &str, apple: bool) -> String {
    if apple {
        format!("⌘{key}")
    } else {
        format!("Ctrl+{key}")
    }
}

pub fn format_ctrl_shortcut(key: &str, apple: bool) -> String {
    if apple {
        format!("⌃{key}")
    } else {
        format!("Ctrl+{key}")
    }
}

pub fn detect_apple_platform() -> bool {
    cfg!(target_os = "macos")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_apple_platforms() {
        assert!(is_apple_platform("MacIntel"));
        assert!(is_apple_platform("iPhone"));
    }

    #[test]
    fn treats_everything_else_as_non_apple() {
        assert!(!is_apple_platform("Linux x86_64"));
        assert!(!is_apple_platform("Win32"));
        assert!(!is_apple_platform(""));
    }

    #[test]
    fn uses_the_command_symbol_on_apple_platforms() {
        assert_eq!(format_shortcut("K", true), "⌘K");
    }

    #[test]
    fn uses_ctrl_elsewhere() {
        assert_eq!(format_shortcut("K", false), "Ctrl+K");
    }

    #[test]
    fn uses_the_control_symbol_on_apple_platforms_where_the_key_stays_control() {
        assert_eq!(format_ctrl_shortcut("H", true), "⌃H");
    }

    #[test]
    fn ctrl_shortcut_uses_ctrl_elsewhere() {
        assert_eq!(format_ctrl_shortcut("H", false), "Ctrl+H");
    }
}
