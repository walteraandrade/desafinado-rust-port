use std::sync::OnceLock;

use super::book_palettes::{BOOK_PALETTES, Palette};
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

fn roles_of(palette: &Palette) -> ThemeRoles {
    let code = |name: &str| -> String {
        palette
            .colors
            .iter()
            .find(|color| color.name == name)
            .unwrap_or_else(|| panic!("No color named {name}"))
            .hex
            .to_string()
    };
    let jobs = &palette.jobs;
    let accent = jobs.accent.unwrap_or(jobs.ink);
    let hairline = jobs
        .hairline
        .or(jobs.scrollbar)
        .or(jobs.accent)
        .or(jobs.fill)
        .unwrap_or(jobs.ink);
    ThemeRoles {
        canvas: code(jobs.canvas),
        raised: code(jobs.raised),
        ink: code(jobs.ink),
        faint: code(accent),
        accent: code(accent),
        ai: code(jobs.ai),
        danger: code(jobs.danger),
        selection: code(jobs.selection.unwrap_or(jobs.raised)),
        fill: code(jobs.fill.unwrap_or(jobs.raised)),
        fill_hover: code(if jobs.fill.is_some() {
            jobs.raised
        } else {
            jobs.canvas
        }),
        hairline: code(hairline),
        scrollbar: code(jobs.scrollbar.unwrap_or(hairline)),
    }
}

