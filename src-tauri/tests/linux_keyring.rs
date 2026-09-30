// http_client/src-tauri/tests/linux_keyring.rs
//
// The Linux data-key store against a real Secret Service (PLAN-LINUX.md 17c).
// Ignored by default: `cargo test` must never touch the keyring of the person
// running it, and CI has none. Run it with tools/test-linux-keyring.sh, which
// starts a private D-Bus session with a throwaway GNOME Keyring and a
// temporary data directory, so the real session keyring is never seen.
#![cfg(target_os = "linux")]

use responderhttp_lib::domain::ports::OpenedSecret;
use responderhttp_lib::secrets::keychain::KeychainDataKeyStore;
use responderhttp_lib::secrets::{open_cipher, CipherOrigin};

#[test]
#[ignore = "needs a Secret Service: run tools/test-linux-keyring.sh"]
fn the_data_key_is_created_once_and_read_back_by_the_next_start() {
    let first = KeychainDataKeyStore::new().expect("a Secret Service is reachable");
    let (cipher, origin) = open_cipher(&first);
    assert_eq!(
        origin,
        CipherOrigin::NewKey,
        "the keyring should start empty"
    );
    let sealed = cipher.seal("scope", "hunter2").expect("should seal");

    // A second store is what the next start of the app does.
    let second = KeychainDataKeyStore::new().expect("a Secret Service is reachable");
    let (cipher, origin) = open_cipher(&second);

    assert_eq!(origin, CipherOrigin::ExistingKey);
    assert_eq!(
        cipher.open("scope", &sealed),
        OpenedSecret::Plain("hunter2".into())
    );
}
