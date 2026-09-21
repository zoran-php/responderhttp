// http_client/src-tauri/src/secrets/envelope.rs
//
// The sealed format:
//
//   version (1) ‖ key_id (16) ‖ nonce (12) ‖ ciphertext ‖ tag (16)
//
// AES-256-GCM through aws-lc's RandomizedNonceKey: the library draws a fresh
// nonce for every seal, so nothing here ever chooses one. The scope is the
// associated data, which binds a value to the row and field it was sealed
// for (domain::secrets::secret_scope).
use aws_lc_rs::aead::{Aad, Nonce, RandomizedNonceKey, AES_256_GCM, NONCE_LEN};

use crate::domain::error::AppError;
use crate::domain::ports::{OpenedSecret, SecretCipher};
use crate::domain::secrets::SecretState;
use crate::secrets::data_key::{DataKey, KEY_ID_LEN};

/// Bumped only together with `domain::secrets::SCOPE_PREFIX`'s version.
const FORMAT_V1: u8 = 1;
const TAG_LEN: usize = 16;
const HEADER_LEN: usize = 1 + KEY_ID_LEN + NONCE_LEN;

pub struct EnvelopeCipher {
    key_id: [u8; KEY_ID_LEN],
    key: RandomizedNonceKey,
}

impl EnvelopeCipher {
    pub fn new(data_key: &DataKey) -> Result<Self, AppError> {
        let key = RandomizedNonceKey::new(&AES_256_GCM, data_key.key_bytes())
            .map_err(|_| AppError::SecretStore("the data key was rejected".into()))?;
        Ok(Self {
            key_id: *data_key.id(),
            key,
        })
    }
}

impl SecretCipher for EnvelopeCipher {
    fn seal(&self, scope: &str, plaintext: &str) -> Result<Vec<u8>, AppError> {
        let mut in_out = plaintext.as_bytes().to_vec();
        let nonce = self
            .key
            .seal_in_place_append_tag(Aad::from(scope.as_bytes()), &mut in_out)
            .map_err(|_| AppError::SecretStore("a secret could not be encrypted".into()))?;

        let mut sealed = Vec::with_capacity(HEADER_LEN + in_out.len());
        sealed.push(FORMAT_V1);
        sealed.extend_from_slice(&self.key_id);
        sealed.extend_from_slice(nonce.as_ref());
        sealed.extend_from_slice(&in_out);
        Ok(sealed)
    }

    fn open(&self, scope: &str, sealed: &[u8]) -> OpenedSecret {
        if sealed.len() < HEADER_LEN + TAG_LEN {
            return OpenedSecret::Lost(SecretState::NeedsReentry);
        }
        let (version, rest) = sealed.split_at(1);
        // A format this build does not know was written by a newer one. The
        // field loads empty rather than failing its whole row; the version
        // byte is what would let a later build read it again.
        if version != [FORMAT_V1] {
            return OpenedSecret::Lost(SecretState::NeedsReentry);
        }
        let (key_id, rest) = rest.split_at(KEY_ID_LEN);
        // Sealed under a key this machine no longer has. Trying it against
        // the current key could only fail, so it is not attempted.
        if key_id != self.key_id {
            return OpenedSecret::Lost(SecretState::NeedsReentry);
        }
        let (nonce, ciphertext) = rest.split_at(NONCE_LEN);
        let Ok(nonce) = Nonce::try_assume_unique_for_key(nonce) else {
            return OpenedSecret::Lost(SecretState::NeedsReentry);
        };

        let mut in_out = ciphertext.to_vec();
        // An authentication failure under the current key means the value
        // was altered, or moved from another row. Either way it is not this
        // field's secret, and nothing about it is worth showing.
        match self
            .key
            .open_in_place(nonce, Aad::from(scope.as_bytes()), &mut in_out)
        {
            Ok(plaintext) => match std::str::from_utf8(plaintext) {
                Ok(text) => OpenedSecret::Plain(text.to_string()),
                Err(_) => OpenedSecret::Lost(SecretState::NeedsReentry),
            },
            Err(_) => OpenedSecret::Lost(SecretState::NeedsReentry),
        }
    }

    fn ensure_available(&self) -> Result<(), AppError> {
        Ok(())
    }
}

/// The session's cipher when no data key could be loaded. Refuses to seal,
/// and reports every stored secret as unavailable rather than lost: the key
/// may well be readable again after a restart.
pub struct UnavailableCipher {
    reason: String,
}

impl UnavailableCipher {
    pub fn new(reason: String) -> Self {
        Self { reason }
    }

    fn error(&self) -> AppError {
        AppError::SecretStore(format!(
            "secrets cannot be saved this session: {}",
            self.reason
        ))
    }
}

