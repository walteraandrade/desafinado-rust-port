use std::{
    path::PathBuf,
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use desafinado_ai::records::AiCallRecord;

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
    pending: Mutex<BTreeMap<u64, AiCallRecord>>,
    next_pending: AtomicU64,
}

const MODEL_FILE: &str = "model.txt";

impl Services {
    pub fn hold_record(&self, record: AiCallRecord) -> u64 {
        let id = self.next_pending.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut pending) = self.pending.lock() {
            pending.insert(id, record);
        }
        id
    }

    pub fn complete_record(&self, id: u64, record: AiCallRecord) {
        let held = self
            .pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&id));
        if held.is_some() {
            let _ = self.log.append(&record);
        }
    }

    pub fn flush_records(&self) {
        let held = self
            .pending
            .lock()
            .map(|mut pending| std::mem::take(&mut *pending))
            .unwrap_or_default();
        for record in held.values() {
            let _ = self.log.append(record);
        }
    }

    pub fn load() -> Arc<Self> {
        let user_data = user_data_dir();
        let _ = std::fs::create_dir_all(&user_data);
        let keys = KeyStore::new(
            Box::new(SecretServiceKeyring),
            Box::new(FsKeyFile::new(key_file_path(&user_data))),
        );
        keys.load();
        let configured = std::fs::read_to_string(user_data.join(MODEL_FILE))
            .ok()
            .filter(|text| !text.trim().is_empty())
            .or_else(|| std::env::var("DESAFINADO_MODEL").ok());
        Arc::new(Self {
            log: AiCallLog::new(&user_data),
            model: Mutex::new(resolve_model(configured.as_deref())),
            pending: Mutex::new(BTreeMap::new()),
            next_pending: AtomicU64::new(0),
            keys,
            user_data,
        })
    }

    pub fn model(&self) -> String {
        self.model
            .lock()
            .map(|model| model.clone())
            .unwrap_or_default()
    }

    pub fn set_model(&self, model: &str) {
        let model = resolve_model(Some(model));
        let _ = std::fs::write(self.user_data.join(MODEL_FILE), &model);
        if let Ok(mut current) = self.model.lock() {
            *current = model;
        }
    }

    pub fn default_model(&self) -> String {
        resolve_model(std::env::var("DESAFINADO_MODEL").ok().as_deref())
    }

    pub fn model_choice(&self) -> Option<String> {
        std::fs::read_to_string(self.user_data.join(MODEL_FILE))
            .ok()
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
    }

    pub fn use_default_model(&self) {
        let _ = std::fs::remove_file(self.user_data.join(MODEL_FILE));
        if let Ok(mut current) = self.model.lock() {
            *current = self.default_model();
        }
    }
}
