use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::base64;

pub const KEY_FILE_NAME: &str = "openrouter-key.json";

pub fn key_file_path(user_data_dir: &Path) -> PathBuf {
    user_data_dir.join(KEY_FILE_NAME)
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("keyring failure: {0}")]
pub struct KeyringError(pub String);

pub trait Keyring: Send + Sync {
    fn available(&self) -> bool;
    fn backend(&self) -> String;
    fn set(&self, secret: &str) -> Result<(), KeyringError>;
    fn get(&self) -> Result<Option<String>, KeyringError>;
    fn delete(&self) -> Result<(), KeyringError>;
}

pub trait KeyFile: Send + Sync {
    fn read(&self) -> io::Result<Option<String>>;
    fn write(&self, content: &str) -> io::Result<()>;
    fn remove(&self) -> io::Result<()>;
}

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
pub struct KeyDetails {
    pub present: bool,
    pub storage: KeyStorage,
    pub keyring_available: bool,
    pub keyring_stored: bool,
    pub backend: String,
    pub unreadable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum StoredFormat {
    Keyring,
    PlainText,
}

impl StoredFormat {
    fn as_storage(self) -> KeyStorage {
        match self {
            Self::Keyring => KeyStorage::Keyring,
            Self::PlainText => KeyStorage::PlainText,
        }
    }
}

#[derive(Debug, Serialize)]
struct StoredOut<'a> {
    version: u8,
    storage: StoredFormat,
    data: &'a str,
}

#[derive(Debug, Clone)]
struct Held {
    key: Option<String>,
    storage: KeyStorage,
    unreadable: bool,
}

impl Held {
    fn none() -> Self {
        Self {
            key: None,
            storage: KeyStorage::None,
            unreadable: false,
        }
    }

    fn unreadable() -> Self {
        Self {
            unreadable: true,
            ..Self::none()
        }
    }

    fn with(key: String, storage: KeyStorage) -> Self {
        Self {
            key: Some(key),
            storage,
            unreadable: false,
        }
    }
}

struct State {
    held: Held,
    on_disk: Option<StoredFormat>,
    keyring_available: bool,
}

pub struct KeyStore {
    keyring: Box<dyn Keyring>,
    file: Box<dyn KeyFile>,
    state: Mutex<State>,
}

const KEYRING_DATA: &str = "openrouter-key";

fn parse_stored(text: &str) -> Option<(StoredFormat, String)> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let object = value.as_object()?;
    if object.get("version")?.as_u64()? != 1 {
        return None;
    }
    let format = match object.get("storage")?.as_str()? {
        "keyring" => StoredFormat::Keyring,
        "plain-text" => StoredFormat::PlainText,
        _ => return None,
    };
    Some((format, object.get("data")?.as_str()?.to_owned()))
}

fn serialize(format: StoredFormat, data: &str) -> String {
    serde_json::to_string(&StoredOut {
        version: 1,
        storage: format,
        data,
    })
    .unwrap_or_default()
}

