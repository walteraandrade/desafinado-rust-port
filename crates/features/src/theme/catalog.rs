use std::sync::OnceLock;

use super::screen_palettes::{DESAFINADO_SEED_ID, THEME_SEEDS, ThemeSeed, roles_of_seed};
use super::tokens::{ThemeMode, ThemeRoles, ThemeTokens, theme_label};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeColor {
    pub name: String,
    pub hex: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeOption {
    pub id: String,
    pub title: String,
    pub origin: String,
    pub colors: Vec<ThemeColor>,
    pub tokens: ThemeTokens,
}

const DEFAULT_THEME_ID: &str = "mood-indigo";

pub fn roles_as_colors(roles: &ThemeRoles) -> Vec<ThemeColor> {
    let entries = [
        ("canvas", &roles.canvas),
        ("raised", &roles.raised),
        ("overlay", &roles.overlay),
        ("heading", &roles.heading),
        ("ink", &roles.ink),
        ("muted", &roles.muted),
        ("faint", &roles.faint),
        ("accent", &roles.accent),
        ("ai", &roles.ai),
        ("success", &roles.success),
        ("danger", &roles.danger),
    ];
    entries
        .iter()
        .fold(Vec::<ThemeColor>::new(), |mut colors, (name, hex)| {
            let color = ThemeColor {
                name: (*name).to_string(),
                hex: (*hex).clone(),
            };
            match colors.iter_mut().find(|known| known.hex == color.hex) {
                Some(known) => *known = color,
                None => colors.push(color),
            }
            colors
        })
}

fn origin_of(seed: &ThemeSeed) -> String {
    match seed.source {
        Some((first, second)) => format!("After Wada #{first} + #{second}"),
        None => "The original theme".into(),
    }
}

fn option_of(seed: &ThemeSeed) -> ThemeOption {
    let roles = roles_of_seed(seed);
    ThemeOption {
        id: seed.id.to_string(),
        title: seed.title.to_string(),
        origin: origin_of(seed),
        colors: roles_as_colors(&roles),
        tokens: ThemeTokens {
            name: seed.id.to_string(),
            mode: seed.mode,
            roles,
        },
    }
}

pub fn desafinado_theme() -> &'static ThemeOption {
    theme_by_id(DESAFINADO_SEED_ID).unwrap_or_else(|| panic!("No theme named {DESAFINADO_SEED_ID}"))
}

pub fn omarchy_option(id: &str, tokens: ThemeTokens) -> ThemeOption {
    ThemeOption {
        id: id.to_string(),
        title: "Omarchy".into(),
        origin: format!("Follows {}", theme_label(&tokens.name)),
        colors: roles_as_colors(&tokens.roles),
        tokens,
    }
}

pub fn theme_options() -> &'static [ThemeOption] {
    static OPTIONS: OnceLock<Vec<ThemeOption>> = OnceLock::new();
    OPTIONS.get_or_init(|| THEME_SEEDS.iter().map(option_of).collect())
}

pub fn default_theme() -> &'static ThemeOption {
    theme_options()
        .iter()
        .find(|option| option.id == DEFAULT_THEME_ID)
        .unwrap_or_else(|| panic!("No theme named {DEFAULT_THEME_ID}"))
}

pub fn themes_of_mode(mode: ThemeMode) -> Vec<&'static ThemeOption> {
    theme_options()
        .iter()
        .filter(|option| option.tokens.mode == mode)
        .collect()
}

