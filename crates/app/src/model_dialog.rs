use std::sync::Arc;

use desafinado_ai::model_choice::{ModelChoice, parse_model_choice};
use desafinado_runtime::services::Services;
use gpui_kit::{
    component::input::{Input, InputEvent, InputState},
    prelude::FluentBuilder as _,
    *,
};

use crate::{
    theme,
    ui::{self, ButtonKind},
};

actions!(model_dialog, [CloseModelDialog, SubmitModel]);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CloseModelDialog, Some("ModelDialog")),
        KeyBinding::new("enter", SubmitModel, Some("ModelDialog > Input")),
    ]);
}

pub enum ModelEvent {
    Saved(String),
    Close,
}

impl EventEmitter<ModelEvent> for ModelDialog {}

pub struct ModelDialog {
    services: Arc<Services>,
    default_model: String,
    input: Entity<InputState>,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

pub fn saved_model_message(choice: &ModelChoice, default_model: &str) -> Option<String> {
    match choice {
        ModelChoice::Model { id } => Some(format!("Model: {id}")),
        ModelChoice::Default => Some(format!("Model: {default_model} (default)")),
        ModelChoice::Invalid { .. } => None,
    }
}

impl ModelDialog {
    pub fn new(services: Arc<Services>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let default_model = services.default_model();
        let current = services.model_choice().unwrap_or_default();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(default_model.clone())
                .default_value(current)
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        let changes = cx.subscribe(&input, |this: &mut Self, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.error = None;
                cx.notify();
            }
        });
        Self {
            services,
            default_model,
            input,
            error: None,
            _subscriptions: vec![changes],
        }
    }

    fn submit(&mut self, _: &SubmitModel, _: &mut Window, cx: &mut Context<Self>) {
        let choice = parse_model_choice(&self.input.read(cx).value());
        match &choice {
            ModelChoice::Invalid { message } => {
                self.error = Some(message.clone());
                cx.notify();
                return;
            }
            ModelChoice::Model { id } => self.services.set_model(id),
            ModelChoice::Default => self.services.use_default_model(),
        }
        if let Some(message) = saved_model_message(&choice, &self.default_model) {
            cx.emit(ModelEvent::Saved(message));
        }
        cx.emit(ModelEvent::Close);
    }

    fn close(&mut self, _: &CloseModelDialog, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(ModelEvent::Close);
    }
}

impl Render for ModelDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let roles = theme::roles(cx);
        let focused = self.input.read(cx).focus_handle(cx).is_focused(window);
        ui::modal_backdrop(
            "model-backdrop",
            roles,
            ui::dialog_frame("model-dialog", px(448.), roles)
                .key_context("ModelDialog")
                .on_action(cx.listener(Self::submit))
                .on_action(cx.listener(Self::close))
                .child(ui::title("Model", roles))
                .child(ui::detail(
                    format!(
                        "An OpenRouter model id. Empty uses the default, {}. The next request uses the new model.",
                        self.default_model
                    ),
                    roles,
                ))
                .child(div().mt(px(16.)).child(ui::field(
                    Input::new(&self.input).appearance(false),
                    focused,
                    self.error.is_some(),
                    roles,
                )))
                .when_some(self.error.clone(), |this, error| {
                    this.child(ui::error_text(error, roles).mt(px(8.)))
                })
                .child(
                    ui::actions_row()
                        .child(
                            ui::button("model-cancel", "Cancel", ButtonKind::Default, roles)
                                .on_click(cx.listener(|_, _, _, cx| cx.emit(ModelEvent::Close))),
                        )
                        .child(
                            ui::button("model-save", "Save model", ButtonKind::Strong, roles)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.submit(&SubmitModel, window, cx)
                                })),
                        ),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_model_choice, saved_model_message};

    #[test]
    fn says_which_model_the_next_request_uses() {
        assert_eq!(
            saved_model_message(&parse_model_choice(" vendor/model-a "), "x/y").as_deref(),
            Some("Model: vendor/model-a")
        );
        assert_eq!(
            saved_model_message(&parse_model_choice(""), "x/y").as_deref(),
            Some("Model: x/y (default)")
        );
        assert_eq!(
            saved_model_message(&parse_model_choice("nope"), "x/y"),
            None
        );
    }
}
