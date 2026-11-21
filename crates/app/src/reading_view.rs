use desafinado_core::blocks::body_after_frontmatter;
use gpui_kit::{
    base::TextView,
    component::{self, scroll::ScrollableElement as _},
    *,
};

use crate::{
    theme::Roles,
    ui::{Type as _, markdown_style},
    workspace::column_row,
};

pub fn render_reading(text: &str, roles: Roles) -> AnyElement {
    div()
        .id("reading-view")
        .size_full()
        .pt(px(16.))
        .pb(px(48.))
        .child(column_row(
            div()
                .type_ui()
                .type_document()
                .line_height(relative(1.75))
                .text_color(roles.ink)
                .child(
                    TextView::markdown("reading-text", body_after_frontmatter(text).to_string())
                        .style(markdown_style(roles))
                        .selectable(true)
                        .on_link_click(|_, _, _, _| {})
                        .link_tooltip(|url, window, cx| {
                            component::tooltip::Tooltip::new(url.clone()).build(window, cx)
                        }),
                ),
        ))
        .overflow_y_scrollbar()
        .into_any_element()
}
