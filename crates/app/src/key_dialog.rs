use std::sync::Arc;

use desafinado_platform::key::{KeyDetails, KeyStorage, parse_key_input};
use desafinado_runtime::services::Services;
use gpui_kit::{
    component::input::{Input, InputEvent, InputState},
    prelude::FluentBuilder as _,
    *,
};

use crate::{
    theme,
    ui::{self, ButtonKind, ButtonSize},
};

actions!(key_dialog, [CloseKeyDialog, SubmitKey]);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CloseKeyDialog, Some("KeyDialog")),
        KeyBinding::new("enter", SubmitKey, Some("KeyDialog > Input")),
    ]);
}

pub fn describe_key_state(state: &KeyDetails) -> String {
    match state.storage {
        KeyStorage::Keyring => format!("Stored in the system keyring ({}).", state.backend),
        KeyStorage::PlainText => "Stored as plain text on this machine, like .env.".into(),
        KeyStorage::Session => "Kept for this session only. Quitting forgets it.".into(),
        KeyStorage::None if !state.unreadable => "No key set.".into(),
        KeyStorage::None if state.keyring_available => {
            "The stored key could not be read. Enter it again.".into()
        }
        KeyStorage::None => "The system keyring is not available, so the stored key cannot be read. Unlock your keyring and restart Desafinado.".into(),
    }
}

pub fn offers_plain_text(state: &KeyDetails) -> bool {
    state.present && state.storage == KeyStorage::Session && !state.keyring_available
}

pub fn saved_key_message(state: &KeyDetails) -> Option<&'static str> {
    if !state.present || offers_plain_text(state) {
        return None;
    }
    match state.storage {
        KeyStorage::Keyring => Some("Key saved in the system keyring."),
        KeyStorage::PlainText => Some("Key saved as plain text."),
        KeyStorage::Session => Some("Key kept for this session only."),
        KeyStorage::None => None,
    }
}

pub fn remove_note(state: &KeyDetails) -> Option<&'static str> {
    (state.keyring_stored && state.storage != KeyStorage::Keyring)
        .then_some("Remove key also deletes the key stored in your keyring.")
}

pub const INVALID_KEY_MESSAGE: &str = "That does not look like a key. Paste it again.";

pub enum KeyEvent {
    State(KeyDetails),
    Saved(String),
    Close,
}

impl EventEmitter<KeyEvent> for KeyDialog {}

#[derive(Clone, Copy)]
enum KeyCommand {
    Save,
    PlainText,
    Clear,
}

pub struct KeyDialog {
    services: Arc<Services>,
    state: KeyDetails,
    input: Entity<InputState>,
    busy: bool,
    error: Option<String>,
    _task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl KeyDialog {
    pub fn new(services: Arc<Services>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = services.keys.status();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Paste the key")
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
            state,
            input,
            busy: false,
            error: None,
            _task: None,
            _subscriptions: vec![changes],
        }
    }

    fn value(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    fn run(&mut self, command: KeyCommand, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let key = match command {
            KeyCommand::Save => {
                let value = self.value(cx);
                if value.trim().is_empty() {
                    return;
                }
                match parse_key_input(&value) {
                    Ok(key) => Some(key),
                    Err(_) => {
                        self.error = Some(INVALID_KEY_MESSAGE.into());
                        cx.notify();
                        return;
                    }
                }
            }
            _ => None,
        };
        self.busy = true;
        self.error = None;
        let services = self.services.clone();
        let work = cx.background_spawn(async move {
            match (command, key) {
                (KeyCommand::Save, Some(key)) => services.keys.save(&key),
                (KeyCommand::PlainText, _) => services.keys.keep_as_plain_text(),
                (KeyCommand::Clear, _) => services.keys.clear(),
                _ => services.keys.status(),
            }
        });
        let window_handle = window.window_handle();
        self._task = Some(cx.spawn(async move |this, cx| {
            let next = work.await;
            let _ = window_handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |this, cx| this.finish(command, next, window, cx));
            });
        }));
        cx.notify();
    }

    fn finish(
        &mut self,
        command: KeyCommand,
        next: KeyDetails,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.busy = false;
        self.state = next.clone();
        cx.emit(KeyEvent::State(next.clone()));
        if matches!(command, KeyCommand::Clear) {
            cx.notify();
            return;
        }
        if matches!(command, KeyCommand::Save) && !next.present {
            self.error = Some("Could not save the key.".into());
            cx.notify();
            return;
        }
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        if let Some(message) = saved_key_message(&next) {
            cx.emit(KeyEvent::Saved(message.into()));
            cx.emit(KeyEvent::Close);
        }
        cx.notify();
    }

    fn submit(&mut self, _: &SubmitKey, window: &mut Window, cx: &mut Context<Self>) {
        self.run(KeyCommand::Save, window, cx);
    }

    fn close(&mut self, _: &CloseKeyDialog, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(KeyEvent::Close);
    }
}

