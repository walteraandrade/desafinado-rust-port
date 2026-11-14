use gpui_kit::{
    App, Global, Hsla, Rgba,
    component::{Theme, highlighter::HighlightTheme},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roles {
    pub canvas: Hsla,
    pub raised: Hsla,
    pub ink: Hsla,
    pub faint: Hsla,
    pub accent: Hsla,
    pub ai: Hsla,
    pub danger: Hsla,
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

pub fn mood_indigo() -> Roles {
    Roles {
        canvas: hex("#051230"),
        raised: hex("#1e0e3f"),
        ink: hex("#ffdd00"),
        faint: hex("#a6a159"),
        accent: hex("#a6a159"),
        ai: hex("#f68c50"),
        danger: hex("#d46d7a"),
        selection: hex("#1e0e3f"),
        fill: hex("#1e0e3f"),
        fill_hover: hex("#051230"),
        hairline: hex("#a6a159"),
        scrollbar: hex("#a6a159"),
        dark: true,
    }
}

pub fn roles(cx: &App) -> Roles {
    *cx.global::<Roles>()
}

pub fn apply(roles: Roles, cx: &mut App) {
    cx.set_global(roles);
    Theme::update(cx, |theme| {
        let c = &mut theme.colors;
        c.background = roles.canvas;
        c.foreground = roles.ink;
        c.muted = roles.fill;
        c.muted_foreground = roles.faint;
        c.border = roles.hairline;
        c.input = roles.hairline;
        c.ring = roles.accent;
        c.caret = roles.ink;
        c.selection = roles.selection;
        c.popover = roles.raised;
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
        c.link = roles.accent;
        c.link_hover = roles.accent;
        c.link_active = roles.accent;
        c.scrollbar = roles.canvas;
        c.scrollbar_thumb = roles.scrollbar;
        c.scrollbar_thumb_hover = roles.ink;
        c.title_bar = roles.raised;
        c.title_bar_border = roles.hairline;
        c.list = roles.raised;
        c.list_hover = roles.canvas;
        c.list_active = roles.canvas;
        c.list_active_border = roles.accent;
        c.sidebar = roles.raised;
        c.sidebar_foreground = roles.ink;
        c.sidebar_border = roles.hairline;
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
    syntax.insert("title".into(), weighted(&ink, 700));
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