impl SecretCipher for UnavailableCipher {
    fn seal(&self, _scope: &str, _plaintext: &str) -> Result<Vec<u8>, AppError> {
        Err(self.error())
    }

    fn open(&self, _scope: &str, _sealed: &[u8]) -> OpenedSecret {
        OpenedSecret::Lost(SecretState::Unavailable)
    }

    fn ensure_available(&self) -> Result<(), AppError> {
        Err(self.error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCOPE: &str = "responderhttp/v1/requests/req_1/auth.token";

    fn cipher() -> EnvelopeCipher {
        EnvelopeCipher::new(&DataKey::generate().expect("should generate")).expect("should build")
    }

    #[test]
    fn round_trips_a_secret() {
        let cipher = cipher();

        let sealed = cipher
            .seal(SCOPE, "eyJhbGciOi.secret")
            .expect("should seal");

        assert_eq!(
            cipher.open(SCOPE, &sealed),
            OpenedSecret::Plain("eyJhbGciOi.secret".into())
        );
    }

    #[test]
    fn round_trips_a_value_far_past_the_credential_manager_limit() {
        let cipher = cipher();
        let long = "x".repeat(64 * 1024);

        let sealed = cipher.seal(SCOPE, &long).expect("should seal");

        assert_eq!(cipher.open(SCOPE, &sealed), OpenedSecret::Plain(long));
    }

    #[test]
    fn has_the_documented_layout() {
        let data_key = DataKey::generate().expect("should generate");
        let cipher = EnvelopeCipher::new(&data_key).expect("should build");

        let sealed = cipher.seal(SCOPE, "abc").expect("should seal");

        assert_eq!(sealed.len(), 1 + KEY_ID_LEN + NONCE_LEN + 3 + TAG_LEN);
        assert_eq!(sealed[0], FORMAT_V1);
        assert_eq!(&sealed[1..1 + KEY_ID_LEN], data_key.id());
    }

    #[test]
    fn never_contains_the_plaintext() {
        let cipher = cipher();

        let sealed = cipher
            .seal(SCOPE, "plaintext-marker-1234567890")
            .expect("should seal");

        assert!(!sealed
            .windows("plaintext-marker".len())
            .any(|window| window == b"plaintext-marker"));
    }

    #[test]
    fn sealing_the_same_value_twice_gives_different_bytes() {
        let cipher = cipher();

        let first = cipher.seal(SCOPE, "same").expect("should seal");
        let second = cipher.seal(SCOPE, "same").expect("should seal");

        assert_ne!(first, second);
    }

    #[test]
    fn does_not_open_under_another_scope() {
        let cipher = cipher();
        let sealed = cipher.seal(SCOPE, "secret").expect("should seal");

        assert_eq!(
            cipher.open("responderhttp/v1/requests/req_2/auth.token", &sealed),
            OpenedSecret::Lost(SecretState::NeedsReentry)
        );
    }

    #[test]
    fn detects_a_flipped_byte() {
        let cipher = cipher();
        let mut sealed = cipher.seal(SCOPE, "secret").expect("should seal");
        let last = sealed.len() - 1;
        sealed[last] ^= 0x01;

        assert_eq!(
            cipher.open(SCOPE, &sealed),
            OpenedSecret::Lost(SecretState::NeedsReentry)
        );
    }

    #[test]
    fn does_not_open_under_a_different_key() {
        let sealed = cipher().seal(SCOPE, "secret").expect("should seal");

        assert_eq!(
            cipher().open(SCOPE, &sealed),
            OpenedSecret::Lost(SecretState::NeedsReentry)
        );
    }

    #[test]
    fn treats_an_unknown_version_as_lost() {
        let cipher = cipher();
        let mut sealed = cipher.seal(SCOPE, "secret").expect("should seal");
        sealed[0] = 2;

        assert_eq!(
            cipher.open(SCOPE, &sealed),
            OpenedSecret::Lost(SecretState::NeedsReentry)
        );
    }

    #[test]
    fn treats_a_truncated_value_as_lost() {
        let cipher = cipher();

        assert_eq!(
            cipher.open(SCOPE, &[FORMAT_V1, 0, 0]),
            OpenedSecret::Lost(SecretState::NeedsReentry)
        );
        assert_eq!(
            cipher.open(SCOPE, &[]),
            OpenedSecret::Lost(SecretState::NeedsReentry)
        );
    }

    #[test]
    fn an_unavailable_cipher_refuses_to_seal_and_says_why() {
        let cipher = UnavailableCipher::new("access denied".into());

        let error = cipher.seal(SCOPE, "secret").expect_err("must refuse");

        assert!(
            matches!(&error, AppError::SecretStore(message) if message.contains("access denied"))
        );
        assert!(cipher.ensure_available().is_err());
    }
}
