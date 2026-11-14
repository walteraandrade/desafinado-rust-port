use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use desafinado_ai::default_model::resolve_model;
use desafinado_platform::{
    key::{FsKeyFile, KeyStore, SecretServiceKeyring, key_file_path},
    records::AiCallLog,
    user_data::user_data_dir,
};

pub struct Services {
    pub user_data: PathBuf,
    pub keys: KeyStore,
    pub log: AiCallLog,
    pub model: Mutex<String>,
}

const MODEL_FILE: &str = "model.txt";

impl Services {
    pub fn load() -> Arc<Self> {
        let user_data = user_data_dir();
        let _ = std::fs::create_dir_all(&user_data);
        let keys = KeyStore::new(
            Box::new(SecretServiceKeyring),
            Box::new(FsKeyFile::new(key_file_path(&user_data))),
        );
        keys.load();
        let configured = std::env::var("DESAFINADO_MODEL")
            .ok()
            .or_else(|| std::fs::read_to_string(user_data.join(MODEL_FILE)).ok());
        Arc::new(Self {
            log: AiCallLog::new(&user_data),
            model: Mutex::new(resolve_model(configured.as_deref())),
            keys,
            user_data,
        })
    }

    pub fn model(&self) -> String {
        self.model.lock().map(|model| model.clone()).unwrap_or_default()
    }

    pub fn set_model(&self, model: &str) {
        let model = resolve_model(Some(model));
        let _ = std::fs::write(self.user_data.join(MODEL_FILE), &model);
        if let Ok(mut current) = self.model.lock() {
            *current = model;
        }
    }
}
