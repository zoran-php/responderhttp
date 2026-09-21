// http_client/src-tauri/src/secrets/mod.rs
//
// Encryption at rest (PLAN.md Phase 9): one data key in the OS credential
// store, every secret sealed with it before it reaches SQLite. Infrastructure
// beside http/ and persistence/; the domain sees only the SecretCipher and
// DataKeyStore ports.
pub mod data_key;
pub mod envelope;
pub mod keychain;
pub mod memory;

use std::sync::Arc;

use crate::domain::ports::{DataKeyStore, SecretCipher};
use crate::secrets::data_key::DataKey;
use crate::secrets::envelope::{EnvelopeCipher, UnavailableCipher};

/// How the session's cipher came to be, for the startup log. Never carries
/// key material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CipherOrigin {
    ExistingKey,
    /// No key was stored, so one was created. Anything sealed before (under a
    /// key that has since gone) now opens as "needs re-entering".
    NewKey,
    /// No usable key this session. Secrets load as unavailable and saving one
    /// fails; nothing is ever written in plain text instead.
    Unavailable(String),
}

/// Loads the data key, creating it only when the store says there is none.
///
/// Any other failure — access denied, a store that is down, a stored key of
/// the wrong shape — leaves the session without a key rather than replacing
/// it: a replacement would make every secret sealed under the old key
/// unreadable, and the old key may be back after a restart.
pub fn open_cipher(store: &dyn DataKeyStore) -> (Arc<dyn SecretCipher>, CipherOrigin) {
    match load_or_create(store) {
        Ok((key, origin)) => match EnvelopeCipher::new(&key) {
            Ok(cipher) => (Arc::new(cipher), origin),
            Err(error) => without_key(error.to_string()),
        },
        Err(error) => without_key(error.to_string()),
    }
}

fn load_or_create(
    store: &dyn DataKeyStore,
) -> Result<(DataKey, CipherOrigin), crate::domain::error::AppError> {
    if let Some(bytes) = store.load()? {
        return Ok((DataKey::from_bytes(&bytes)?, CipherOrigin::ExistingKey));
    }
    let key = DataKey::generate()?;
    store.store(&key.to_bytes())?;
    Ok((key, CipherOrigin::NewKey))
}

/// The session's cipher when there is no store to load a key from at all.
pub fn without_key(reason: String) -> (Arc<dyn SecretCipher>, CipherOrigin) {
    (
        Arc::new(UnavailableCipher::new(reason.clone())),
        CipherOrigin::Unavailable(reason),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ports::OpenedSecret;
    use crate::domain::secrets::SecretState;
    use crate::secrets::memory::MemoryDataKeyStore;

    #[test]
    fn creates_and_stores_a_key_when_there_is_none() {
        let store = MemoryDataKeyStore::empty();

        let (cipher, origin) = open_cipher(&store);

        assert_eq!(origin, CipherOrigin::NewKey);
        assert!(store.stored().is_some());
        assert!(cipher.ensure_available().is_ok());
    }

    #[test]
    fn a_second_start_reuses_the_stored_key() {
        let store = MemoryDataKeyStore::empty();
        let (first, _) = open_cipher(&store);
        let sealed = first.seal("scope", "hunter2").expect("should seal");

        let (second, origin) = open_cipher(&store);

        assert_eq!(origin, CipherOrigin::ExistingKey);
        assert_eq!(
            second.open("scope", &sealed),
            OpenedSecret::Plain("hunter2".into())
        );
    }

    #[test]
    fn a_store_that_fails_to_load_is_never_overwritten() {
        let store = MemoryDataKeyStore::failing_load("access denied");

        let (cipher, origin) = open_cipher(&store);

        assert!(matches!(origin, CipherOrigin::Unavailable(_)));
        assert!(
            store.stored().is_none(),
            "no replacement key may be written"
        );
        assert!(cipher.seal("scope", "x").is_err());
        assert_eq!(
            cipher.open("scope", b"anything"),
            OpenedSecret::Lost(SecretState::Unavailable)
        );
    }

    #[test]
    fn a_malformed_stored_key_is_left_alone() {
        let store = MemoryDataKeyStore::holding(vec![1, 2, 3]);

        let (_, origin) = open_cipher(&store);

        assert!(matches!(origin, CipherOrigin::Unavailable(_)));
        assert_eq!(store.stored().as_deref(), Some(&[1u8, 2, 3][..]));
    }

    #[test]
    fn a_store_that_refuses_the_new_key_leaves_the_session_without_one() {
        let store = MemoryDataKeyStore::failing_store("read-only");

        let (cipher, origin) = open_cipher(&store);

        assert!(matches!(origin, CipherOrigin::Unavailable(_)));
        assert!(cipher.ensure_available().is_err());
    }

    #[test]
    fn a_replaced_key_reports_old_secrets_as_needing_reentry() {
        let store = MemoryDataKeyStore::empty();
        let (old, _) = open_cipher(&store);
        let sealed = old.seal("scope", "hunter2").expect("should seal");
        store.forget();

        let (new, origin) = open_cipher(&store);

        assert_eq!(origin, CipherOrigin::NewKey);
        assert_eq!(
            new.open("scope", &sealed),
            OpenedSecret::Lost(SecretState::NeedsReentry)
        );
    }
}