fn roles_as_colors(roles: &ThemeRoles) -> Vec<ThemeColor> {
    let entries = [
        ("canvas", &roles.canvas),
        ("raised", &roles.raised),
        ("ink", &roles.ink),
        ("faint", &roles.faint),
        ("accent", &roles.accent),
        ("ai", &roles.ai),
        ("danger", &roles.danger),
        ("selection", &roles.selection),
        ("fill", &roles.fill),
        ("fillHover", &roles.fill_hover),
        ("hairline", &roles.hairline),
        ("scrollbar", &roles.scrollbar),
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

fn option_of(palette: &Palette) -> ThemeOption {
    ThemeOption {
        id: palette.id.to_string(),
        title: palette.title.to_string(),
        origin: format!("#{} + #{}", palette.source.0, palette.source.1),
        colors: palette
            .colors
            .iter()
            .map(|color| ThemeColor {
                name: color.name.to_string(),
                hex: color.hex.to_string(),
            })
            .collect(),
        tokens: ThemeTokens {
            name: palette.id.to_string(),
            mode: palette.mode,
            roles: roles_of(palette),
        },
    }
}

fn desafinado_roles() -> ThemeRoles {
    let s = |value: &str| value.to_string();
    ThemeRoles {
        canvas: s("#141417"),
        raised: s("#28282d"),
        ink: s("#f1f1f3"),
        faint: s("#a6a6ab"),
        accent: s("#e37545"),
        ai: s("#e37545"),
        danger: s("#f87171"),
        selection: s("#3a241b"),
        fill: s("#222226"),
        fill_hover: s("#303035"),
        hairline: s("#333338"),
        scrollbar: s("#46464b"),
    }
}

pub fn desafinado_theme() -> &'static ThemeOption {
    static THEME: OnceLock<ThemeOption> = OnceLock::new();
    THEME.get_or_init(|| {
        let roles = desafinado_roles();
        ThemeOption {
            id: "desafinado".into(),
            title: "Desafinado".into(),
            origin: "The original theme".into(),
            colors: roles_as_colors(&roles),
            tokens: ThemeTokens {
                name: "desafinado".into(),
                mode: ThemeMode::Dark,
                roles,
            },
        }
    })
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
    OPTIONS.get_or_init(|| {
        std::iter::once(desafinado_theme().clone())
            .chain(BOOK_PALETTES.iter().map(option_of))
            .collect()
    })
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
    use std::collections::{HashMap, HashSet};

    use serde::Deserialize;

    use super::*;
    use crate::theme::color::{contrast, rgb_of};

    const AA: f64 = 4.5;

    #[derive(Deserialize)]
    struct Wada {
        name: String,
        combinations: Vec<u32>,
        lab: Vec<f64>,
        hex: String,
    }

    fn wada() -> Vec<Wada> {
        serde_json::from_str(include_str!("../../tests/data/wada-colors.json"))
            .expect("wada colors")
    }

    fn combination(colors: &[Wada], id: u32) -> Vec<String> {
        colors
            .iter()
            .filter(|color| color.combinations.contains(&id))
            .map(|color| color.name.clone())
            .collect()
    }

    fn lab_of<'a>(book: &'a HashMap<&str, &Wada>, name: &str) -> &'a [f64] {
        &book
            .get(name)
            .unwrap_or_else(|| panic!("Not in the book: {name}"))
            .lab
    }

    fn lab_distance(book: &HashMap<&str, &Wada>, a: &str, b: &str) -> f64 {
        lab_of(book, a)
            .iter()
            .zip(lab_of(book, b))
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f64>()
            .sqrt()
    }

    fn chroma(book: &HashMap<&str, &Wada>, name: &str) -> f64 {
        let lab = lab_of(book, name);
        lab[1].hypot(lab[2])
    }

    fn is_reddish(hex: &str) -> bool {
        let [r, g, b] = rgb_of(hex).map(|value| f64::from(value) / 255.0);
        let max = r.max(g).max(b);
        let span = max - r.min(g).min(b);
        let saturation = if max == 0.0 { 0.0 } else { span / max };
        let hue = if max == r && span > 0.0 {
            (((g - b) / span) * 60.0 + 360.0) % 360.0
        } else {
            -1.0
        };
        hue >= 0.0 && (hue <= 18.0 || hue >= 340.0) && saturation >= 0.45
    }

    fn expect_legible(roles: &ThemeRoles) {
        let pairs = [
            (&roles.ink, &roles.canvas),
            (&roles.ink, &roles.raised),
            (&roles.ink, &roles.fill),
            (&roles.ink, &roles.fill_hover),
            (&roles.ink, &roles.selection),
            (&roles.ai, &roles.canvas),
            (&roles.ai, &roles.raised),
            (&roles.ai, &roles.selection),
            (&roles.canvas, &roles.ai),
            (&roles.accent, &roles.canvas),
            (&roles.accent, &roles.raised),
            (&roles.faint, &roles.canvas),
            (&roles.faint, &roles.raised),
            (&roles.danger, &roles.canvas),
            (&roles.danger, &roles.raised),
        ];
        for (text, background) in pairs {
            let ratio = contrast(text, background);
            assert!(ratio >= AA, "{text} on {background}: {ratio}");
        }
    }

    #[test]
    fn ships_24_palettes_with_unique_ids_and_titles() {
        assert_eq!(BOOK_PALETTES.len(), 24);
        let ids: HashSet<_> = BOOK_PALETTES.iter().map(|p| p.id).collect();
        let titles: HashSet<_> = BOOK_PALETTES.iter().map(|p| p.title).collect();
        assert_eq!((ids.len(), titles.len()), (24, 24));
    }

    #[test]
    fn every_palette_uses_only_exact_book_codes() {
        let colors = wada();
        let book: HashMap<&str, &Wada> = colors.iter().map(|c| (c.name.as_str(), c)).collect();
        for palette in BOOK_PALETTES {
            for color in palette.colors {
                assert_eq!(
                    book.get(color.name).map(|c| c.hex.as_str()),
                    Some(color.hex),
                    "{}",
                    palette.title
                );
            }
        }
    }

    #[test]
    fn every_palette_joins_two_combinations_that_share_a_color() {
        let colors = wada();
        for palette in BOOK_PALETTES {
            let a = combination(&colors, palette.source.0);
            let b = combination(&colors, palette.source.1);
            assert!(a.iter().any(|name| b.contains(name)), "{}", palette.title);
            let mut names: Vec<_> = palette.colors.iter().map(|c| c.name.to_string()).collect();
            names.sort();
            let mut union: Vec<_> = a
                .iter()
                .chain(&b)
                .cloned()
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            union.sort();
            assert_eq!(names, union, "{}", palette.title);
        }
    }

    #[test]
    fn every_palette_has_at_least_six_colors() {
        for palette in BOOK_PALETTES {
            assert!(palette.colors.len() >= 6, "{}", palette.title);
        }
    }

    #[test]
    fn every_palette_gives_every_color_a_job() {
        for palette in BOOK_PALETTES {
            let jobs = &palette.jobs;
            let assigned: HashSet<&str> = [
                Some(jobs.canvas),
                Some(jobs.raised),
                Some(jobs.ink),
                Some(jobs.ai),
                Some(jobs.danger),
                jobs.accent,
                jobs.selection,
                jobs.fill,
                jobs.hairline,
                jobs.scrollbar,
            ]
            .into_iter()
            .flatten()
            .collect();
            let names: HashSet<&str> = palette.colors.iter().map(|c| c.name).collect();
            assert_eq!(assigned, names, "{}", palette.title);
        }
    }

    #[test]
    fn every_palette_keeps_a_real_ai_hue_and_a_red() {
        let colors = wada();
        let book: HashMap<&str, &Wada> = colors.iter().map(|c| (c.name.as_str(), c)).collect();
        for palette in BOOK_PALETTES {
            let jobs = &palette.jobs;
            assert!(chroma(&book, jobs.ai) >= 20.0, "{}", palette.title);
            assert!(
                lab_distance(&book, jobs.ai, jobs.ink) >= 25.0,
                "{}",
                palette.title
            );
            assert!(
                lab_distance(&book, jobs.ai, jobs.danger) >= 25.0,
                "{}",
                palette.title
            );
            assert!(is_reddish(&book[jobs.danger].hex), "{}", palette.title);
        }
    }

    #[test]
    fn every_palette_paints_only_its_own_colors() {
        for palette in BOOK_PALETTES {
            let codes: Vec<&str> = palette.colors.iter().map(|c| c.hex).collect();
            let roles = &theme_by_id(palette.id).unwrap().tokens.roles;
            for value in roles_as_colors(roles) {
                assert!(codes.contains(&value.hex.as_str()), "{}", palette.title);
            }
        }
    }

    #[test]
    fn every_palette_passes_aa() {
        for palette in BOOK_PALETTES {
            expect_legible(&theme_by_id(palette.id).expect("option").tokens.roles);
        }
    }

    #[test]
    fn every_palette_declares_its_mode() {
        for palette in BOOK_PALETTES {
            let roles = &theme_by_id(palette.id).unwrap().tokens.roles;
            let dark_canvas = contrast(&roles.canvas, "#000000") < contrast(&roles.ink, "#000000");
            let expected = if dark_canvas {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            };
            assert_eq!(palette.mode, expected, "{}", palette.title);
        }
    }

    #[test]
    fn the_desafinado_theme_passes_aa() {
        expect_legible(&desafinado_theme().tokens.roles);
    }

    #[test]
    fn the_default_theme_is_mood_indigo() {
        assert_eq!(default_theme().title, "Mood Indigo");
        assert_eq!(default_theme().tokens.mode, ThemeMode::Dark);
    }

    #[test]
    fn lists_the_desafinado_theme_first_then_every_palette() {
        assert_eq!(theme_options().len(), 25);
        assert_eq!(&theme_options()[0], desafinado_theme());
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
    fn shows_every_code_the_desafinado_theme_paints() {
        let theme = desafinado_theme();
        let codes: Vec<&str> = theme.colors.iter().map(|c| c.hex.as_str()).collect();
        for color in roles_as_colors(&theme.tokens.roles) {
            assert!(codes.contains(&color.hex.as_str()));
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
        let hexes: Vec<_> = option.colors.iter().map(|c| c.hex.clone()).collect();
        let expected: Vec<_> = desafinado_theme()
            .colors
            .iter()
            .map(|c| c.hex.clone())
            .collect();
        assert_eq!(hexes, expected);
    }

    #[test]
    fn finds_an_option_by_id_and_answers_none_otherwise() {
        assert_eq!(theme_by_id("desafinado"), Some(desafinado_theme()));
        assert_eq!(theme_by_id("neon"), None);
    }
}
