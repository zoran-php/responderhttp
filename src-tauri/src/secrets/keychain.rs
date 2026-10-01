// http_client/src-tauri/src/secrets/keychain.rs
//
// The data key's home in the OS credential store. On Windows it is one
// generic credential in Credential Manager, named KEYCHAIN_TARGET. On Linux
// it is one item found by `linux_attributes`, kept by oo7 (PLAN-LINUX.md 17c):
// in Secret Service (GNOME Keyring, KWallet) on the host, and inside a
// Flatpak in an encrypted keyring file whose key comes from the Secret
// portal. macOS is still deferred and reports the store as unavailable,
// which leaves secrets unsaveable there rather than silently stored in plain
// text.
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
//
// The store is named after the identifier the app is running under, passed
// in from lib.rs, rather than a constant (PLAN-LINUX.md, D1 as revised
// 2026-09-30). Every build uses KEYCHAIN_SERVICE except the Flatpak, which
// Flathub compiles with io.github.zoran_php.responderhttp.
use crate::domain::error::AppError;
use crate::domain::ports::DataKeyStore;

/// The app's identifier, as in tauri.conf.json. What every build but the
/// Flatpak passes to `KeychainDataKeyStore::new`; the tests below tie it to
/// the config and the uninstall hook.
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
    /// `app_identifier` is always KEYCHAIN_SERVICE on Windows, the name the
    /// uninstall hook deletes.
    pub fn new(app_identifier: &str) -> Result<Self, AppError> {
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
            .build(app_identifier, KEYCHAIN_USER, None)
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

/// How the item is found. "service" and "username" are the attribute names
/// libsecret-based tools use, so `secret-tool lookup service <identifier>
/// username data-key` finds the same item.
pub fn linux_attributes(app_identifier: &str) -> [(&str, &str); 2] {
    [("service", app_identifier), ("username", KEYCHAIN_USER)]
}

/// What a keyring manager such as Seahorse shows for the item.
pub const LINUX_LABEL: &str = "ResponderHTTP data key";

#[cfg(target_os = "linux")]
pub struct KeychainDataKeyStore {
    keyring: oo7::Keyring,
    app_identifier: String,
}

#[cfg(target_os = "linux")]
impl KeychainDataKeyStore {
    /// Fails when there is neither a Secret portal (in a Flatpak) nor a
    /// Secret Service on the session bus: secrets are then unavailable for
    /// the session, as when Credential Manager cannot be reached on Windows.
    pub fn new(app_identifier: &str) -> Result<Self, AppError> {
        let keyring = block_on(oo7::Keyring::new())?.map_err(describe)?;
        Ok(Self {
            keyring,
            app_identifier: app_identifier.to_string(),
        })
    }
}

#[cfg(target_os = "linux")]
impl DataKeyStore for KeychainDataKeyStore {
    fn load(&self) -> Result<Option<zeroize::Zeroizing<Vec<u8>>>, AppError> {
        block_on(async {
            // A locked collection lists its items but will not hand over a
            // secret. Unlocking one may show the desktop's own prompt.
            self.keyring.unlock().await.map_err(describe)?;
            let items = self
                .keyring
                .search_items(&linux_attributes(&self.app_identifier))
                .await
                .map_err(describe)?;
            match items.as_slice() {
                // The only answer that allows a new key to be created.
                [] => Ok(None),
                [item] => {
                    let secret = item.secret().await.map_err(describe)?;
                    Ok(Some(zeroize::Zeroizing::new(secret.to_vec())))
                }
                _ => Err(AppError::SecretStore(
                    "more than one keyring item matches the data key".to_string(),
                )),
            }
        })?
    }

    fn store(&self, key: &[u8]) -> Result<(), AppError> {
        block_on(async {
            self.keyring.unlock().await.map_err(describe)?;
            self.keyring
                .create_item(
                    LINUX_LABEL,
                    &linux_attributes(&self.app_identifier),
                    key,
                    true,
                )
                .await
                .map_err(describe)
        })?
    }
}

/// Runs one of oo7's futures to completion from synchronous code. It runs on
/// a helper thread inside Tauri's tokio runtime, which oo7's zbus connection
/// needs and keeps its background tasks on, so this works whether or not
/// the caller is already inside a runtime (block_on there would panic).
#[cfg(target_os = "linux")]
fn block_on<T: Send>(future: impl std::future::Future<Output = T> + Send) -> Result<T, AppError> {
    std::thread::scope(|scope| {
        scope
            .spawn(|| tauri::async_runtime::block_on(future))
            .join()
            .map_err(|_| AppError::SecretStore("the keyring call panicked".to_string()))
    })
}

/// oo7's errors name the backend and the D-Bus or file failure. None of them
/// carries secret bytes, but the text is still prefixed so a log line says
/// which store it came from.
#[cfg(target_os = "linux")]
fn describe(error: oo7::Error) -> AppError {
    AppError::SecretStore(format!("the system keyring failed: {error}"))
}

#[cfg(not(any(windows, target_os = "linux")))]
pub struct KeychainDataKeyStore;

#[cfg(not(any(windows, target_os = "linux")))]
impl KeychainDataKeyStore {
    pub fn new(_app_identifier: &str) -> Result<Self, AppError> {
        Err(AppError::SecretStore(
            "no OS credential store is wired up for this platform yet".into(),
        ))
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
impl DataKeyStore for KeychainDataKeyStore {
    fn load(&self) -> Result<Option<zeroize::Zeroizing<Vec<u8>>>, AppError> {
        Self::new(KEYCHAIN_SERVICE).map(|_| None)
    }

    fn store(&self, _key: &[u8]) -> Result<(), AppError> {
        Self::new(KEYCHAIN_SERVICE).map(|_| ())
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

    /// The Linux item is found by the identifier the app runs under, so a
    /// Flatpak build and an RPM keep separate keys, as they keep separate
    /// databases.
    #[test]
    fn the_linux_item_is_named_after_the_running_identifier() {
        assert_eq!(
            linux_attributes("io.example.app"),
            [("service", "io.example.app"), ("username", KEYCHAIN_USER)]
        );
    }

    /// The store refuses a service containing the divider, so the divider
    /// must not appear in it.
    #[test]
    fn the_divider_never_appears_in_the_service() {
        assert!(!KEYCHAIN_SERVICE.contains('@'));
    }
}
