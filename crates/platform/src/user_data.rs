use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const APP_DIR_NAME: &str = "desafinado-native";
pub const USER_DATA_ENV: &str = "DESAFINADO_USER_DATA_DIR";

const ELECTRON_APP_DIR_NAME: &str = "desafinado";
const MIGRATED_ENTRIES: [&str; 4] = [
    "history.sqlite",
    "quick-actions.json",
    "library-roots.json",
    "eval-runs",
];

pub fn resolve_user_data_dir(
    read_var: &dyn Fn(&str) -> Option<String>,
    config_dir: Option<PathBuf>,
    home: Option<PathBuf>,
) -> PathBuf {
    if let Some(value) = read_var(USER_DATA_ENV) {
        let path = PathBuf::from(value.trim());
        if path.is_absolute() {
            return path;
        }
    }
    config_dir
        .or_else(|| home.map(|home| home.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR_NAME)
}

pub fn user_data_dir() -> PathBuf {
    resolve_user_data_dir(
        &|name| std::env::var(name).ok(),
        dirs::config_dir(),
        dirs::home_dir(),
    )
}

pub fn ensure_user_data_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)
}

pub fn electron_user_data_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join(ELECTRON_APP_DIR_NAME))
}

fn copy_entry(source: &Path, target: &Path) -> io::Result<()> {
    if source.is_dir() {
        fs::create_dir_all(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_entry(&entry.path(), &target.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(source, target).map(|_| ())
    }
}

pub fn migrate_from_electron(source: &Path, target: &Path) -> io::Result<Vec<String>> {
    if source == target || !source.is_dir() {
        return Ok(Vec::new());
    }
    fs::create_dir_all(target)?;
    let mut copied = Vec::new();
    for entry in MIGRATED_ENTRIES {
        let from = source.join(entry);
        let to = target.join(entry);
        if from.exists() && !to.exists() {
            copy_entry(&from, &to)?;
            if entry == "history.sqlite" {
                for suffix in ["-wal", "-shm"] {
                    let side = source.join(format!("{entry}{suffix}"));
                    if side.exists() {
                        fs::copy(&side, target.join(format!("{entry}{suffix}")))?;
                    }
                }
            }
            copied.push(entry.to_owned());
        }
    }
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_the_config_dir() {
        let dir = resolve_user_data_dir(&|_| None, Some("/home/me/.config".into()), None);
        assert_eq!(dir, PathBuf::from("/home/me/.config/desafinado-native"));
    }

    #[test]
    fn falls_back_to_home_dot_config() {
        let dir = resolve_user_data_dir(&|_| None, None, Some("/home/me".into()));
        assert_eq!(dir, PathBuf::from("/home/me/.config/desafinado-native"));
    }

    #[test]
    fn the_env_override_wins_when_absolute() {
        let dir = resolve_user_data_dir(
            &|name| (name == USER_DATA_ENV).then(|| "/tmp/check".to_owned()),
            Some("/home/me/.config".into()),
            None,
        );
        assert_eq!(dir, PathBuf::from("/tmp/check"));
    }

    #[test]
    fn a_relative_override_is_ignored() {
        let dir = resolve_user_data_dir(&|_| Some("relative".to_owned()), Some("/c".into()), None);
        assert_eq!(dir, PathBuf::from("/c/desafinado-native"));
    }

    #[test]
    fn migrates_app_data_without_the_key_file_and_never_overwrites() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        fs::write(source.path().join("history.sqlite"), "db").unwrap();
        fs::write(source.path().join("quick-actions.json"), "new").unwrap();
        fs::write(source.path().join("openrouter-key.json"), "secret").unwrap();
        fs::create_dir(source.path().join("eval-runs")).unwrap();
        fs::write(source.path().join("eval-runs/r.json"), "r").unwrap();
        fs::write(target.path().join("quick-actions.json"), "mine").unwrap();
        let copied = migrate_from_electron(source.path(), target.path()).unwrap();
        assert_eq!(copied, vec!["history.sqlite", "eval-runs"]);
        assert_eq!(
            fs::read_to_string(target.path().join("quick-actions.json")).unwrap(),
            "mine"
        );
        assert!(!target.path().join("openrouter-key.json").exists());
        assert_eq!(
            fs::read_to_string(target.path().join("eval-runs/r.json")).unwrap(),
            "r"
        );
    }
}
