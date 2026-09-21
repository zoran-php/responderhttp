// http_client/src-tauri/src/domain/ports.rs
//
// Trait boundary between domain and infrastructure. Domain services depend
// on these; concrete implementations (CurlClient, SQLite repositories) are
// injected at startup in lib.rs.
use crate::domain::cancellation::CancellationToken;
use crate::domain::error::AppError;
use crate::domain::models::{HttpRequest, HttpResponse};

/// Blocking on purpose: the caller decides how to get off the UI thread
/// (the command layer uses a blocking task), and a blocking trait keeps the
/// test mock trivial. The token lets a caller abort a transfer in progress.
pub trait HttpClient: Send + Sync {
    fn send(
        &self,
        request: &HttpRequest,
        cancel: &CancellationToken,
    ) -> Result<HttpResponse, AppError>;
}

use crate::domain::import_plan::{ImportPlan, ImportedIds};
use crate::domain::models::{
    Collection, Cookie, Environment, EnvironmentVariable, Example, ExampleSummary, Folder,
    HistoryEntry, NewExample, NewHistoryEntry, SavedRequest,
};
use crate::domain::secrets::SecretState;

/// One trait per aggregate rather than one storage god-trait
/// (CLAUDE.md section 7, interface segregation).
pub trait CollectionRepository: Send + Sync {
    fn list(&self) -> Result<Vec<Collection>, AppError>;
    fn create(&self, name: &str) -> Result<Collection, AppError>;
    fn rename(&self, id: &str, name: &str) -> Result<(), AppError>;
    /// Cascades to the collection's folders and requests.
    fn delete(&self, id: &str) -> Result<(), AppError>;
}

pub trait FolderRepository: Send + Sync {
    fn list_by_collection(&self, collection_id: &str) -> Result<Vec<Folder>, AppError>;
    fn create(
        &self,
        collection_id: &str,
        parent_folder_id: Option<&str>,
        name: &str,
    ) -> Result<Folder, AppError>;
    fn rename(&self, id: &str, name: &str) -> Result<(), AppError>;
    fn delete(&self, id: &str) -> Result<(), AppError>;
}

pub trait CookieRepository: Send + Sync {
    fn list(&self) -> Result<Vec<Cookie>, AppError>;
    /// Replaces any cookie with the same (domain, path, name).
    fn upsert(&self, cookie: &Cookie) -> Result<(), AppError>;
    fn delete(&self, domain: &str, path: &str, name: &str) -> Result<(), AppError>;
    fn clear(&self) -> Result<(), AppError>;
    /// Session cookies die with the app, per RFC 6265.
    fn clear_session(&self) -> Result<(), AppError>;
    fn purge_expired(&self, now: u64) -> Result<(), AppError>;
}

/// Variables belong to an environment, so they sit behind the environment's
/// own trait rather than getting a second one for the same aggregate.
pub trait EnvironmentRepository: Send + Sync {
    fn list(&self) -> Result<Vec<Environment>, AppError>;
    fn create(&self, name: &str) -> Result<Environment, AppError>;
    fn rename(&self, id: &str, name: &str) -> Result<(), AppError>;
    /// Cascades to the environment's variables.
    fn delete(&self, id: &str) -> Result<(), AppError>;
    fn variables(&self, environment_id: &str) -> Result<Vec<EnvironmentVariable>, AppError>;
    /// Replaces the whole set: the editor saves every row at once. Secret
    /// values are sealed on the way in, which is safe to key on the row's
    /// position only because every save rewrites every row.
    fn set_variables(
        &self,
        environment_id: &str,
        variables: &[EnvironmentVariable],
    ) -> Result<(), AppError>;
}

pub trait SavedRequestRepository: Send + Sync {
    fn list_by_collection(&self, collection_id: &str) -> Result<Vec<SavedRequest>, AppError>;
    fn get(&self, id: &str) -> Result<SavedRequest, AppError>;
    /// Inserts when the id is new, replaces the stored row when it exists.
    fn save(&self, saved: &SavedRequest) -> Result<(), AppError>;
    fn rename(&self, id: &str, name: &str) -> Result<(), AppError>;
    /// `folder_id` of None moves the request to the collection root.
    fn move_to(&self, id: &str, folder_id: Option<&str>) -> Result<(), AppError>;
    fn delete(&self, id: &str) -> Result<(), AppError>;
}

