// http_client/src-tauri/src/secrets/memory.rs
//
// A DataKeyStore that lives in memory. Tests use it so that nothing in the
// suite ever touches the real credential store (CLAUDE.md section 8), and it
// can be told to fail in the ways the real one can.
use std::sync::Mutex;

use zeroize::Zeroizing;

use crate::domain::error::AppError;
use crate::domain::ports::DataKeyStore;

#[derive(Default)]
pub struct MemoryDataKeyStore {
    key: Mutex<Option<Vec<u8>>>,
    load_error: Option<String>,
    store_error: Option<String>,
}

impl MemoryDataKeyStore {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn holding(key: Vec<u8>) -> Self {
        Self {
            key: Mutex::new(Some(key)),
            ..Self::default()
        }
    }

    pub fn failing_load(reason: &str) -> Self {
        Self {
            load_error: Some(reason.to_string()),
            ..Self::default()
        }
    }

    pub fn failing_store(reason: &str) -> Self {
        Self {
            store_error: Some(reason.to_string()),
            ..Self::default()
        }
    }

    pub fn stored(&self) -> Option<Vec<u8>> {
        self.slot().clone()
    }

    /// What deleting the Credential Manager entry does.
    pub fn forget(&self) {
        *self.slot() = None;
    }

    fn slot(&self) -> std::sync::MutexGuard<'_, Option<Vec<u8>>> {
        self.key
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl DataKeyStore for MemoryDataKeyStore {
    fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, AppError> {
        if let Some(reason) = &self.load_error {
            return Err(AppError::SecretStore(reason.clone()));
        }
        Ok(self.slot().clone().map(Zeroizing::new))
    }

    fn store(&self, key: &[u8]) -> Result<(), AppError> {
        if let Some(reason) = &self.store_error {
            return Err(AppError::SecretStore(reason.clone()));
        }
        *self.slot() = Some(key.to_vec());
        Ok(())
    }
}
