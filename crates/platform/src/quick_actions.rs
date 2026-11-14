use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::files::write_atomic_unconditionally;

pub const QUICK_ACTIONS_FILE_NAME: &str = "quick-actions.json";
const MAX_TEXT_LENGTH: usize = 256 * 1024;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum QuickActionsError {
    #[error("Invalid Quick actions")]
    Invalid,
    #[error("Could not keep the Quick actions: {0}")]
    Io(String),
}

pub fn quick_actions_file(user_data_dir: &Path) -> PathBuf {
    user_data_dir.join(QUICK_ACTIONS_FILE_NAME)
}

pub fn load_quick_actions(user_data_dir: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(quick_actions_file(user_data_dir)) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn save_quick_actions(user_data_dir: &Path, text: &str) -> Result<(), QuickActionsError> {
    if text.chars().count() > MAX_TEXT_LENGTH
        || serde_json::from_str::<serde_json::Value>(text).is_err()
    {
        return Err(QuickActionsError::Invalid);
    }
    fs::create_dir_all(user_data_dir).map_err(|e| QuickActionsError::Io(e.to_string()))?;
    write_atomic_unconditionally(&quick_actions_file(user_data_dir), text)
        .map_err(|e| QuickActionsError::Io(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_nothing_when_the_file_does_not_exist() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_quick_actions(dir.path()).unwrap(), None);
    }

    #[test]
    fn saves_and_loads_the_text() {
        let dir = tempfile::tempdir().unwrap();
        let text = r#"{"version":1,"actions":[]}"#;
        save_quick_actions(dir.path(), text).unwrap();
        assert_eq!(
            load_quick_actions(dir.path()).unwrap().as_deref(),
            Some(text)
        );
    }

    #[test]
    fn creates_the_user_data_folder_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a/b");
        save_quick_actions(&nested, "{}").unwrap();
        assert!(nested.join(QUICK_ACTIONS_FILE_NAME).is_file());
    }

    #[test]
    fn refuses_text_that_is_not_json_or_too_long() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            save_quick_actions(dir.path(), "nope"),
            Err(QuickActionsError::Invalid)
        );
        let long = format!("\"{}\"", "a".repeat(MAX_TEXT_LENGTH));
        assert_eq!(
            save_quick_actions(dir.path(), &long),
            Err(QuickActionsError::Invalid)
        );
        assert_eq!(load_quick_actions(dir.path()).unwrap(), None);
    }
}