/// Sent-request history. The repository owns trimming so the table stays
/// bounded without a caller having to remember to sweep it.
pub trait HistoryRepository: Send + Sync {
    /// Newest first.
    fn list(&self, limit: u32) -> Result<Vec<HistoryEntry>, AppError>;
    /// Inserts, then drops everything past the newest `keep` entries, in one
    /// transaction. Returns the stored entry with its assigned id and time.
    fn record(&self, entry: &NewHistoryEntry, keep: u32) -> Result<HistoryEntry, AppError>;
    fn delete(&self, id: &str) -> Result<(), AppError>;
    fn clear(&self) -> Result<(), AppError>;
}

/// Saved responses, keyed to the request that produced them.
pub trait ExampleRepository: Send + Sync {
    /// Summaries only, in creation order — the tree draws all of a
    /// collection's examples at once and must not pull their bodies with it.
    fn list_summaries_by_collection(
        &self,
        collection_id: &str,
    ) -> Result<Vec<ExampleSummary>, AppError>;
    /// The full example, body included. Called when one is opened.
    fn get(&self, id: &str) -> Result<Example, AppError>;
    fn create(&self, example: &NewExample) -> Result<Example, AppError>;
    fn rename(&self, id: &str, name: &str) -> Result<(), AppError>;
    fn delete(&self, id: &str) -> Result<(), AppError>;
}

/// Writes a whole OpenAPI import — collection, folders, requests, examples
/// and environment — in one transaction (PLAN.md Phase 8d). Its own trait
/// because the point is atomicity across aggregates, which none of the
/// per-aggregate repositories can offer.
pub trait ImportRepository: Send + Sync {
    fn import(&self, plan: &ImportPlan) -> Result<ImportedIds, AppError>;
}

/// What a sealed secret turned out to be when opened.
#[derive(Debug, PartialEq, Eq)]
pub enum OpenedSecret {
    Plain(String),
    /// The value cannot be recovered; `SecretState` says why.
    Lost(SecretState),
}

/// Encrypts secrets for storage (PLAN.md Phase 9). The key never leaves the
/// implementation; callers only ever hand over a scope and a value.
///
/// `scope` is bound into the ciphertext as associated data — see
/// `domain::secrets::secret_scope` — so a value only opens in the place it
/// was sealed for.
/// App-level preferences, keyed by name. Values are plain text and never
/// hold a secret — those go through `SecretCipher` like everything else.
pub trait AppSettingsRepository: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, AppError>;
    fn set(&self, key: &str, value: &str) -> Result<(), AppError>;
}

pub trait SecretCipher: Send + Sync {
    /// Fails when there is no usable key this session. Never falls back to
    /// returning the plaintext.
    fn seal(&self, scope: &str, plaintext: &str) -> Result<Vec<u8>, AppError>;
    fn open(&self, scope: &str, sealed: &[u8]) -> OpenedSecret;
    /// Whether `seal` can work. Checked before writing an empty secret over a
    /// stored one, so a credential store that is merely unreachable cannot
    /// cause a secret that still exists to be overwritten.
    fn ensure_available(&self) -> Result<(), AppError>;
}

/// Where the data key is kept: the OS credential store in the app, memory in
/// tests. Only the raw bytes cross this boundary; what they mean is the
/// secrets module's business.
pub trait DataKeyStore: Send + Sync {
    /// `Ok(None)` means no key has been stored yet — the only case in which a
    /// new one may be created. Every other failure is an error, because
    /// replacing a key that exists but could not be read would make every
    /// secret sealed under it unreadable.
    fn load(&self) -> Result<Option<zeroize::Zeroizing<Vec<u8>>>, AppError>;
    fn store(&self, key: &[u8]) -> Result<(), AppError>;
}
