use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use desafinado_features::theme::{
    catalog::{ThemeOption, omarchy_option},
    color::mix_hex,
    omarchy_source::{default_omarchy_state_dir, read_omarchy_theme},
    preference::{
        KeyValueStorage, OMARCHY_CHOICE, THEME_STORAGE_KEY, active_theme, load_theme_choice,
        save_theme_choice, selected_option,
    },
    tokens::{ThemeMode, ThemeTokens},
};
use desafinado_platform::files::write_atomic_unconditionally;
use gpui_kit::{
    App, Global, HighlightStyle, Hsla, Rgba,
    base::{input::Rope, text::CodeBlock},
    component::{
        Theme, ThemeMode as ThemeModeKind,
        highlighter::{HighlightTheme, SyntaxHighlighter},
    },
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
    pub accent_hover: Hsla,
    pub ai: Hsla,
    pub ai_hover: Hsla,
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

const HOVER_SHARE: f64 = 0.2;

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
        accent_hover: hex(&mix_hex(&r.accent, &r.ink, HOVER_SHARE)),
        ai: hex(&r.ai),
        ai_hover: hex(&mix_hex(&r.ai, &r.ink, HOVER_SHARE)),
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

pub fn swatches(option: &ThemeOption) -> Vec<Hsla> {
    option.colors.iter().map(|color| hex(&color.hex)).collect()
}

pub fn roles(cx: &App) -> Roles {
    *cx.global::<Roles>()
}

pub type CodeHighlighter =
    dyn Fn(&CodeBlock) -> Vec<(std::ops::Range<usize>, HighlightStyle)> + Send + Sync;

pub struct ReadingCode(pub Arc<CodeHighlighter>);

impl Global for ReadingCode {}

pub fn reading_code(cx: &App) -> Arc<CodeHighlighter> {
    Arc::clone(&cx.global::<ReadingCode>().0)
}

fn code_highlighter(theme: HighlightTheme) -> Arc<CodeHighlighter> {
    Arc::new(move |block: &CodeBlock| {
        thread_local! {
            static HIGHLIGHTERS: std::cell::RefCell<std::collections::HashMap<String, SyntaxHighlighter>> =
                std::cell::RefCell::default();
        }
        let Some(lang) = block.lang() else {
            return Vec::new();
        };
        let lang = lang.to_lowercase();
        let code = block.code();
        HIGHLIGHTERS.with(|cache| {
            let mut cache = cache.borrow_mut();
            let highlighter = cache
                .entry(lang.clone())
                .or_insert_with(|| SyntaxHighlighter::new(&lang));
            highlighter.update(None, &Rope::from_str(code.as_ref()), None);
            highlighter.styles(&(0..code.len()), &theme)
        })
    })
}

pub fn apply(roles: Roles, cx: &mut App) {
    cx.set_global(roles);
    cx.set_global(ReadingCode(code_highlighter(code_theme(&roles))));
    Theme::change(
        if roles.dark {
            ThemeModeKind::Dark
        } else {
            ThemeModeKind::Light
        },
        None,
        cx,
    );
    let caret = crate::caret::settings(cx);
    Theme::update(cx, |theme| {
        theme.scrollbar_mode = gpui_kit::component::scroll::ScrollbarMode::Always;
        crate::caret::apply_to_theme(theme, caret);
        let c = &mut theme.colors;
        c.background = roles.canvas;
        c.foreground = roles.ink;
        c.muted = roles.fill;
        c.muted_foreground = roles.muted;
        c.border = roles.hairline;
        c.input = roles.hairline;
        c.ring = roles.accent;
        c.caret = roles.accent;
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
        theme.mono_font_size = crate::ui::TEXT_DOCUMENT;
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

/// Syntax colors for code read in the Reading view. The editor's theme mutes
/// code on purpose; here code is content, so every token stays readable.
fn code_theme(roles: &Roles) -> HighlightTheme {
    let color = |hsla: Hsla| serde_json::json!({ "color": css(hsla) });
    let mut syntax = serde_json::Map::new();
    let groups: [(&[&str], Hsla); 5] = [
        (
            &[
                "punctuation",
                "punctuation.bracket",
                "punctuation.delimiter",
                "punctuation.special",
                "operator",
            ],
            roles.muted,
        ),
        (
            &[
                "string",
                "string.escape",
                "string.regex",
                "string.special",
                "string.special.symbol",
            ],
            roles.success,
        ),
        (
            &["number", "boolean", "constant", "constant.builtin"],
            roles.ai,
        ),
        (
            &[
                "keyword",
                "type",
                "function",
                "tag",
                "attribute",
                "constructor",
            ],
            roles.accent,
        ),
        (
            &[
                "property",
                "variable",
                "variable.special",
                "label",
                "variant",
                "enum",
            ],
            roles.ink,
        ),
    ];
    for (keys, hsla) in groups {
        for key in keys {
            syntax.insert((*key).into(), color(hsla));
        }
    }
    for key in ["link_text", "link_uri", "text.uri", "text.reference"] {
        syntax.insert(key.into(), color(roles.accent));
    }
    for key in ["text.literal", "text.code.span"] {
        syntax.insert(key.into(), color(roles.success));
    }
    let styled = |hsla: Hsla, extra: serde_json::Value| {
        let mut style = serde_json::json!({ "color": css(hsla) });
        style
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        style
    };
    syntax.insert(
        "title".into(),
        styled(roles.heading, serde_json::json!({ "font_weight": 700 })),
    );
    syntax.insert(
        "emphasis".into(),
        styled(roles.ink, serde_json::json!({ "font_style": "italic" })),
    );
    syntax.insert(
        "emphasis.strong".into(),
        styled(roles.ink, serde_json::json!({ "font_weight": 700 })),
    );
    for key in ["comment", "comment_doc"] {
        syntax.insert(
            key.into(),
            serde_json::json!({ "color": css(roles.muted), "font_style": "italic" }),
        );
    }
    let value = serde_json::json!({
        "name": "desafinado-reading-code",
        "appearance": if roles.dark { "dark" } else { "light" },
        "style": {
            "editor.background": css(roles.raised),
            "editor.foreground": css(roles.ink),
            "syntax": syntax,
        }
    });
    serde_json::from_value(value).expect("code highlight theme")
}

#[cfg(test)]
mod tests {
    use super::*;
    use desafinado_features::theme::catalog::{default_theme, theme_by_id};

    #[test]
    fn reads_code_tokens_in_distinct_role_colors() {
        use gpui_kit::base::input::HighlightStyleResolver as _;
        let roles = roles_of(&default_theme().tokens);
        let theme = code_theme(&roles);
        let color = |name: &str| theme.style(name).and_then(|style| style.color);
        assert_eq!(color("string"), Some(roles.success));
        assert_eq!(color("number"), Some(roles.ai));
        assert_eq!(color("keyword"), Some(roles.accent));
        assert_eq!(color("punctuation.bracket"), Some(roles.muted));
        assert_eq!(color("property"), Some(roles.ink));
        assert_eq!(color("title"), Some(roles.heading));
        assert_eq!(color("link_text"), Some(roles.accent));
    }

    #[test]
    fn colors_json_strings_whatever_the_fence_case() {
        let roles = roles_of(&default_theme().tokens);
        let highlight = code_highlighter(code_theme(&roles));
        for lang in ["json", "JSON"] {
            let block = CodeBlock::from_code(r#"{"key": "value", "n": 1}"#, Some(lang));
            let styles = highlight(&block);
            let string_color = styles
                .iter()
                .find(|(range, _)| &block.code()[range.clone()] == "\"value\"")
                .and_then(|(_, style)| style.color);
            assert_eq!(string_color, Some(roles.success), "{lang}");
        }
    }

    #[test]
    fn colors_each_block_from_its_own_text_when_a_language_repeats() {
        let roles = roles_of(&default_theme().tokens);
        let highlight = code_highlighter(code_theme(&roles));
        for code in [r#"{"a": "long string here", "b": 2}"#, r#"[1, "x"]"#] {
            let block = CodeBlock::from_code(code, Some("json"));
            for (range, style) in highlight(&block) {
                assert!(range.end <= code.len(), "{code}: {range:?}");
                let text = &code[range];
                if style.color == Some(roles.success) {
                    assert!(
                        text.starts_with('"') && text.ends_with('"'),
                        "{code}: {text}"
                    );
                }
            }
        }
    }

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

    const TEXT: f64 = 4.5;
    const MARK: f64 = 3.;

    /// Every text and mark color the shared components draw, on the
    /// surface they draw it on. The minimum is WCAG AA: 4.5 for text,
    /// 3 for the focus ring and data marks. Disabled text is exempt.
    fn used_pairs(
        r: &desafinado_features::theme::tokens::ThemeRoles,
    ) -> Vec<(&'static str, String, String, f64)> {
        let accent_hover = mix_hex(&r.accent, &r.ink, HOVER_SHARE);
        let ai_hover = mix_hex(&r.ai, &r.ink, HOVER_SHARE);
        let mut pairs = Vec::new();
        for (surface, background) in [
            ("canvas", &r.canvas),
            ("raised", &r.raised),
            ("overlay", &r.overlay),
            ("fill", &r.fill),
            ("fill_hover", &r.fill_hover),
        ] {
            for (name, text) in [
                ("ink", &r.ink),
                ("heading", &r.heading),
                ("muted", &r.muted),
            ] {
                pairs.push((name, text.clone(), format!("{surface}:{background}"), TEXT));
            }
            if surface != "fill_hover" {
                pairs.push((
                    "danger",
                    r.danger.clone(),
                    format!("{surface}:{background}"),
                    TEXT,
                ));
            }
            pairs.push((
                "accent ring",
                r.accent.clone(),
                format!("{surface}:{background}"),
                MARK,
            ));
        }
        for (name, text, surface, background) in [
            ("heading", &r.heading, "selection", &r.selection),
            ("ink", &r.ink, "selection", &r.selection),
            ("accent link", &r.accent, "canvas", &r.canvas),
            ("ai", &r.ai, "ai_fill", &r.ai_fill),
            ("ai", &r.ai, "canvas", &r.canvas),
            ("ai", &r.ai, "raised", &r.raised),
            ("ai", &r.ai, "overlay", &r.overlay),
            ("ai", &r.ai, "fill", &r.fill),
            ("ink", &r.ink, "ai_fill", &r.ai_fill),
            ("danger", &r.danger, "danger_fill", &r.danger_fill),
            ("ink", &r.ink, "danger_fill", &r.danger_fill),
            ("success", &r.success, "success_fill", &r.success_fill),
            ("canvas", &r.canvas, "accent", &r.accent),
            ("canvas", &r.canvas, "accent_hover", &accent_hover),
            ("canvas", &r.canvas, "ai", &r.ai),
            ("canvas", &r.canvas, "ai_hover", &ai_hover),
            ("canvas", &r.canvas, "danger", &r.danger),
        ] {
            pairs.push((name, text.clone(), format!("{surface}:{background}"), TEXT));
        }
        pairs
    }

    #[test]
    fn every_used_pair_meets_aa_in_every_theme() {
        use desafinado_features::theme::{catalog::theme_options, color::contrast};
        let failures: Vec<String> = theme_options()
            .iter()
            .flat_map(|option| {
                used_pairs(&option.tokens.roles)
                    .into_iter()
                    .filter_map(|(name, text, surface, minimum)| {
                        let background = surface.split(':').nth(1).unwrap_or("");
                        let ratio = contrast(&text, background);
                        (ratio < minimum).then(|| {
                            format!(
                                "{}: {name} on {surface} is {ratio:.2}, needs {minimum}",
                                option.id
                            )
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(
            failures.is_empty(),
            "C5 {} pair(s) below AA:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
