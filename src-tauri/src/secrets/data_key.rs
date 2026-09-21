// http_client/src-tauri/src/secrets/data_key.rs
//
// The one key everything is sealed with, and its stored form:
// `key_id (16 bytes) ‖ key (32 bytes)`. The id travels inside every sealed
// value, so a replacement key is recognised as a different key rather than
// being tried against ciphertext it never produced.
use zeroize::Zeroizing;

use crate::domain::error::AppError;

pub const KEY_ID_LEN: usize = 16;
pub const KEY_LEN: usize = 32;
const STORED_LEN: usize = KEY_ID_LEN + KEY_LEN;

pub struct DataKey {
    id: [u8; KEY_ID_LEN],
    key: Zeroizing<[u8; KEY_LEN]>,
}

impl DataKey {
    /// From the OS random source, through aws-lc — never a userland PRNG.
    pub fn generate() -> Result<Self, AppError> {
        let mut id = [0u8; KEY_ID_LEN];
        let mut key = Zeroizing::new([0u8; KEY_LEN]);
        aws_lc_rs::rand::fill(&mut id).map_err(|_| random_failure())?;
        aws_lc_rs::rand::fill(key.as_mut()).map_err(|_| random_failure())?;
        Ok(Self { id, key })
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AppError> {
        if bytes.len() != STORED_LEN {
            return Err(AppError::SecretStore(format!(
                "the stored data key is {} bytes, expected {STORED_LEN}",
                bytes.len()
            )));
        }
        let (id_part, key_part) = bytes.split_at(KEY_ID_LEN);
        let mut id = [0u8; KEY_ID_LEN];
        id.copy_from_slice(id_part);
        let mut key = Zeroizing::new([0u8; KEY_LEN]);
        key.copy_from_slice(key_part);
        Ok(Self { id, key })
    }

    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        let mut bytes = Zeroizing::new(Vec::with_capacity(STORED_LEN));
        bytes.extend_from_slice(&self.id);
        bytes.extend_from_slice(self.key.as_ref());
        bytes
    }

    pub fn id(&self) -> &[u8; KEY_ID_LEN] {
        &self.id
    }

    pub fn key_bytes(&self) -> &[u8; KEY_LEN] {
        &self.key
    }
}

/// Deliberately says nothing but the type, so a stray `{:?}` can never put
/// key material in a log (CLAUDE.md section 11, rule 6).
impl std::fmt::Debug for DataKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("DataKey(..)")
    }
}

fn random_failure() -> AppError {
    AppError::SecretStore("the system random source failed".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_its_stored_form() {
        let key = DataKey::generate().expect("should generate");

        let restored = DataKey::from_bytes(&key.to_bytes()).expect("should parse");

        assert_eq!(restored.id(), key.id());
        assert_eq!(restored.key_bytes(), key.key_bytes());
    }

    #[test]
    fn two_generated_keys_differ() {
        let first = DataKey::generate().expect("should generate");
        let second = DataKey::generate().expect("should generate");

        assert_ne!(first.id(), second.id());
        assert_ne!(first.key_bytes(), second.key_bytes());
    }

    #[test]
    fn refuses_a_stored_key_of_the_wrong_length() {
        assert!(matches!(
            DataKey::from_bytes(&[0u8; 32]),
            Err(AppError::SecretStore(_))
        ));
    }

    #[test]
    fn debug_output_carries_no_key_material() {
        let key = DataKey::from_bytes(&[7u8; STORED_LEN]).expect("should parse");

        assert_eq!(format!("{key:?}"), "DataKey(..)");
    }
}
