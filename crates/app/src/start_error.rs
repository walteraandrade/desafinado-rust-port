use gpui_kit::*;

use crate::theme;

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
            .font_family("Inter")
            .child(component::TitleBar::new())
            .child(
                div().w_full().flex().justify_center().child(
                    div()
                        .w(px(768.))
                        .max_w_full()
                        .px(px(24.))
                        .pt(px(40.))
                        .child(
                            div()
                                .font_family("League Spartan")
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_size(px(24.))
                                .child("Desafinado could not start"),
                        )
                        .child(
                            div()
                                .pt(px(12.))
                                .text_size(px(14.))
                                .text_color(roles.danger)
                                .child(self.message.clone()),
                        )
                        .child(
                            div()
                                .pt(px(12.))
                                .text_size(px(14.))
                                .child("Close the window and start the app again."),
                        ),
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
