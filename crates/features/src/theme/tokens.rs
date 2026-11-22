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
    pub overlay: String,
    pub heading: String,
    pub ink: String,
    pub muted: String,
    pub faint: String,
    pub accent: String,
    pub ai: String,
    pub ai_fill: String,
    pub danger: String,
    pub danger_fill: String,
    pub success: String,
    pub success_fill: String,
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

pub const THEME_VARIABLE_NAMES: [&str; 19] = [
    "--theme-canvas",
    "--theme-raised",
    "--theme-overlay",
    "--theme-heading",
    "--theme-ink",
    "--theme-muted",
    "--theme-faint",
    "--theme-accent",
    "--theme-ai",
    "--theme-ai-fill",
    "--theme-danger",
    "--theme-danger-fill",
    "--theme-success",
    "--theme-success-fill",
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
        &roles.overlay,
        &roles.heading,
        &roles.ink,
        &roles.muted,
        &roles.faint,
        &roles.accent,
        &roles.ai,
        &roles.ai_fill,
        &roles.danger,
        &roles.danger_fill,
        &roles.success,
        &roles.success_fill,
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
                overlay: hex(3),
                heading: hex(4),
                ink: hex(5),
                muted: hex(6),
                faint: hex(7),
                accent: hex(8),
                ai: hex(9),
                ai_fill: hex(10),
                danger: hex(11),
                danger_fill: hex(12),
                success: hex(13),
                success_fill: hex(14),
                selection: hex(15),
                fill: hex(16),
                fill_hover: hex(17),
                hairline: hex(18),
                scrollbar: hex(19),
            },
        }
    }

    #[test]
    fn maps_every_role_to_a_theme_custom_property() {
        let variables = theme_variables(&tokens("tokyo-night"));
        let expected = [
            ("--theme-canvas", "#000001"),
            ("--theme-raised", "#000002"),
            ("--theme-overlay", "#000003"),
            ("--theme-heading", "#000004"),
            ("--theme-ink", "#000005"),
            ("--theme-muted", "#000006"),
            ("--theme-faint", "#000007"),
            ("--theme-accent", "#000008"),
            ("--theme-ai", "#000009"),
            ("--theme-ai-fill", "#00000a"),
            ("--theme-danger", "#00000b"),
            ("--theme-danger-fill", "#00000c"),
            ("--theme-success", "#00000d"),
            ("--theme-success-fill", "#00000e"),
            ("--theme-selection", "#00000f"),
            ("--theme-fill", "#000010"),
            ("--theme-fill-hover", "#000011"),
            ("--theme-hairline", "#000012"),
            ("--theme-scrollbar", "#000013"),
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
        written.sort_unstable();
        names.sort_unstable();
        assert_eq!(names, written);
    }

    #[test]
    fn title_cases_a_slug() {
        assert_eq!(theme_label("tokyo-night"), "Tokyo Night");
        assert_eq!(theme_label("vantablack"), "Vantablack");
        assert_eq!(theme_label("rose_pine  moon"), "Rose Pine Moon");
    }
}