impl KeyStore {
    pub fn new(keyring: Box<dyn Keyring>, file: Box<dyn KeyFile>) -> Self {
        Self {
            keyring,
            file,
            state: Mutex::new(State {
                held: Held::none(),
                on_disk: None,
                keyring_available: false,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn details(&self, state: &State) -> KeyDetails {
        KeyDetails {
            present: state.held.key.is_some(),
            storage: state.held.storage,
            keyring_available: state.keyring_available,
            keyring_stored: state.on_disk == Some(StoredFormat::Keyring),
            backend: self.keyring.backend(),
            unreadable: state.held.unreadable,
        }
    }

    pub fn status(&self) -> KeyDetails {
        self.details(&self.lock())
    }

    pub fn current(&self) -> Option<String> {
        self.lock().held.key.clone()
    }

    pub fn load(&self) -> KeyDetails {
        let mut state = self.lock();
        state.keyring_available = self.keyring.available();
        let text = self.file.read();
        let stored = match &text {
            Ok(Some(text)) => parse_stored(text),
            _ => None,
        };
        state.on_disk = stored.as_ref().map(|(format, _)| *format);
        state.held = match (&text, stored) {
            (Ok(None), _) => Held::none(),
            (Err(_), _) | (Ok(Some(_)), None) => Held::unreadable(),
            (Ok(Some(_)), Some((format, data))) => self.decrypt(&state, format, &data),
        };
        self.details(&state)
    }

    fn decrypt(&self, state: &State, format: StoredFormat, data: &str) -> Held {
        let key = match format {
            StoredFormat::PlainText => {
                base64::decode(data).and_then(|bytes| String::from_utf8(bytes).ok())
            }
            StoredFormat::Keyring => {
                if !state.keyring_available {
                    return Held::unreadable();
                }
                self.keyring.get().ok().flatten()
            }
        };
        match key {
            Some(key) if !key.is_empty() => Held::with(key, format.as_storage()),
            _ => Held::unreadable(),
        }
    }

    fn persist(&self, state: &mut State, key: &str, format: StoredFormat) -> Option<Held> {
        let data = match format {
            StoredFormat::Keyring => {
                self.keyring.set(key).ok()?;
                KEYRING_DATA.to_owned()
            }
            StoredFormat::PlainText => base64::encode(key.as_bytes()),
        };
        if self.file.write(&serialize(format, &data)).is_err() {
            if format == StoredFormat::Keyring && state.on_disk != Some(StoredFormat::Keyring) {
                let _ = self.keyring.delete();
            }
            return None;
        }
        state.on_disk = Some(format);
        Some(Held::with(key.to_owned(), format.as_storage()))
    }

    fn stored_format(&self) -> Option<StoredFormat> {
        let text = self.file.read().ok().flatten()?;
        parse_stored(&text).map(|(format, _)| format)
    }

    fn keep_for_session(&self, state: &mut State, key: &str) -> Held {
        if self.stored_format() != Some(StoredFormat::Keyring) {
            let _ = self.file.remove();
            state.on_disk = None;
        }
        Held::with(key.to_owned(), KeyStorage::Session)
    }

    pub fn save(&self, key: &str) -> KeyDetails {
        let mut state = self.lock();
        state.keyring_available = self.keyring.available();
        let persisted = if state.keyring_available {
            self.persist(&mut state, key, StoredFormat::Keyring)
        } else {
            None
        };
        state.held = match persisted {
            Some(held) => held,
            None => self.keep_for_session(&mut state, key),
        };
        self.details(&state)
    }

    pub fn keep_as_plain_text(&self) -> KeyDetails {
        let mut state = self.lock();
        if let (Some(key), KeyStorage::Session) = (state.held.key.clone(), state.held.storage)
            && let Some(held) = self.persist(&mut state, &key, StoredFormat::PlainText)
        {
            state.held = held;
        }
        self.details(&state)
    }

    pub fn clear(&self) -> KeyDetails {
        let mut state = self.lock();
        let _ = self.file.remove();
        let _ = self.keyring.delete();
        state.on_disk = None;
        state.held = Held::none();
        self.details(&state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(Default)]
    struct FakeKeyringState {
        available: bool,
        secret: Option<String>,
        fail_set: bool,
        sets: u32,
    }

    #[derive(Clone)]
    struct FakeKeyring(Arc<Mutex<FakeKeyringState>>);

    impl FakeKeyring {
        fn new(available: bool) -> Self {
            Self(Arc::new(Mutex::new(FakeKeyringState {
                available,
                ..Default::default()
            })))
        }
        fn with<T>(&self, f: impl FnOnce(&mut FakeKeyringState) -> T) -> T {
            f(&mut self.0.lock().unwrap())
        }
    }

    impl Keyring for FakeKeyring {
        fn available(&self) -> bool {
            self.with(|s| s.available)
        }
        fn backend(&self) -> String {
            "fake".into()
        }
        fn set(&self, secret: &str) -> Result<(), KeyringError> {
            self.with(|s| {
                if s.fail_set || !s.available {
                    return Err(KeyringError("no".into()));
                }
                s.secret = Some(secret.to_owned());
                s.sets += 1;
                Ok(())
            })
        }
        fn get(&self) -> Result<Option<String>, KeyringError> {
            self.with(|s| {
                if s.available {
                    Ok(s.secret.clone())
                } else {
                    Err(KeyringError("locked".into()))
                }
            })
        }
        fn delete(&self) -> Result<(), KeyringError> {
            self.with(|s| {
                s.secret = None;
                Ok(())
            })
        }
    }

    #[derive(Default)]
    struct FileState {
        content: Option<String>,
        fail_read: bool,
        fail_write: bool,
    }

    #[derive(Clone, Default)]
    struct FakeFile(Arc<Mutex<FileState>>);

    impl FakeFile {
        fn with<T>(&self, f: impl FnOnce(&mut FileState) -> T) -> T {
            f(&mut self.0.lock().unwrap())
        }
        fn content(&self) -> Option<String> {
            self.with(|s| s.content.clone())
        }
    }

    impl KeyFile for FakeFile {
        fn read(&self) -> io::Result<Option<String>> {
            self.with(|s| {
                if s.fail_read {
                    Err(io::Error::from(io::ErrorKind::PermissionDenied))
                } else {
                    Ok(s.content.clone())
                }
            })
        }
        fn write(&self, content: &str) -> io::Result<()> {
            self.with(|s| {
                if s.fail_write {
                    return Err(io::Error::from(io::ErrorKind::PermissionDenied));
                }
                s.content = Some(content.to_owned());
                Ok(())
            })
        }
        fn remove(&self) -> io::Result<()> {
            self.with(|s| {
                s.content = None;
                Ok(())
            })
        }
    }

    fn store(keyring: &FakeKeyring, file: &FakeFile) -> KeyStore {
        KeyStore::new(Box::new(keyring.clone()), Box::new(file.clone()))
    }

    #[test]
    fn saves_to_the_keyring_and_writes_a_marker_file() {
        let (keyring, file) = (FakeKeyring::new(true), FakeFile::default());
        let store = store(&keyring, &file);
        let details = store.save("sk-1");
        assert_eq!(details.storage, KeyStorage::Keyring);
        assert!(details.present && details.keyring_stored && details.keyring_available);
        assert_eq!(store.current().as_deref(), Some("sk-1"));
        assert_eq!(keyring.with(|s| s.secret.clone()).as_deref(), Some("sk-1"));
        let written = file.content().unwrap();
        assert!(!written.contains("sk-1"));
        assert_eq!(parse_stored(&written).unwrap().0, StoredFormat::Keyring);
    }

    #[test]
    fn keeps_the_key_for_the_session_and_writes_nothing_when_no_keyring_is_available() {
        let (keyring, file) = (FakeKeyring::new(false), FakeFile::default());
        let store = store(&keyring, &file);
        let details = store.save("sk-1");
        assert_eq!(details.storage, KeyStorage::Session);
        assert!(details.present && !details.keyring_available && !details.keyring_stored);
        assert_eq!(file.content(), None);
    }

    #[test]
    fn removes_an_older_plain_text_key_when_the_new_one_cannot_be_stored() {
        let (keyring, file) = (FakeKeyring::new(false), FakeFile::default());
        let store = store(&keyring, &file);
        store.save("old");
        store.keep_as_plain_text();
        assert!(file.content().is_some());
        store.save("new");
        assert_eq!(file.content(), None);
        assert_eq!(store.current().as_deref(), Some("new"));
    }

    #[test]
    fn keeps_a_stored_keyring_file_when_the_keyring_is_unavailable_and_a_key_is_saved() {
        let (keyring, file) = (FakeKeyring::new(true), FakeFile::default());
        store(&keyring, &file).save("stored");
        let before = file.content();
        keyring.with(|s| s.available = false);
        let fresh = store(&keyring, &file);
        fresh.load();
        let details = fresh.save("typed");
        assert_eq!(details.storage, KeyStorage::Session);
        assert!(details.keyring_stored);
        assert_eq!(file.content(), before);
    }

    #[test]
    fn keeps_a_stored_keyring_file_when_storing_fails_and_the_key_falls_back_to_the_session() {
        let (keyring, file) = (FakeKeyring::new(true), FakeFile::default());
        store(&keyring, &file).save("stored");
        let before = file.content();
        keyring.with(|s| s.fail_set = true);
        let fresh = store(&keyring, &file);
        let details = fresh.save("typed");
        assert_eq!(details.storage, KeyStorage::Session);
        assert_eq!(file.content(), before);
    }

    #[test]
    fn still_lets_remove_key_delete_a_keyring_file_and_entry() {
        let (keyring, file) = (FakeKeyring::new(true), FakeFile::default());
        let store = store(&keyring, &file);
        store.save("k");
        keyring.with(|s| s.available = false);
        let details = store.clear();
        assert_eq!(file.content(), None);
        assert!(!details.present && !details.keyring_stored);
        assert_eq!(details.storage, KeyStorage::None);
        assert_eq!(keyring.with(|s| s.secret.clone()), None);
    }

    #[test]
    fn falls_back_to_the_session_when_the_keyring_fails_while_storing() {
        let (keyring, file) = (FakeKeyring::new(true), FakeFile::default());
        keyring.with(|s| s.fail_set = true);
        let store = store(&keyring, &file);
        let details = store.save("k");
        assert_eq!(details.storage, KeyStorage::Session);
        assert_eq!(file.content(), None);
    }

    #[test]
    fn falls_back_when_the_file_cannot_be_written_and_leaves_no_secret_behind() {
        let (keyring, file) = (FakeKeyring::new(true), FakeFile::default());
        file.with(|s| s.fail_write = true);
        let store = store(&keyring, &file);
        assert_eq!(store.save("k").storage, KeyStorage::Session);
        assert_eq!(keyring.with(|s| s.secret.clone()), None);
    }

    #[test]
    fn replaces_the_previous_key() {
        let (keyring, file) = (FakeKeyring::new(true), FakeFile::default());
        let store = store(&keyring, &file);
        store.save("one");
        store.save("two");
        assert_eq!(store.current().as_deref(), Some("two"));
        assert_eq!(keyring.with(|s| s.secret.clone()).as_deref(), Some("two"));
    }

    #[test]
    fn stores_the_session_key_as_plain_text_only_when_asked() {
        let (keyring, file) = (FakeKeyring::new(false), FakeFile::default());
        let store = store(&keyring, &file);
        store.save("sk-plain");
        assert_eq!(file.content(), None);
        let details = store.keep_as_plain_text();
        assert_eq!(details.storage, KeyStorage::PlainText);
        assert!(!details.keyring_stored);
        let reloaded = super::KeyStore::new(Box::new(keyring.clone()), Box::new(file.clone()));
        let loaded = reloaded.load();
        assert_eq!(loaded.storage, KeyStorage::PlainText);
        assert_eq!(reloaded.current().as_deref(), Some("sk-plain"));
    }

    #[test]
    fn keep_as_plain_text_does_nothing_without_a_session_key_or_when_in_the_keyring() {
        let (keyring, file) = (FakeKeyring::new(true), FakeFile::default());
        let store = store(&keyring, &file);
        assert_eq!(store.keep_as_plain_text().storage, KeyStorage::None);
        assert_eq!(file.content(), None);
        store.save("k");
        let before = file.content();
        assert_eq!(store.keep_as_plain_text().storage, KeyStorage::Keyring);
        assert_eq!(file.content(), before);
    }

    #[test]
    fn reads_a_stored_key_back() {
        let (keyring, file) = (FakeKeyring::new(true), FakeFile::default());
        store(&keyring, &file).save("sk-stored");
        let fresh = store(&keyring, &file);
        let details = fresh.load();
        assert!(details.present && details.keyring_stored && !details.unreadable);
        assert_eq!(fresh.current().as_deref(), Some("sk-stored"));
    }

    #[test]
    fn has_no_key_when_nothing_is_stored() {
        let (keyring, file) = (FakeKeyring::new(true), FakeFile::default());
        let store = store(&keyring, &file);
        let details = store.load();
        assert!(!details.present && !details.unreadable && !details.keyring_stored);
        assert_eq!(details.storage, KeyStorage::None);
        assert_eq!(store.current(), None);
    }

    #[test]
    fn reports_an_unreadable_key_and_leaves_the_file_alone_when_the_keyring_is_gone() {
        let (keyring, file) = (FakeKeyring::new(true), FakeFile::default());
        store(&keyring, &file).save("k");
        let before = file.content();
        keyring.with(|s| s.available = false);
        let fresh = store(&keyring, &file);
        let details = fresh.load();
        assert!(details.unreadable && !details.present && details.keyring_stored);
        assert_eq!(file.content(), before);
    }

    #[test]
    fn reports_an_unreadable_key_when_the_keyring_lost_the_entry() {
        let (keyring, file) = (FakeKeyring::new(true), FakeFile::default());
        store(&keyring, &file).save("k");
        keyring.with(|s| s.secret = None);
        assert!(store(&keyring, &file).load().unreadable);
    }

    #[test]
    fn reports_an_unreadable_key_for_a_corrupt_file_or_one_with_the_wrong_shape() {
        for content in [
            "not json",
            "[]",
            r#"{"version":2,"storage":"keyring","data":"x"}"#,
            r#"{"version":1,"storage":"other","data":"x"}"#,
            r#"{"version":1,"storage":"keyring","data":1}"#,
        ] {
            let (keyring, file) = (FakeKeyring::new(true), FakeFile::default());
            file.with(|s| s.content = Some(content.into()));
            let fresh = store(&keyring, &file);
            let details = fresh.load();
            assert!(details.unreadable && !details.present, "{content}");
            assert!(!details.keyring_stored);
            assert_eq!(file.content().as_deref(), Some(content));
        }
    }

    #[test]
    fn reports_an_unreadable_key_when_the_file_cannot_be_read() {
        let (keyring, file) = (FakeKeyring::new(true), FakeFile::default());
        file.with(|s| s.fail_read = true);
        assert!(store(&keyring, &file).load().unreadable);
    }

    #[test]
    fn clears_the_unreadable_flag_once_a_new_key_is_saved() {
        let (keyring, file) = (FakeKeyring::new(true), FakeFile::default());
        file.with(|s| s.content = Some("garbage".into()));
        let store = store(&keyring, &file);
        assert!(store.load().unreadable);
        let details = store.save("fresh");
        assert!(!details.unreadable && details.present);
    }

    #[test]
    fn never_carries_the_key_in_its_status() {
        let (keyring, file) = (FakeKeyring::new(true), FakeFile::default());
        let store = store(&keyring, &file);
        store.save("sk-very-secret");
        let json = serde_json::to_string(&store.status()).unwrap();
        assert!(!json.contains("sk-very-secret"));
        assert!(json.contains("keyringStored"));
    }

    #[test]
    fn keyring_stored_is_false_for_a_plain_text_file_and_turns_false_when_plain_text_replaces_the_keyring_file()
     {
        let (keyring, file) = (FakeKeyring::new(false), FakeFile::default());
        let store = store(&keyring, &file);
        store.save("k");
        store.keep_as_plain_text();
        assert!(!store.status().keyring_stored);
    }
}
