// http_client/src-tauri/src/domain/error.rs
//
// Single domain error type. Variants are added as each phase introduces a
// new failure mode; nothing here knows about Tauri or serde — the commands
// layer maps this into a DTO for the frontend.
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    /// The request could not be built from what the user supplied.
    #[error("{0}")]
    InvalidRequest(String),

    /// libcurl could not complete the exchange (DNS, TLS, timeout, reset...).
    #[error("{0}")]
    Transport(String),

    /// The user aborted the request. Distinct from Transport so the UI can
    /// stay quiet instead of reporting a failure the user caused.
    #[error("request cancelled")]
    Cancelled,

    /// Reading or writing local storage failed.
    #[error("{0}")]
    Storage(String),

    /// The referenced collection, folder or saved request does not exist.
    #[error("{0}")]
    NotFound(String),

    /// A failure the user cannot act on and did not cause — a serialiser that
    /// refused a value we built ourselves, for instance. Distinct from
    /// Storage so "the disk said no" stays a separate thing from "we produced
    /// something we could not write".
    #[error("{0}")]
    Internal(String),

    /// The OS credential store that holds the data key could not be used, so
    /// a secret cannot be encrypted. Kept apart from Storage because the fix
    /// is different: the database is fine, and the credential store (or the
    /// user's access to it) is not.
    #[error("{0}")]
    SecretStore(String),
}