pub fn theme_by_id(id: &str) -> Option<&'static ThemeOption> {
    theme_options().iter().find(|option| option.id == id)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::theme::color::{contrast, oklch_of};

    const AA: f64 = 4.5;

    fn hue_gap(a: f64, b: f64) -> f64 {
        let gap = (a - b).rem_euclid(360.0);
        gap.min(360.0 - gap)
    }

    fn oklab_distance(a: &str, b: &str) -> f64 {
        let lab = |hex: &str| {
            let [l, c, h] = oklch_of(hex);
            [l, c * h.to_radians().cos(), c * h.to_radians().sin()]
        };
        let (a, b) = (lab(a), lab(b));
        a.iter()
            .zip(b)
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f64>()
            .sqrt()
    }

    fn expect_legible(title: &str, roles: &ThemeRoles) {
        let floors = [&roles.canvas, &roles.raised, &roles.overlay];
        let controls = [&roles.fill, &roles.fill_hover, &roles.selection];
        let mut pairs: Vec<(&String, &String, f64)> = Vec::new();
        for text in [&roles.heading, &roles.ink, &roles.muted] {
            for background in floors.iter().chain(&controls) {
                pairs.push((text, background, AA));
            }
        }
        for text in [&roles.accent, &roles.ai, &roles.danger, &roles.success] {
            for background in floors {
                pairs.push((text, background, AA));
            }
        }
        for background in floors {
            pairs.push((&roles.faint, background, 3.0));
        }
        pairs.push((&roles.ai, &roles.ai_fill, AA));
        pairs.push((&roles.danger, &roles.danger_fill, AA));
        pairs.push((&roles.success, &roles.success_fill, AA));
        pairs.push((&roles.canvas, &roles.ai, AA));
        pairs.push((&roles.canvas, &roles.accent, AA));
        for (text, background, floor) in pairs {
            let ratio = contrast(text, background);
            assert!(ratio >= floor, "{title}: {text} on {background}: {ratio}");
        }
    }

    #[test]
    fn ships_25_themes_with_unique_ids_and_titles() {
        assert_eq!(THEME_SEEDS.len(), 25);
        let ids: HashSet<_> = THEME_SEEDS.iter().map(|seed| seed.id).collect();
        let titles: HashSet<_> = THEME_SEEDS.iter().map(|seed| seed.title).collect();
        assert_eq!((ids.len(), titles.len()), (25, 25));
    }

    #[test]
    fn every_theme_passes_aa() {
        for option in theme_options() {
            expect_legible(&option.title, &option.tokens.roles);
        }
    }

    #[test]
    fn every_theme_steps_its_surfaces_by_color() {
        for option in theme_options() {
            let roles = &option.tokens.roles;
            let surfaces = [&roles.canvas, &roles.raised, &roles.overlay, &roles.fill];
            for (index, a) in surfaces.iter().enumerate() {
                for b in &surfaces[index + 1..] {
                    assert!(
                        oklab_distance(a, b) >= 0.02,
                        "{}: {a} and {b}",
                        option.title
                    );
                }
            }
        }
    }

    #[test]
    fn every_theme_ranks_its_text_by_color() {
        for option in theme_options() {
            let roles = &option.tokens.roles;
            let on = |text: &str| contrast(text, &roles.canvas);
            assert!(on(&roles.ink) > on(&roles.muted), "{}", option.title);
            assert!(on(&roles.muted) > on(&roles.faint), "{}", option.title);
            let [_, heading_chroma, _] = oklch_of(&roles.heading);
            let [_, ink_chroma, _] = oklch_of(&roles.ink);
            assert!(heading_chroma >= ink_chroma + 0.04, "{}", option.title);
        }
    }

    #[test]
    fn every_theme_has_a_red_danger_and_a_green_success() {
        for option in theme_options() {
            let roles = &option.tokens.roles;
            let [_, danger_chroma, danger_hue] = oklch_of(&roles.danger);
            let [_, success_chroma, success_hue] = oklch_of(&roles.success);
            assert!(
                hue_gap(danger_hue, 22.0) <= 25.0,
                "{} danger {danger_hue}",
                option.title
            );
            assert!(
                (115.0..=175.0).contains(&success_hue),
                "{} success {success_hue}",
                option.title
            );
            assert!(
                danger_chroma >= 0.1 && success_chroma >= 0.1,
                "{}",
                option.title
            );
        }
    }

    #[test]
    fn every_theme_keeps_the_ai_color_apart_from_danger_and_success() {
        for option in theme_options() {
            let roles = &option.tokens.roles;
            for other in [&roles.danger, &roles.success, &roles.ink, &roles.accent] {
                assert!(
                    oklab_distance(&roles.ai, other) >= 0.08,
                    "{}: ai {} vs {other}",
                    option.title,
                    roles.ai
                );
            }
        }
    }

    #[test]
    fn every_theme_declares_its_mode() {
        for option in theme_options() {
            let roles = &option.tokens.roles;
            let dark_canvas = contrast(&roles.canvas, "#000000") < contrast(&roles.ink, "#000000");
            let expected = if dark_canvas {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            };
            assert_eq!(option.tokens.mode, expected, "{}", option.title);
        }
    }

    #[test]
    fn credits_the_book_combination_each_theme_started_from() {
        assert_eq!(
            theme_by_id("mood-indigo").unwrap().origin,
            "After Wada #265 + #211"
        );
        assert_eq!(desafinado_theme().origin, "The original theme");
    }

    #[test]
    fn the_default_theme_is_mood_indigo() {
        assert_eq!(default_theme().title, "Mood Indigo");
        assert_eq!(default_theme().tokens.mode, ThemeMode::Dark);
    }

    #[test]
    fn lists_the_desafinado_theme_first_then_every_palette() {
        assert_eq!(theme_options().len(), 25);
        assert_eq!(theme_options()[0].id, "desafinado");
    }

    #[test]
    fn splits_the_options_by_mode_with_no_option_lost() {
        let dark = themes_of_mode(ThemeMode::Dark);
        let light = themes_of_mode(ThemeMode::Light);
        assert!(dark.iter().all(|o| o.tokens.mode == ThemeMode::Dark));
        assert!(light.iter().all(|o| o.tokens.mode == ThemeMode::Light));
        assert_eq!(dark.len() + light.len(), theme_options().len());
    }

    #[test]
    fn shows_the_colors_that_carry_the_hierarchy() {
        let theme = desafinado_theme();
        let codes: Vec<&str> = theme.colors.iter().map(|c| c.hex.as_str()).collect();
        let roles = &theme.tokens.roles;
        for role in [
            &roles.canvas,
            &roles.heading,
            &roles.ai,
            &roles.danger,
            &roles.success,
        ] {
            assert!(codes.contains(&role.as_str()));
        }
    }

    #[test]
    fn offers_omarchy_with_every_code_it_paints() {
        let option = omarchy_option(
            "omarchy",
            ThemeTokens {
                name: "tokyo-night".into(),
                mode: ThemeMode::Dark,
                roles: desafinado_theme().tokens.roles.clone(),
            },
        );
        assert_eq!(option.origin, "Follows Tokyo Night");
        assert_eq!(option.colors, desafinado_theme().colors);
    }

    #[test]
    fn finds_an_option_by_id_and_answers_none_otherwise() {
        assert_eq!(theme_by_id("desafinado"), Some(desafinado_theme()));
        assert_eq!(theme_by_id("neon"), None);
    }
}
