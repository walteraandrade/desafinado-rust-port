use std::collections::HashMap;
use std::fs;
use std::path::Path;

use regex::Regex;

use super::color::{mix_hex, rgb_of};
use super::tokens::{ThemeMode, ThemeRoles, ThemeTokens};

pub type Colors = HashMap<String, String>;

fn pair_pattern() -> Regex {
    Regex::new(r#"^([A-Za-z0-9_-]+)\s*=\s*(?:"(.*?)"|'(.*?)')\s*(?:#.*)?$"#).expect("pair pattern")
}

fn hex_pattern() -> Regex {
    Regex::new(r"^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$").expect("hex pattern")
}

pub fn parse_colors_toml(text: &str) -> Colors {
    let pair = pair_pattern();
    text.split('\n')
        .filter_map(|line| {
            let captures = pair.captures(line.trim())?;
            let value = captures.get(2).or_else(|| captures.get(3))?;
            Some((captures[1].to_string(), value.as_str().to_string()))
        })
        .collect()
}

fn hex_of(colors: &Colors, key: &str) -> Option<String> {
    colors
        .get(key)
        .filter(|value| hex_pattern().is_match(value))
        .cloned()
}

fn luminance(hex: &str) -> f64 {
    let [r, g, b] = rgb_of(hex).map(|channel| f64::from(channel) / 255.0);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn mode_of(colors: &Colors, background: &str) -> ThemeMode {
    match colors.get("mode").map(String::as_str) {
        Some("light") => ThemeMode::Light,
        Some("dark") => ThemeMode::Dark,
        _ if luminance(background) > 0.5 => ThemeMode::Light,
        _ => ThemeMode::Dark,
    }
}

pub fn omarchy_tokens(colors: &Colors, name: &str) -> Option<ThemeTokens> {
    let bg = hex_of(colors, "background")?;
    let ink = hex_of(colors, "foreground")?;
    let accent = hex_of(colors, "accent")?;
    let mode = mode_of(colors, &bg);
    Some(ThemeTokens {
        name: name.to_string(),
        mode,
        roles: ThemeRoles {
            canvas: bg.clone(),
            raised: hex_of(colors, "lighter_background")
                .unwrap_or_else(|| mix_hex(&bg, &ink, 0.09)),
            ink: ink.clone(),
            faint: mix_hex(&bg, &ink, 0.7),
            accent: accent.clone(),
            ai: accent.clone(),
            danger: hex_of(colors, "red").unwrap_or_else(|| {
                if mode == ThemeMode::Light {
                    "#b42318"
                } else {
                    "#f87171"
                }
                .to_string()
            }),
            selection: hex_of(colors, "selection").unwrap_or_else(|| mix_hex(&bg, &accent, 0.28)),
            fill: mix_hex(&bg, &ink, 0.06),
            fill_hover: mix_hex(&bg, &ink, 0.11),
            hairline: mix_hex(&bg, &ink, 0.09),
            scrollbar: mix_hex(&bg, &ink, 0.16),
        },
    })
}

pub fn read_omarchy_theme(state_dir: &Path) -> Option<ThemeTokens> {
    let toml = fs::read_to_string(state_dir.join("theme").join("colors.toml")).ok()?;
    let name = fs::read_to_string(state_dir.join("theme.name"))
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "omarchy".to_string());
    omarchy_tokens(&parse_colors_toml(&toml), &name)
}

pub fn default_omarchy_state_dir(home: &Path) -> std::path::PathBuf {
    home.join(".local")
        .join("state")
        .join("omarchy")
        .join("current")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKYO_NIGHT: &str = "mode = \"dark\"\n\naccent = \"#7aa2f7\"\nselection = \"#292e42\"\nmuted = \"#414868\"\n\nbackground = \"#1a1b26\"\nlighter_background = \"#24283b\"\n\nforeground = \"#a9b1d6\"\nbright_foreground = \"#c0caf5\"\n\nred = \"#f7768e\"\n";

    fn colors(pairs: &[(&str, &str)]) -> Colors {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn reads_quoted_key_value_pairs() {
        let parsed = parse_colors_toml(TOKYO_NIGHT);
        assert_eq!(parsed["mode"], "dark");
        assert_eq!(parsed["accent"], "#7aa2f7");
        assert_eq!(parsed["background"], "#1a1b26");
    }

    #[test]
    fn skips_comments_sections_and_malformed_lines() {
        let parsed = parse_colors_toml(
            "# comment\n[section]\naccent = \"#ffffff\" # trailing\nbroken =\nred = #ff0000\n",
        );
        assert_eq!(parsed, colors(&[("accent", "#ffffff")]));
    }

    #[test]
    fn accepts_single_quotes_and_spaces_around_the_equal_sign() {
        assert_eq!(
            parse_colors_toml("  background='#000000'  "),
            colors(&[("background", "#000000")])
        );
    }

    #[test]
    fn maps_an_omarchy_palette_to_theme_tokens() {
        let tokens = omarchy_tokens(&parse_colors_toml(TOKYO_NIGHT), "tokyo-night").unwrap();
        assert_eq!(
            tokens,
            ThemeTokens {
                name: "tokyo-night".into(),
                mode: ThemeMode::Dark,
                roles: ThemeRoles {
                    canvas: "#1a1b26".into(),
                    raised: "#24283b".into(),
                    ink: "#a9b1d6".into(),
                    faint: mix_hex("#1a1b26", "#a9b1d6", 0.7),
                    accent: "#7aa2f7".into(),
                    ai: "#7aa2f7".into(),
                    danger: "#f7768e".into(),
                    selection: "#292e42".into(),
                    fill: mix_hex("#1a1b26", "#a9b1d6", 0.06),
                    fill_hover: mix_hex("#1a1b26", "#a9b1d6", 0.11),
                    hairline: mix_hex("#1a1b26", "#a9b1d6", 0.09),
                    scrollbar: mix_hex("#1a1b26", "#a9b1d6", 0.16),
                },
            }
        );
    }

    #[test]
    fn returns_none_without_background_foreground_or_accent() {
        let mut parsed = parse_colors_toml(TOKYO_NIGHT);
        parsed.remove("accent");
        assert_eq!(omarchy_tokens(&parsed, "x"), None);
    }

    #[test]
    fn rejects_colors_that_are_not_hex() {
        assert_eq!(
            omarchy_tokens(
                &colors(&[
                    ("background", "black"),
                    ("foreground", "#ffffff"),
                    ("accent", "#ff0000")
                ]),
                "x"
            ),
            None
        );
    }

    #[test]
    fn fills_optional_colors_from_the_required_ones() {
        let tokens = omarchy_tokens(
            &colors(&[
                ("background", "#eff1f5"),
                ("foreground", "#4c4f69"),
                ("accent", "#1e66f5"),
            ]),
            "latte",
        )
        .unwrap();
        assert_eq!(tokens.mode, ThemeMode::Light);
        assert_eq!(tokens.roles.raised, mix_hex("#eff1f5", "#4c4f69", 0.09));
        assert_eq!(tokens.roles.danger, "#b42318");
        assert_eq!(tokens.roles.selection, mix_hex("#eff1f5", "#1e66f5", 0.28));
    }

    #[test]
    fn picks_a_readable_red_for_a_dark_theme_without_one() {
        let tokens = omarchy_tokens(
            &colors(&[
                ("background", "#101010"),
                ("foreground", "#eeeeee"),
                ("accent", "#ff0000"),
            ]),
            "x",
        )
        .unwrap();
        assert_eq!(tokens.roles.danger, "#f87171");
    }

    #[test]
    fn trusts_an_explicit_mode_over_the_background() {
        let tokens = omarchy_tokens(
            &colors(&[
                ("mode", "light"),
                ("background", "#101010"),
                ("foreground", "#888888"),
                ("accent", "#ff0000"),
            ]),
            "x",
        )
        .unwrap();
        assert_eq!(tokens.mode, ThemeMode::Light);
    }

    #[test]
    fn reads_short_hex_and_ignores_an_unknown_mode() {
        let tokens = omarchy_tokens(
            &colors(&[
                ("mode", "dim"),
                ("background", "#fff"),
                ("foreground", "#333"),
                ("accent", "#f00"),
            ]),
            "x",
        )
        .unwrap();
        assert_eq!(tokens.mode, ThemeMode::Light);
    }

    #[test]
    fn infers_the_mode_from_the_background_when_mode_is_missing() {
        let mode = |background| {
            omarchy_tokens(
                &colors(&[
                    ("background", background),
                    ("foreground", "#888888"),
                    ("accent", "#ff0000"),
                ]),
                "x",
            )
            .unwrap()
            .mode
        };
        assert_eq!(mode("#101010"), ThemeMode::Dark);
        assert_eq!(mode("#fafafa"), ThemeMode::Light);
    }

    fn state_dir(files: &[(&str, &str)]) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "omarchy-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(dir.join("theme")).unwrap();
        for (path, text) in files {
            fs::write(dir.join(path), text).unwrap();
        }
        dir
    }

    #[test]
    fn reads_the_current_theme_and_its_name() {
        let dir = state_dir(&[
            ("theme/colors.toml", TOKYO_NIGHT),
            ("theme.name", "tokyo-night\n"),
        ]);
        let tokens = read_omarchy_theme(&dir).unwrap();
        assert_eq!(tokens.name, "tokyo-night");
        assert_eq!(tokens.roles.canvas, "#1a1b26");
    }

    #[test]
    fn falls_back_to_a_generic_name_without_theme_name() {
        let dir = state_dir(&[("theme/colors.toml", TOKYO_NIGHT)]);
        assert_eq!(read_omarchy_theme(&dir).unwrap().name, "omarchy");
    }

    #[test]
    fn is_none_without_colors_toml() {
        assert_eq!(read_omarchy_theme(&state_dir(&[])), None);
    }
}
