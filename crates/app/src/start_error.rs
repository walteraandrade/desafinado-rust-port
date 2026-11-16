use gpui_kit::*;

use crate::{
    theme,
    ui::{self, Type as _},
};

const FALLBACK_MESSAGE: &str = "Unknown error";

pub fn start_error_message(message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        FALLBACK_MESSAGE.into()
    } else {
        trimmed.into()
    }
}

pub struct StartError {
    message: String,
}

impl StartError {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

impl Render for StartError {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let roles = theme::roles(cx);
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(roles.canvas)
            .text_color(roles.ink)
            .type_ui()
            .type_body()
            .child(component::TitleBar::new())
            .child(
                div().w_full().flex().justify_center().child(
                    div()
                        .w(px(768.))
                        .max_w_full()
                        .px(px(24.))
                        .pt(px(40.))
                        .child(ui::wordmark(roles))
                        .child(ui::title("Desafinado could not start", roles).mt(px(16.)))
                        .child(ui::error_text(self.message.clone(), roles).mt(px(12.)))
                        .child(ui::detail(
                            "Close the window and start the app again.",
                            roles,
                        )),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::start_error_message;

    #[test]
    fn shows_the_message_or_a_fallback() {
        assert_eq!(start_error_message("  disk full \n"), "disk full");
        assert_eq!(start_error_message("   "), "Unknown error");
    }
}