impl KeyDialog {
    fn render_plain_text_offer(&self, roles: theme::Roles, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(12.))
            .bg(roles.raised)
            .child(
                "No system keyring is available. Storing the key as plain text keeps it across restarts, but anyone who can read your files can read it, like .env today.",
            )
            .child(
                div().flex().child(
                    ui::sized_button(
                        "key-plain-text",
                        "Store as plain text",
                        ButtonKind::Default,
                        ButtonSize::Regular,
                        ui::Control::disabled(self.busy),
                        roles,
                    )
                    .when(!self.busy, |this| {
                        this.on_click(cx.listener(|this, _, window, cx| {
                            this.run(KeyCommand::PlainText, window, cx)
                        }))
                    }),
                ),
            )
    }

    fn render_buttons(
        &self,
        can_remove: bool,
        empty: bool,
        roles: theme::Roles,
        cx: &mut Context<Self>,
    ) -> Div {
        ui::row()
            .gap(px(8.))
            .when(can_remove, |this| {
                this.child(
                    ui::sized_button(
                        "key-remove",
                        "Remove key",
                        ButtonKind::Danger,
                        ButtonSize::Regular,
                        ui::Control::disabled(self.busy),
                        roles,
                    )
                    .when(!self.busy, |this| {
                        this.on_click(cx.listener(|this, _, window, cx| {
                            this.run(KeyCommand::Clear, window, cx)
                        }))
                    }),
                )
            })
            .child(div().flex_1())
            .child(
                ui::button("key-close", "Close", ButtonKind::Default, roles)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(KeyEvent::Close))),
            )
            .child(
                ui::sized_button(
                    "key-save",
                    "Save key",
                    ButtonKind::Strong,
                    ButtonSize::Regular,
                    ui::Control::disabled(self.busy || empty),
                    roles,
                )
                .when(!self.busy && !empty, |this| {
                    this.on_click(
                        cx.listener(|this, _, window, cx| this.run(KeyCommand::Save, window, cx)),
                    )
                }),
            )
    }
}

impl Render for KeyDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let roles = theme::roles(cx);
        let state = self.state.clone();
        let empty = self.value(cx).trim().is_empty();
        let focused = self.input.read(cx).focus_handle(cx).is_focused(window);
        let can_remove = state.present || state.unreadable;
        ui::modal_backdrop(
            "key-backdrop",
            roles,
            ui::dialog_frame("key-dialog", px(544.), roles)
                .key_context("KeyDialog")
                .on_action(cx.listener(Self::submit))
                .on_action(cx.listener(Self::close))
                .gap(px(16.))
                .child(ui::title("OpenRouter key", roles))
                .child(
                    div()
                        .text_color(roles.ink)
                        .child(describe_key_state(&state)),
                )
                .child(
                    div()
                        .child(ui::label(
                            if state.present {
                                "Replace the key"
                            } else {
                                "Key"
                            },
                            roles,
                        ))
                        .child(ui::field(
                            Input::new(&self.input).appearance(false),
                            focused,
                            self.error.is_some(),
                            roles,
                        )),
                )
                .when(offers_plain_text(&state), |this| {
                    this.child(self.render_plain_text_offer(roles, cx))
                })
                .when_some(self.error.clone(), |this, error| {
                    this.child(ui::error_text(error, roles))
                })
                .when_some(remove_note(&state), |this, note| {
                    this.child(div().text_color(roles.ink).child(note))
                })
                .child(self.render_buttons(can_remove, empty, roles, cx)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        KeyDetails, KeyStorage, describe_key_state, offers_plain_text, remove_note,
        saved_key_message,
    };

    fn details(storage: KeyStorage) -> KeyDetails {
        KeyDetails {
            present: storage != KeyStorage::None,
            storage,
            keyring_available: true,
            keyring_stored: false,
            backend: "gnome-libsecret".into(),
            unreadable: false,
        }
    }

    #[test]
    fn describes_each_storage() {
        assert_eq!(
            describe_key_state(&details(KeyStorage::Keyring)),
            "Stored in the system keyring (gnome-libsecret)."
        );
        assert_eq!(
            describe_key_state(&details(KeyStorage::None)),
            "No key set."
        );
        let unreadable = KeyDetails {
            unreadable: true,
            ..details(KeyStorage::None)
        };
        assert_eq!(
            describe_key_state(&unreadable),
            "The stored key could not be read. Enter it again."
        );
        assert!(
            describe_key_state(&KeyDetails {
                keyring_available: false,
                ..unreadable
            })
            .starts_with("The system keyring is not available")
        );
    }

    #[test]
    fn offers_plain_text_only_for_a_session_key_without_a_keyring() {
        let session = KeyDetails {
            keyring_available: false,
            ..details(KeyStorage::Session)
        };
        assert!(offers_plain_text(&session));
        assert_eq!(saved_key_message(&session), None);
        assert!(!offers_plain_text(&details(KeyStorage::Session)));
        assert_eq!(
            saved_key_message(&details(KeyStorage::Session)),
            Some("Key kept for this session only.")
        );
        assert_eq!(
            saved_key_message(&details(KeyStorage::PlainText)),
            Some("Key saved as plain text.")
        );
    }

    #[test]
    fn notes_that_remove_also_clears_the_keyring() {
        let stored = KeyDetails {
            keyring_stored: true,
            ..details(KeyStorage::Session)
        };
        assert!(remove_note(&stored).is_some());
        assert!(
            remove_note(&KeyDetails {
                keyring_stored: true,
                ..details(KeyStorage::Keyring)
            })
            .is_none()
        );
    }
}
