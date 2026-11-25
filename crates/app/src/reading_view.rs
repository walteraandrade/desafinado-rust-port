use std::sync::Arc;

use desafinado_core::blocks::body_after_frontmatter;
use gpui_kit::{
    base::TextView,
    component::{self, scroll::ScrollableElement as _},
    *,
};

use crate::{
    theme::{CodeHighlighter, Roles},
    ui::{Type as _, code_block_scroll_anchor, markdown_style},
    workspace::column_row,
};

pub fn render_reading(text: &str, roles: Roles, code: Arc<CodeHighlighter>) -> AnyElement {
    perf_span!("reading.render");
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
                        .shared_code_block_highlighter(code)
                        // An actions slot gives each code block an element id, which its sideways scroll needs.
                        .code_block_actions(|_, _, _| code_block_scroll_anchor())
                        .on_link_click(|_, _, _, _| {})
                        .link_tooltip(|url, window, cx| {
                            component::tooltip::Tooltip::new(url.clone()).build(window, cx)
                        }),
                ),
        ))
        .overflow_y_scrollbar()
        .into_any_element()
}
