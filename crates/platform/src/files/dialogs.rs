use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

pub trait FileDialogs: Send + Sync {
    fn pick_file(&self) -> Option<PathBuf>;
    fn pick_save_path(&self, suggested_name: &str) -> Option<PathBuf>;
    fn pick_folder(&self) -> Option<PathBuf>;
}

#[derive(Debug, Default)]
pub struct NativeDialogs {
    directory: Mutex<Option<PathBuf>>,
}

impl NativeDialogs {
    pub fn new() -> Self {
        Self::default()
    }

    fn remember(&self, path: PathBuf) -> PathBuf {
        if let Some(parent) = path.parent() {
            *self
                .directory
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(parent.to_path_buf());
        }
        path
    }

    fn start_directory(&self) -> Option<PathBuf> {
        self.directory
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl FileDialogs for NativeDialogs {
    fn pick_file(&self) -> Option<PathBuf> {
        let dialog = rfd::FileDialog::new().add_filter("Markdown", &["md", "markdown"]);
        let dialog = match self.start_directory() {
            Some(dir) => dialog.set_directory(dir),
            None => dialog,
        };
        dialog.pick_file().map(|path| self.remember(path))
    }

    fn pick_save_path(&self, suggested_name: &str) -> Option<PathBuf> {
        let dialog = rfd::FileDialog::new()
            .add_filter("Markdown", &["md", "markdown"])
            .set_file_name(suggested_name);
        let dialog = match self.start_directory() {
            Some(dir) => dialog.set_directory(dir),
            None => dialog,
        };
        dialog.save_file().map(|path| self.remember(path))
    }

    fn pick_folder(&self) -> Option<PathBuf> {
        let dialog = rfd::FileDialog::new();
        let dialog = match self.start_directory() {
            Some(dir) => dialog.set_directory(dir),
            None => dialog,
        };
        dialog.pick_folder().inspect(|path| {
            self.remember(path.clone());
        })
    }
}

#[derive(Debug, Clone)]
pub struct DevDialogs {
    pick_file: PathBuf,
}

impl DevDialogs {
    pub fn from_env(read_var: &dyn Fn(&str) -> Option<String>) -> Option<Self> {
        let value = read_var("DESAFINADO_DEV_PICK_FILE")?;
        let trimmed = value.trim();
        (!trimmed.is_empty() && Path::new(trimmed).is_absolute()).then(|| Self {
            pick_file: PathBuf::from(trimmed),
        })
    }

    fn next(&self) -> Option<PathBuf> {
        let text = std::fs::read_to_string(&self.pick_file).ok()?;
        let path = text.trim();
        (!path.is_empty()).then(|| PathBuf::from(path))
    }
}

impl FileDialogs for DevDialogs {
    fn pick_file(&self) -> Option<PathBuf> {
        self.next()
    }

    fn pick_save_path(&self, _suggested_name: &str) -> Option<PathBuf> {
        self.next()
    }

    fn pick_folder(&self) -> Option<PathBuf> {
        self.next()
    }
}

pub fn dialogs_from_env() -> Box<dyn FileDialogs> {
    match DevDialogs::from_env(&|name| std::env::var(name).ok()) {
        Some(dev) => Box::new(dev),
        None => Box::new(NativeDialogs::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_with(value: Option<&'static str>) -> impl Fn(&str) -> Option<String> {
        move |name| {
            (name == "DESAFINADO_DEV_PICK_FILE")
                .then(|| value.map(str::to_owned))
                .flatten()
        }
    }

    #[test]
    fn is_off_without_the_switch() {
        assert!(DevDialogs::from_env(&env_with(None)).is_none());
        assert!(DevDialogs::from_env(&env_with(Some("  "))).is_none());
    }

    #[test]
    fn refuses_a_relative_path_for_the_switch() {
        assert!(DevDialogs::from_env(&env_with(Some("pick.txt"))).is_none());
    }

    #[test]
    fn answers_every_dialog_with_the_path_in_the_file_read_when_the_dialog_opens() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pick.txt");
        std::fs::write(&file, "/a/one.md\n").unwrap();
        let spec = file.to_string_lossy().into_owned();
        let dialogs = DevDialogs::from_env(&|_| Some(spec.clone())).unwrap();
        assert_eq!(dialogs.pick_file(), Some(PathBuf::from("/a/one.md")));
        std::fs::write(&file, "/b/two.md").unwrap();
        assert_eq!(
            dialogs.pick_save_path("x.md"),
            Some(PathBuf::from("/b/two.md"))
        );
        assert_eq!(dialogs.pick_folder(), Some(PathBuf::from("/b/two.md")));
    }

    #[test]
    fn cancels_when_the_file_is_empty_or_cannot_be_read() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pick.txt");
        let spec = file.to_string_lossy().into_owned();
        let dialogs = DevDialogs::from_env(&|_| Some(spec.clone())).unwrap();
        assert_eq!(dialogs.pick_file(), None);
        std::fs::write(&file, "  \n").unwrap();
        assert_eq!(dialogs.pick_file(), None);
    }
}
