use desafinado_core::blocks::body_after_frontmatter;
use gpui_kit::{
    base::{TextView, TextViewStyle},
    *,
};

use crate::{
    theme::Roles,
    ui::{TEXT_DOCUMENT, UI_FAMILY},
    workspace::column_row,
};

const HEADING_SIZES: [f32; 6] = [30., 24., 20., 18., 17., 17.];

fn heading_size(level: u8) -> Pixels {
    px(HEADING_SIZES[usize::from(level.clamp(1, 6)) - 1])
}

fn reading_style(roles: Roles) -> TextViewStyle {
    let heading_color = roles.heading;
    TextViewStyle::default()
        .with_dark(roles.dark)
        .with_foreground(roles.ink)
        .with_muted_foreground(roles.muted)
        .with_link(roles.accent)
        .with_selection(roles.selection)
        .with_code_background(roles.raised)
        .with_border(roles.hairline)
        .with_paragraph_gap(rems(1.))
        .with_inline_code(HighlightStyle {
            color: Some(roles.ink),
            background_color: Some(roles.raised),
            ..Default::default()
        })
        .with_heading(move |level| {
            let mut style = StyleRefinement::default();
            style.text.font_size = Some(heading_size(level).into());
            style.text.font_weight = Some(FontWeight::BOLD);
            style.text.color = Some(heading_color);
            style
        })
}

pub fn render_reading(text: &str, roles: Roles) -> AnyElement {
    div()
        .id("reading-view")
        .size_full()
        .overflow_y_scroll()
        .pt(px(16.))
        .pb(px(48.))
        .child(column_row(
            div()
                .font_family(UI_FAMILY)
                .text_size(TEXT_DOCUMENT)
                .line_height(relative(1.6))
                .text_color(roles.ink)
                .child(
                    TextView::markdown("reading-text", body_after_frontmatter(text).to_string())
                        .style(reading_style(roles))
                        .selectable(true),
                ),
        ))
        .into_any_element()
}
