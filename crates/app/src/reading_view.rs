use std::sync::Arc;

use desafinado_core::blocks::body_after_frontmatter;
use gpui_kit::{
    base::{TextView, TextViewState},
    component, *,
};

use crate::{
    theme::{CodeHighlighter, Roles},
    ui::{Type as _, code_block_scroll_anchor, markdown_style},
};

#[derive(Debug, Default)]
pub struct ReadingFeed {
    revision: u64,
    fed: Option<u64>,
}

impl ReadingFeed {
    pub fn touch(&mut self) {
        self.revision += 1;
    }

    pub fn due(&mut self) -> bool {
        let due = self.fed != Some(self.revision);
        self.fed = Some(self.revision);
        due
    }
}

pub struct ReadingView {
    state: Entity<TextViewState>,
}

impl ReadingView {
    pub fn new(cx: &mut App) -> Self {
        Self {
            state: cx.new(|cx| TextViewState::markdown("", cx)),
        }
    }

    pub fn feed(&self, text: &str, cx: &mut App) {
        perf_span!("reading.feed");
        self.state.update(cx, |state, cx| {
            state.set_text(body_after_frontmatter(text), cx);
        });
    }

    pub fn render(&self, roles: Roles, code: Arc<CodeHighlighter>) -> AnyElement {
        perf_span!("reading.render");
        div()
            .id("reading-view")
            .size_full()
            .type_ui()
            .type_document()
            .line_height(relative(1.75))
            .text_color(roles.ink)
            .child(
                TextView::new(&self.state)
                    .scrollable(true)
                    .lazy_measure(true)
                    .content_max_width(px(720.))
                    .px(px(24.))
                    .scroll_padding(px(16.), px(60.))
                    .style(markdown_style(roles))
                    .selectable(true)
                    .shared_code_block_highlighter(code)
                    // An actions slot gives each code block an element id, which its sideways scroll needs.
                    .code_block_actions(|_, _, _| code_block_scroll_anchor())
                    .on_link_click(|_, _, _, _| {})
                    .link_tooltip(|url, window, cx| {
                        component::tooltip::Tooltip::new(url.clone()).build(window, cx)
                    }),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::ReadingFeed;

    #[test]
    fn a_new_feed_is_due_once() {
        let mut feed = ReadingFeed::default();
        assert!(feed.due());
        assert!(!feed.due());
    }

    #[test]
    fn a_touch_makes_the_feed_due_again_once() {
        let mut feed = ReadingFeed::default();
        assert!(feed.due());
        feed.touch();
        feed.touch();
        assert!(feed.due());
        assert!(!feed.due());
    }
}
