// http_client/src-tauri/src/commands/error.rs
use serde::Serialize;

use crate::domain::error::AppError;

/// What a failed command returns to TypeScript: a discriminated union, so the
/// UI can branch on `kind` instead of matching on message text.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ApiError {
    InvalidRequest { message: String },
    Transport { message: String },
    Cancelled { message: String },
    NotFound { message: String },
    Storage { message: String },
    Internal { message: String },
    SecretStore { message: String },
}

impl ApiError {
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }
}

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::InvalidRequest(message) => Self::InvalidRequest { message },
            AppError::Transport(message) => Self::Transport { message },
            AppError::Cancelled => Self::Cancelled {
                message: AppError::Cancelled.to_string(),
            },
            AppError::NotFound(message) => Self::NotFound { message },
            AppError::Storage(message) => Self::Storage { message },
            AppError::Internal(message) => Self::Internal { message },
            AppError::SecretStore(message) => Self::SecretStore { message },
        }
    }
}
