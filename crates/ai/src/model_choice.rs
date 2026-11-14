use std::sync::LazyLock;

use regex::Regex;

use crate::text::js_trim;

pub const MODEL_CHANGED_EVENT: &str = "desafinado:model-changed";
pub const MODEL_STORAGE_KEY: &str = "desafinado.model";

static MODEL_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^[a-z0-9][a-z0-9._-]*/[a-z0-9][a-z0-9._:-]*$").expect("valid regex")
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelChoice {
    Default,
    Model { id: String },
    Invalid { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageError;

pub trait ModelStorage {
    fn get_item(&self, key: &str) -> Result<Option<String>, StorageError>;
    fn set_item(&mut self, key: &str, value: &str) -> Result<(), StorageError>;
    fn remove_item(&mut self, key: &str) -> Result<(), StorageError>;
}

pub fn is_model_id(text: &str) -> bool {
    MODEL_ID.is_match(text)
}

pub fn parse_model_choice(text: &str) -> ModelChoice {
    let id = js_trim(text);
    if id.is_empty() {
        return ModelChoice::Default;
    }
    if is_model_id(id) {
        ModelChoice::Model { id: id.to_owned() }
    } else {
        ModelChoice::Invalid {
            message: "Use an OpenRouter id such as provider/model.".to_owned(),
        }
    }
}

pub fn read_model_choice(storage: &impl ModelStorage) -> Option<String> {
    storage
        .get_item(MODEL_STORAGE_KEY)
        .ok()
        .flatten()
        .filter(|stored| is_model_id(stored))
}

pub fn write_model_choice(storage: &mut impl ModelStorage, id: Option<&str>) {
    let _ = match id {
        None => storage.remove_item(MODEL_STORAGE_KEY),
        Some(id) => storage.set_item(MODEL_STORAGE_KEY, id),
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Memory(HashMap<String, String>);

    impl ModelStorage for Memory {
        fn get_item(&self, key: &str) -> Result<Option<String>, StorageError> {
            Ok(self.0.get(key).cloned())
        }
        fn set_item(&mut self, key: &str, value: &str) -> Result<(), StorageError> {
            self.0.insert(key.to_owned(), value.to_owned());
            Ok(())
        }
        fn remove_item(&mut self, key: &str) -> Result<(), StorageError> {
            self.0.remove(key);
            Ok(())
        }
    }

    struct Broken;

    impl ModelStorage for Broken {
        fn get_item(&self, _: &str) -> Result<Option<String>, StorageError> {
            Err(StorageError)
        }
        fn set_item(&mut self, _: &str, _: &str) -> Result<(), StorageError> {
            Err(StorageError)
        }
        fn remove_item(&mut self, _: &str) -> Result<(), StorageError> {
            Err(StorageError)
        }
    }

    #[test]
    fn reads_an_open_router_model_id() {
        assert_eq!(
            parse_model_choice(" openai/gpt-6.1-terra "),
            ModelChoice::Model {
                id: "openai/gpt-6.1-terra".into()
            }
        );
        assert_eq!(
            parse_model_choice("deepseek/deepseek-v4.1-flash:free"),
            ModelChoice::Model {
                id: "deepseek/deepseek-v4.1-flash:free".into()
            }
        );
    }

    #[test]
    fn reads_empty_text_as_the_default_model() {
        assert_eq!(parse_model_choice("   "), ModelChoice::Default);
    }

    #[test]
    fn refuses_text_that_is_not_a_provider_model_id() {
        for text in ["gpt", "a b/c", "https://x.y/z"] {
            assert!(matches!(
                parse_model_choice(text),
                ModelChoice::Invalid { .. }
            ));
        }
    }

    #[test]
    fn round_trips_a_model_and_clears_it_with_none() {
        let mut storage = Memory::default();
        write_model_choice(&mut storage, Some("openai/gpt-6.1-terra"));
        assert_eq!(
            read_model_choice(&storage).as_deref(),
            Some("openai/gpt-6.1-terra")
        );
        write_model_choice(&mut storage, None);
        assert_eq!(read_model_choice(&storage), None);
    }

    #[test]
    fn ignores_a_stored_value_that_is_not_a_model_id() {
        let mut storage = Memory::default();
        storage.set_item("desafinado.model", "junk").unwrap();
        assert_eq!(read_model_choice(&storage), None);
    }

    #[test]
    fn survives_storage_that_throws() {
        let mut broken = Broken;
        assert_eq!(read_model_choice(&broken), None);
        write_model_choice(&mut broken, Some("a/b"));
    }

    #[test]
    fn names_its_change_event() {
        assert_eq!(MODEL_CHANGED_EVENT, "desafinado:model-changed");
    }
}
