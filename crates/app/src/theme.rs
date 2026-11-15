use std::path::{Path, PathBuf};

use desafinado_features::theme::{
    catalog::{ThemeOption, omarchy_option},
    omarchy_source::{default_omarchy_state_dir, read_omarchy_theme},
    preference::{
        KeyValueStorage, OMARCHY_CHOICE, THEME_STORAGE_KEY, active_theme, load_theme_choice,
        save_theme_choice, selected_option,
    },
    tokens::{ThemeMode, ThemeTokens},
};
use desafinado_platform::files::write_atomic_unconditionally;
use gpui_kit::{
    App, Global, Hsla, Rgba,
    component::{Theme, ThemeMode as ThemeModeKind, highlighter::HighlightTheme},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roles {
    pub canvas: Hsla,
    pub raised: Hsla,
    pub overlay: Hsla,
    pub heading: Hsla,
    pub ink: Hsla,
    pub muted: Hsla,
    pub faint: Hsla,
    pub accent: Hsla,
    pub ai: Hsla,
    pub ai_fill: Hsla,
    pub danger: Hsla,
    pub danger_fill: Hsla,
    pub success: Hsla,
    pub success_fill: Hsla,
    pub selection: Hsla,
    pub fill: Hsla,
    pub fill_hover: Hsla,
    pub hairline: Hsla,
    pub scrollbar: Hsla,
    pub dark: bool,
}

impl Global for Roles {}

pub fn hex(code: &str) -> Hsla {
    let value = u32::from_str_radix(code.trim_start_matches('#'), 16).unwrap_or(0);
    Rgba {
        r: ((value >> 16) & 0xff) as f32 / 255.,
        g: ((value >> 8) & 0xff) as f32 / 255.,
        b: (value & 0xff) as f32 / 255.,
        a: 1.,
    }
    .into()
}

pub fn roles_of(tokens: &ThemeTokens) -> Roles {
    let r = &tokens.roles;
    Roles {
        canvas: hex(&r.canvas),
        raised: hex(&r.raised),
        overlay: hex(&r.overlay),
        heading: hex(&r.heading),
        ink: hex(&r.ink),
        muted: hex(&r.muted),
        faint: hex(&r.faint),
        accent: hex(&r.accent),
        ai: hex(&r.ai),
        ai_fill: hex(&r.ai_fill),
        danger: hex(&r.danger),
        danger_fill: hex(&r.danger_fill),
        success: hex(&r.success),
        success_fill: hex(&r.success_fill),
        selection: hex(&r.selection),
        fill: hex(&r.fill),
        fill_hover: hex(&r.fill_hover),
        hairline: hex(&r.hairline),
        scrollbar: hex(&r.scrollbar),
        dark: tokens.mode == ThemeMode::Dark,
    }
}

pub const THEME_FILE_NAME: &str = "theme.txt";

struct ThemeFile(PathBuf);

impl KeyValueStorage for ThemeFile {
    fn get_item(&self, key: &str) -> Result<Option<String>, String> {
        if key != THEME_STORAGE_KEY {
            return Ok(None);
        }
        match std::fs::read_to_string(&self.0) {
            Ok(text) => Ok(Some(text.trim().to_string())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }

    fn set_item(&self, key: &str, value: &str) -> Result<(), String> {
        if key != THEME_STORAGE_KEY {
            return Ok(());
        }
        write_atomic_unconditionally(&self.0, value).map_err(|error| error.to_string())
    }
}

#[derive(Clone, Debug)]
pub struct ThemeState {
    pub choice: String,
    pub omarchy: Option<ThemeTokens>,
    file: PathBuf,
}

impl Global for ThemeState {}

impl ThemeState {
    pub fn load(user_data: &Path) -> Self {
        let file = user_data.join(THEME_FILE_NAME);
        let omarchy = std::env::var_os("HOME")
            .map(PathBuf::from)
            .and_then(|home| read_omarchy_theme(&default_omarchy_state_dir(&home)));
        Self {
            choice: load_theme_choice(Some(&ThemeFile(file.clone()))),
            omarchy,
            file,
        }
    }

    pub fn tokens(&self) -> &ThemeTokens {
        active_theme(&self.choice, self.omarchy.as_ref())
    }

    pub fn selected(&self) -> String {
        selected_option(&self.choice, self.omarchy.as_ref())
    }

    pub fn omarchy_option(&self) -> Option<ThemeOption> {
        self.omarchy
            .clone()
            .map(|tokens| omarchy_option(OMARCHY_CHOICE, tokens))
    }
}

pub fn init(user_data: &Path, cx: &mut App) {
    let state = ThemeState::load(user_data);
    apply(roles_of(state.tokens()), cx);
    cx.set_global(state);
}

pub fn apply_default(cx: &mut App) {
    apply(
        roles_of(&desafinado_features::theme::catalog::default_theme().tokens),
        cx,
    );
}

pub fn state(cx: &App) -> &ThemeState {
    cx.global::<ThemeState>()
}

pub fn choose(choice: &str, cx: &mut App) {
    let mut state = cx.global::<ThemeState>().clone();
    save_theme_choice(Some(&ThemeFile(state.file.clone())), choice);
    state.choice = choice.to_string();
    apply(roles_of(state.tokens()), cx);
    cx.set_global(state);
    cx.refresh_windows();
}

pub fn roles(cx: &App) -> Roles {
    *cx.global::<Roles>()
}

pub fn apply(roles: Roles, cx: &mut App) {
    cx.set_global(roles);
    Theme::change(
        if roles.dark {
            ThemeModeKind::Dark
        } else {
            ThemeModeKind::Light
        },
        None,
        cx,
    );
    Theme::update(cx, |theme| {
        let c = &mut theme.colors;
        c.background = roles.canvas;
        c.foreground = roles.ink;
        c.muted = roles.fill;
        c.muted_foreground = roles.muted;
        c.border = roles.hairline;
        c.input = roles.hairline;
        c.ring = roles.accent;
        c.caret = roles.ink;
        c.selection = roles.selection;
        c.popover = roles.overlay;
        c.popover_foreground = roles.ink;
        c.accent = roles.fill_hover;
        c.accent_foreground = roles.ink;
        c.primary = roles.ai;
        c.primary_foreground = roles.canvas;
        c.primary_hover = roles.ai;
        c.primary_active = roles.ai;
        c.secondary = roles.fill;
        c.secondary_foreground = roles.ink;
        c.secondary_hover = roles.fill_hover;
        c.secondary_active = roles.fill_hover;
        c.danger = roles.danger;
        c.danger_foreground = roles.canvas;
        c.success = roles.success;
        c.success_foreground = roles.canvas;
        c.link = roles.accent;
        c.link_hover = roles.accent;
        c.link_active = roles.accent;
        c.scrollbar = roles.canvas;
        c.scrollbar_thumb = roles.scrollbar;
        c.scrollbar_thumb_hover = roles.ink;
        c.title_bar = roles.raised;
        c.title_bar_border = roles.raised;
        c.list = roles.raised;
        c.list_hover = roles.canvas;
        c.list_active = roles.canvas;
        c.list_active_border = roles.accent;
        c.sidebar = roles.raised;
        c.sidebar_foreground = roles.ink;
        c.sidebar_border = roles.raised;
        c.window_border = roles.hairline;
        c.overlay = roles.canvas;
        c.button = roles.fill;
        c.button_foreground = roles.ink;
        c.button_hover = roles.fill_hover;
        c.button_active = roles.fill_hover;
        c.button_primary = roles.ai;
        c.button_primary_foreground = roles.canvas;
        c.button_primary_hover = roles.ai;
        c.button_primary_active = roles.ai;
        theme.font_family = "Inter".into();
        theme.mono_font_family = "Inter".into();
        theme.mono_font_size = gpui_kit::px(17.);
        theme.highlight_theme = std::sync::Arc::new(highlight_theme(&roles));
        theme.radius = gpui_kit::px(0.);
    });
}

fn css(color: Hsla) -> String {
    let rgba = Rgba::from(color);
    format!(
        "#{:02x}{:02x}{:02x}",
        (rgba.r * 255.).round() as u8,
        (rgba.g * 255.).round() as u8,
        (rgba.b * 255.).round() as u8
    )
}

fn highlight_theme(roles: &Roles) -> HighlightTheme {
    let ink = css(roles.ink);
    let faint = css(roles.faint);
    let accent = css(roles.accent);
    let heading = css(roles.heading);
    let muted = css(roles.muted);
    let canvas = css(roles.canvas);
    let plain = |color: &str| serde_json::json!({ "color": color });
    let weighted =
        |color: &str, weight: u16| serde_json::json!({ "color": color, "font_weight": weight });
    let mut syntax = serde_json::Map::new();
    for key in [
        "attribute",
        "boolean",
        "comment",
        "comment_doc",
        "constant",
        "constructor",
        "embedded",
        "enum",
        "function",
        "hint",
        "keyword",
        "label",
        "number",
        "operator",
        "predictive",
        "preproc",
        "primary",
        "property",
        "string",
        "string.escape",
        "string.regex",
        "string.special",
        "string.special.symbol",
        "tag",
        "tag.doctype",
        "text.code.span",
        "text.literal",
        "type",
        "variable",
        "variable.special",
        "variant",
    ] {
        syntax.insert(key.into(), plain(&ink));
    }
    for key in [
        "punctuation",
        "punctuation.bracket",
        "punctuation.delimiter",
        "punctuation.special",
        "punctuation.list_marker",
    ] {
        syntax.insert(key.into(), plain(&faint));
    }
    for key in ["text.literal", "text.code.span", "string", "embedded"] {
        syntax.insert(key.into(), plain(&muted));
    }
    syntax.insert("title".into(), weighted(&heading, 700));
    syntax.insert(
        "emphasis".into(),
        serde_json::json!({ "color": ink, "font_style": "italic" }),
    );
    syntax.insert("emphasis.strong".into(), weighted(&ink, 700));
    syntax.insert("link_text".into(), plain(&accent));
    syntax.insert("link_uri".into(), plain(&accent));
    let value = serde_json::json!({
        "name": "desafinado",
        "appearance": if roles.dark { "dark" } else { "light" },
        "style": {
            "editor.background": canvas,
            "editor.foreground": ink,
            "editor.active_line.background": canvas,
            "editor.line_number": faint,
            "editor.active_line_number": ink,
            "editor.gutter.background": canvas,
            "syntax": syntax,
        }
    });
    serde_json::from_value(value).expect("highlight theme")
}

#[cfg(test)]
mod tests {
    use super::*;
    use desafinado_features::theme::catalog::{default_theme, theme_by_id};

    #[test]
    fn maps_a_light_palette_to_light_roles() {
        let light = desafinado_features::theme::catalog::themes_of_mode(ThemeMode::Light)[0];
        let roles = roles_of(&light.tokens);
        assert!(!roles.dark);
        assert_eq!(roles.canvas, hex(&light.tokens.roles.canvas));
        assert_eq!(roles.ai, hex(&light.tokens.roles.ai));
    }

    #[test]
    fn maps_the_default_theme_to_mood_indigo() {
        let tokens = &default_theme().tokens;
        let roles = roles_of(tokens);
        assert!(roles.dark);
        assert_eq!(tokens.name, "mood-indigo");
        assert_eq!(roles.canvas, hex(&tokens.roles.canvas));
        assert_eq!(roles.heading, hex(&tokens.roles.heading));
        assert_eq!(roles.success, hex(&tokens.roles.success));
    }

    #[test]
    fn keeps_the_choice_in_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = ThemeFile(dir.path().join(THEME_FILE_NAME));
        assert_eq!(load_theme_choice(Some(&file)), OMARCHY_CHOICE);
        let id = &theme_by_id("desafinado").unwrap().id;
        save_theme_choice(Some(&file), id);
        assert_eq!(load_theme_choice(Some(&file)), "desafinado");
    }
}
