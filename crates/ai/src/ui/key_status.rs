use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyStorage {
    Keyring,
    PlainText,
    Session,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyState {
    pub managed: bool,
    pub present: bool,
    pub storage: KeyStorage,
    pub keyring_available: bool,
    pub keyring_stored: bool,
    pub backend: String,
    pub unreadable: bool,
}

pub fn describe_key_state(state: &KeyState) -> String {
    match state.storage {
        KeyStorage::Keyring => format!("Stored in the system keyring ({}).", state.backend),
        KeyStorage::PlainText => "Stored as plain text on this machine, like .env.".to_owned(),
        KeyStorage::Session => "Kept for this session only. Quitting forgets it.".to_owned(),
        KeyStorage::None if !state.unreadable => "No key set.".to_owned(),
        KeyStorage::None if state.keyring_available => {
            "The stored key could not be read. Enter it again.".to_owned()
        }
        KeyStorage::None => "The system keyring is not available, so the stored key cannot be read. Unlock your keyring and restart Desafinado.".to_owned(),
    }
}

pub fn offers_plain_text(state: &KeyState) -> bool {
    state.present && state.storage == KeyStorage::Session && !state.keyring_available
}

pub fn saved_key_message(state: &KeyState) -> Option<&'static str> {
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

pub fn save_error_message(error_message: Option<&str>) -> &'static str {
    if error_message.is_some_and(|message| message.ends_with("Invalid key")) {
        "That does not look like a key. Paste it again."
    } else {
        "Could not save the key."
    }
}

pub fn remove_note(state: &KeyState) -> Option<&'static str> {
    (state.keyring_stored && state.storage != KeyStorage::Keyring)
        .then_some("Remove key also deletes the key stored in your keyring.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> KeyState {
        KeyState {
            managed: true,
            present: false,
            storage: KeyStorage::None,
            keyring_available: true,
            keyring_stored: false,
            backend: "gnome_libsecret".into(),
            unreadable: false,
        }
    }

    fn present(storage: KeyStorage) -> KeyState {
        KeyState {
            present: true,
            storage,
            ..state()
        }
    }

    #[test]
    fn says_there_is_no_key() {
        assert_eq!(describe_key_state(&state()), "No key set.");
    }

    #[test]
    fn says_where_a_stored_key_lives() {
        assert_eq!(
            describe_key_state(&present(KeyStorage::Keyring)),
            "Stored in the system keyring (gnome_libsecret)."
        );
        assert_eq!(
            describe_key_state(&KeyState {
                backend: "kwallet6".into(),
                ..present(KeyStorage::Keyring)
            }),
            "Stored in the system keyring (kwallet6)."
        );
    }

    #[test]
    fn warns_about_plain_text_storage() {
        assert_eq!(
            describe_key_state(&present(KeyStorage::PlainText)),
            "Stored as plain text on this machine, like .env."
        );
    }

    #[test]
    fn says_a_session_key_is_forgotten_on_quit() {
        assert_eq!(
            describe_key_state(&present(KeyStorage::Session)),
            "Kept for this session only. Quitting forgets it."
        );
    }

    #[test]
    fn asks_for_the_key_again_when_the_stored_one_cannot_be_read() {
        assert_eq!(
            describe_key_state(&KeyState {
                unreadable: true,
                ..state()
            }),
            "The stored key could not be read. Enter it again."
        );
    }

    #[test]
    fn blames_the_keyring_when_the_stored_one_cannot_be_read_and_the_keyring_is_not_available() {
        assert_eq!(
            describe_key_state(&KeyState {
                unreadable: true,
                keyring_available: false,
                ..state()
            }),
            "The system keyring is not available, so the stored key cannot be read. Unlock your keyring and restart Desafinado."
        );
    }

    #[test]
    fn offers_plain_text_only_for_a_session_key_with_no_working_keyring() {
        assert!(offers_plain_text(&KeyState {
            keyring_available: false,
            ..present(KeyStorage::Session)
        }));
    }

    #[test]
    fn does_not_offer_plain_text_in_any_other_state() {
        assert!(!offers_plain_text(&state()));
        assert!(!offers_plain_text(&present(KeyStorage::Keyring)));
        assert!(!offers_plain_text(&present(KeyStorage::PlainText)));
        assert!(!offers_plain_text(&present(KeyStorage::Session)));
    }

    #[test]
    fn names_an_invalid_key() {
        assert_eq!(
            save_error_message(Some(
                "Error invoking remote method 'key:save': Error: Invalid key"
            )),
            "That does not look like a key. Paste it again."
        );
    }

    #[test]
    fn does_not_echo_anything_else() {
        assert_eq!(
            save_error_message(Some("boom sk-or-v1-secret")),
            "Could not save the key."
        );
        assert_eq!(save_error_message(None), "Could not save the key.");
    }

    const NOTE: &str = "Remove key also deletes the key stored in your keyring.";

    #[test]
    fn warns_when_a_keyring_key_sits_behind_a_session_key() {
        assert_eq!(
            remove_note(&KeyState {
                keyring_available: false,
                keyring_stored: true,
                ..present(KeyStorage::Session)
            }),
            Some(NOTE)
        );
    }

    #[test]
    fn warns_when_a_keyring_key_cannot_be_read() {
        assert_eq!(
            remove_note(&KeyState {
                unreadable: true,
                keyring_available: false,
                keyring_stored: true,
                ..state()
            }),
            Some(NOTE)
        );
    }

    #[test]
    fn says_nothing_when_the_key_shown_is_the_keyring_key() {
        assert_eq!(
            remove_note(&KeyState {
                keyring_stored: true,
                ..present(KeyStorage::Keyring)
            }),
            None
        );
    }

    #[test]
    fn says_nothing_when_no_keyring_key_is_stored() {
        assert_eq!(remove_note(&state()), None);
        assert_eq!(remove_note(&present(KeyStorage::Session)), None);
        assert_eq!(remove_note(&present(KeyStorage::PlainText)), None);
    }

    #[test]
    fn confirms_a_key_that_is_now_stored_so_the_dialog_can_close() {
        assert_eq!(
            saved_key_message(&present(KeyStorage::Keyring)),
            Some("Key saved in the system keyring.")
        );
        assert_eq!(
            saved_key_message(&present(KeyStorage::PlainText)),
            Some("Key saved as plain text.")
        );
    }

    #[test]
    fn confirms_a_session_key_when_there_is_no_plain_text_choice_left() {
        assert_eq!(
            saved_key_message(&present(KeyStorage::Session)),
            Some("Key kept for this session only.")
        );
    }

    #[test]
    fn keeps_the_dialog_open_while_it_still_offers_plain_text() {
        assert_eq!(
            saved_key_message(&KeyState {
                keyring_available: false,
                ..present(KeyStorage::Session)
            }),
            None
        );
    }

    #[test]
    fn keeps_the_dialog_open_when_no_key_is_set() {
        assert_eq!(saved_key_message(&state()), None);
        assert_eq!(
            saved_key_message(&KeyState {
                unreadable: true,
                ..state()
            }),
            None
        );
    }
}
