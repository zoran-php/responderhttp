// http_client/src-tauri/src/secrets/keychain.rs
//
// The data key's home in the OS credential store. Windows only for now: on
// Windows it is one generic credential in Credential Manager, named
// KEYCHAIN_TARGET. macOS and Linux are deferred (PLAN.md Phase 0) and report
// the store as unavailable until they get one, which leaves secrets
// unsaveable there rather than silently stored in plain text.
//
// Decided 2026-09-16 (PLAN.md Phase 9, "Keychain entry"):
// - persistence is the store's default, Enterprise, so the key follows a
//   roaming profile the same way the app-data database does;
// - the delimiter is "@" and the store refuses a service name containing it,
//   so no two service/user pairs can land on the same credential;
// - dev and release builds share this one entry, as they share one database.
//
// Renamed 2026-09-18 (PLAN.md Phase 10): the identifier became
// io.github.zoran-php.responderhttp when the app moved to an individual
// publisher. Nothing is shipped yet, so no key is orphaned.
use crate::domain::error::AppError;
use crate::domain::ports::DataKeyStore;

/// The app's identifier, as in tauri.conf.json.
pub const KEYCHAIN_SERVICE: &str = "io.github.zoran-php.responderhttp";
pub const KEYCHAIN_USER: &str = "data-key";
/// What Credential Manager shows, and what the NSIS uninstall hook deletes
/// (windows/hooks.nsh). The store builds it as prefix + user + divider +
/// service + suffix; a test below keeps the three in step.
pub const KEYCHAIN_TARGET: &str = "data-key@io.github.zoran-php.responderhttp";

#[cfg(windows)]
pub struct KeychainDataKeyStore {
    entry: keyring_core::Entry,
}

#[cfg(windows)]
impl KeychainDataKeyStore {
    pub fn new() -> Result<Self, AppError> {
        use keyring_core::api::CredentialStoreApi;
        use std::collections::HashMap;

        // Prefix and suffix are spelled out: the crate's doc comment and its
        // code disagree about their defaults, and the uninstall hook depends
        // on the exact name.
        let config = HashMap::from([
            ("prefix", ""),
            ("divider", "@"),
            ("suffix", ""),
            ("service_no_divider", "true"),
        ]);
        let store = windows_native_keyring_store::Store::new_with_configuration(&config)
            .map_err(describe)?;
        let entry = store
            .build(KEYCHAIN_SERVICE, KEYCHAIN_USER, None)
            .map_err(describe)?;
        Ok(Self { entry })
    }
}

#[cfg(windows)]
impl DataKeyStore for KeychainDataKeyStore {
    fn load(&self) -> Result<Option<zeroize::Zeroizing<Vec<u8>>>, AppError> {
        match self.entry.get_secret() {
            Ok(bytes) => Ok(Some(zeroize::Zeroizing::new(bytes))),
            // The only answer that allows a new key to be created.
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(error) => Err(describe(error)),
        }
    }

    fn store(&self, key: &[u8]) -> Result<(), AppError> {
        self.entry.set_secret(key).map_err(describe)
    }
}

/// Written out per variant rather than using the crate's Display: some
/// variants carry the stored bytes, and none of that belongs in a message
/// that may be logged or shown.
#[cfg(windows)]
fn describe(error: keyring_core::Error) -> AppError {
    use keyring_core::Error;
    let message = match error {
        Error::NoStorageAccess(platform) => {
            format!("Credential Manager refused access: {platform}")
        }
        Error::PlatformFailure(platform) => format!("Credential Manager failed: {platform}"),
        Error::NoEntry => "the data key is missing from Credential Manager".to_string(),
        Error::BadEncoding(_) | Error::BadDataFormat(..) | Error::BadStoreFormat(_) => {
            "the data key in Credential Manager is malformed".to_string()
        }
        Error::Ambiguous(_) => {
            "more than one Credential Manager entry matches the data key".to_string()
        }
        Error::TooLong(name, limit) => {
            format!("the credential {name} is longer than Credential Manager allows ({limit})")
        }
        Error::Invalid(name, reason) => {
            format!("the credential {name} was rejected: {reason}")
        }
        _ => "Credential Manager returned an unexpected error".to_string(),
    };
    AppError::SecretStore(message)
}

#[cfg(not(windows))]
pub struct KeychainDataKeyStore;

#[cfg(not(windows))]
impl KeychainDataKeyStore {
    pub fn new() -> Result<Self, AppError> {
        Err(AppError::SecretStore(
            "no OS credential store is wired up for this platform yet".into(),
        ))
    }
}

#[cfg(not(windows))]
impl DataKeyStore for KeychainDataKeyStore {
    fn load(&self) -> Result<Option<zeroize::Zeroizing<Vec<u8>>>, AppError> {
        Self::new().map(|_| None)
    }

    fn store(&self, _key: &[u8]) -> Result<(), AppError> {
        Self::new().map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The store joins the parts as user + divider + service (prefix and
    /// suffix empty). If this drifts, the key is written under one name and
    /// the uninstaller deletes another.
    #[test]
    fn the_target_is_what_the_store_builds() {
        assert_eq!(
            format!("{KEYCHAIN_USER}@{KEYCHAIN_SERVICE}"),
            KEYCHAIN_TARGET
        );
    }

    #[test]
    fn the_uninstall_hook_deletes_this_exact_target() {
        let hook = include_str!("../../windows/hooks.nsh");

        assert!(
            hook.contains(&format!("cmdkey /delete:{KEYCHAIN_TARGET}")),
            "windows/hooks.nsh must delete {KEYCHAIN_TARGET}"
        );
    }

    #[test]
    fn the_service_is_the_app_identifier() {
        let config = include_str!("../../tauri.conf.json");

        assert!(config.contains(&format!("\"identifier\": \"{KEYCHAIN_SERVICE}\"")));
    }

    /// The store refuses a service containing the divider, so the divider
    /// must not appear in it.
    #[test]
    fn the_divider_never_appears_in_the_service() {
        assert!(!KEYCHAIN_SERVICE.contains('@'));
    }
}
