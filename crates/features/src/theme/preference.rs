use super::catalog::{default_theme, theme_by_id};
use super::tokens::ThemeTokens;

pub const THEME_STORAGE_KEY: &str = "desafinado.theme";
pub const OMARCHY_CHOICE: &str = "omarchy";

pub type ThemeChoice = String;

pub trait KeyValueStorage {
    fn get_item(&self, key: &str) -> Result<Option<String>, String>;
    fn set_item(&self, key: &str, value: &str) -> Result<(), String>;
}

pub fn load_theme_choice(storage: Option<&dyn KeyValueStorage>) -> ThemeChoice {
    storage
        .and_then(|storage| storage.get_item(THEME_STORAGE_KEY).ok().flatten())
        .filter(|stored| theme_by_id(stored).is_some())
        .unwrap_or_else(|| OMARCHY_CHOICE.to_string())
}

pub fn save_theme_choice(storage: Option<&dyn KeyValueStorage>, choice: &str) {
    if let Some(storage) = storage {
        let _ = storage.set_item(THEME_STORAGE_KEY, choice);
    }
}

pub fn active_theme<'a>(choice: &str, omarchy: Option<&'a ThemeTokens>) -> &'a ThemeTokens {
    match (choice == OMARCHY_CHOICE, omarchy) {
        (true, Some(tokens)) => tokens,
        _ => &theme_by_id(choice).unwrap_or_else(default_theme).tokens,
    }
}

pub fn selected_option(choice: &str, omarchy: Option<&ThemeTokens>) -> String {
    if choice == OMARCHY_CHOICE && omarchy.is_none() {
        default_theme().id.clone()
    } else {
        choice.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use super::*;
    use crate::theme::catalog::desafinado_theme;
    use crate::theme::screen_palettes::THEME_SEEDS;

    #[derive(Default)]
    struct Memory(RefCell<HashMap<String, String>>);

    impl Memory {
        fn with(key: &str, value: &str) -> Self {
            let memory = Self::default();
            memory.0.borrow_mut().insert(key.into(), value.into());
            memory
        }
    }

    impl KeyValueStorage for Memory {
        fn get_item(&self, key: &str) -> Result<Option<String>, String> {
            Ok(self.0.borrow().get(key).cloned())
        }
        fn set_item(&self, key: &str, value: &str) -> Result<(), String> {
            self.0.borrow_mut().insert(key.into(), value.into());
            Ok(())
        }
    }

    struct Throwing;

    impl KeyValueStorage for Throwing {
        fn get_item(&self, _: &str) -> Result<Option<String>, String> {
            Err("blocked".into())
        }
        fn set_item(&self, _: &str, _: &str) -> Result<(), String> {
            Err("blocked".into())
        }
    }

    const MOOD_INDIGO: &str = "mood-indigo";

    fn omarchy() -> ThemeTokens {
        ThemeTokens {
            name: "vantablack".into(),
            mode: crate::theme::tokens::ThemeMode::Dark,
            roles: desafinado_theme().tokens.roles.clone(),
        }
    }

    #[test]
    fn follows_omarchy_by_default() {
        assert_eq!(load_theme_choice(Some(&Memory::default())), OMARCHY_CHOICE);
        assert_eq!(load_theme_choice(None), OMARCHY_CHOICE);
        assert_eq!(load_theme_choice(Some(&Throwing)), OMARCHY_CHOICE);
    }

    #[test]
    fn reads_a_saved_theme_and_ignores_unknown_values() {
        let saved = |value| load_theme_choice(Some(&Memory::with(THEME_STORAGE_KEY, value)));
        assert_eq!(saved("desafinado"), "desafinado");
        assert_eq!(saved(MOOD_INDIGO), MOOD_INDIGO);
        assert_eq!(saved("neon"), OMARCHY_CHOICE);
    }

    #[test]
    fn round_trips_through_storage() {
        let storage = Memory::default();
        save_theme_choice(Some(&storage), MOOD_INDIGO);
        assert_eq!(load_theme_choice(Some(&storage)), MOOD_INDIGO);
    }

    #[test]
    fn swallows_storage_errors() {
        save_theme_choice(Some(&Throwing), "desafinado");
    }

    #[test]
    fn uses_omarchy_when_chosen_and_present() {
        let tokens = omarchy();
        assert!(std::ptr::eq(
            active_theme(OMARCHY_CHOICE, Some(&tokens)),
            &tokens
        ));
    }

    #[test]
    fn falls_back_to_the_default_theme_without_omarchy() {
        assert_eq!(active_theme(OMARCHY_CHOICE, None), &default_theme().tokens);
    }

    #[test]
    fn uses_the_chosen_theme_even_with_omarchy_present() {
        let tokens = omarchy();
        assert_eq!(active_theme(MOOD_INDIGO, Some(&tokens)).name, MOOD_INDIGO);
        assert_eq!(
            active_theme("desafinado", Some(&tokens)),
            &desafinado_theme().tokens
        );
    }

    #[test]
    fn falls_back_to_the_default_theme_for_an_unknown_id() {
        assert_eq!(active_theme("neon", None), &default_theme().tokens);
    }

    #[test]
    fn knows_every_palette() {
        for seed in THEME_SEEDS {
            assert_eq!(active_theme(seed.id, None).name, seed.id);
        }
    }

    #[test]
    fn marks_omarchy_only_when_it_is_present() {
        assert_eq!(
            selected_option(OMARCHY_CHOICE, Some(&omarchy())),
            OMARCHY_CHOICE
        );
        assert_eq!(selected_option(OMARCHY_CHOICE, None), default_theme().id);
    }

    #[test]
    fn marks_the_chosen_theme() {
        assert_eq!(selected_option(MOOD_INDIGO, None), MOOD_INDIGO);
    }
}
