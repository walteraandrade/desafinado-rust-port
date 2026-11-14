use super::store::{Keyring, KeyringError};

const SERVICE: &str = "desafinado-native";
const ACCOUNT: &str = "openrouter-key";
const BACKEND: &str = "secret-service";

#[derive(Debug, Clone, Copy, Default)]
pub struct SecretServiceKeyring;

fn entry() -> Result<keyring::Entry, KeyringError> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| KeyringError(e.to_string()))
}

impl Keyring for SecretServiceKeyring {
    fn available(&self) -> bool {
        matches!(
            entry().map(|e| e.get_password()),
            Ok(Ok(_)) | Ok(Err(keyring::Error::NoEntry))
        )
    }

    fn backend(&self) -> String {
        BACKEND.to_owned()
    }

    fn set(&self, secret: &str) -> Result<(), KeyringError> {
        entry()?
            .set_password(secret)
            .map_err(|e| KeyringError(e.to_string()))
    }

    fn get(&self) -> Result<Option<String>, KeyringError> {
        match entry()?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(KeyringError(error.to_string())),
        }
    }

    fn delete(&self) -> Result<(), KeyringError> {
        match entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(KeyringError(error.to_string())),
        }
    }
}
