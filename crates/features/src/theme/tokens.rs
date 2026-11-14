use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeRoles {
    pub canvas: String,
    pub raised: String,
    pub ink: String,
    pub faint: String,
    pub accent: String,
    pub ai: String,
    pub danger: String,
    pub selection: String,
    pub fill: String,
    pub fill_hover: String,
    pub hairline: String,
    pub scrollbar: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeTokens {
    pub name: String,
    pub mode: ThemeMode,
    pub roles: ThemeRoles,
}

pub const THEME_VARIABLE_NAMES: [&str; 12] = [
    "--theme-canvas",
    "--theme-raised",
    "--theme-ink",
    "--theme-faint",
    "--theme-accent",
    "--theme-ai",
    "--theme-danger",
    "--theme-selection",
    "--theme-fill",
    "--theme-fill-hover",
    "--theme-hairline",
    "--theme-scrollbar",
];

pub fn theme_variables(tokens: &ThemeTokens) -> Vec<(&'static str, String)> {
    let roles = &tokens.roles;
    let values = [
        &roles.canvas,
        &roles.raised,
        &roles.ink,
        &roles.faint,
        &roles.accent,
        &roles.ai,
        &roles.danger,
        &roles.selection,
        &roles.fill,
        &roles.fill_hover,
        &roles.hairline,
        &roles.scrollbar,
    ];
    THEME_VARIABLE_NAMES
        .iter()
        .zip(values)
        .map(|(name, value)| (*name, value.clone()))
        .collect()
}

pub fn theme_label(name: &str) -> String {
    name.split(|c: char| c == '-' || c == '_' || crate::text::is_js_space(c))
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(name: &str) -> ThemeTokens {
        let hex = |n: u32| format!("#{n:06x}");
        ThemeTokens {
            name: name.into(),
            mode: ThemeMode::Dark,
            roles: ThemeRoles {
                canvas: hex(1),
                raised: hex(2),
                ink: hex(3),
                faint: hex(4),
                accent: hex(5),
                ai: hex(6),
                danger: hex(7),
                selection: hex(8),
                fill: hex(9),
                fill_hover: hex(10),
                hairline: hex(11),
                scrollbar: hex(12),
            },
        }
    }

    #[test]
    fn maps_every_role_to_a_theme_custom_property() {
        let variables = theme_variables(&tokens("tokyo-night"));
        let expected = [
            ("--theme-canvas", "#000001"),
            ("--theme-raised", "#000002"),
            ("--theme-ink", "#000003"),
            ("--theme-faint", "#000004"),
            ("--theme-accent", "#000005"),
            ("--theme-ai", "#000006"),
            ("--theme-danger", "#000007"),
            ("--theme-selection", "#000008"),
            ("--theme-fill", "#000009"),
            ("--theme-fill-hover", "#00000a"),
            ("--theme-hairline", "#00000b"),
            ("--theme-scrollbar", "#00000c"),
        ];
        let expected: Vec<(&str, String)> = expected
            .iter()
            .map(|(name, value)| (*name, value.to_string()))
            .collect();
        assert_eq!(variables, expected);
    }

    #[test]
    fn names_the_variables_it_writes_so_they_can_be_cleared() {
        let mut written: Vec<&str> = theme_variables(&tokens("x"))
            .iter()
            .map(|(n, _)| *n)
            .collect();
        let mut names = THEME_VARIABLE_NAMES.to_vec();
        written.sort();
        names.sort();
        assert_eq!(names, written);
    }

    #[test]
    fn title_cases_a_slug() {
        assert_eq!(theme_label("tokyo-night"), "Tokyo Night");
        assert_eq!(theme_label("vantablack"), "Vantablack");
        assert_eq!(theme_label("rose_pine  moon"), "Rose Pine Moon");
    }
}
