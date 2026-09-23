// http_client/src-tauri/src/persistence/repositories/request_kind.rs
//
// The two values of `requests.kind` (migration 0010). A closed enum so the
// column can only ever be written with one of them: SQLite cannot add a CHECK
// constraint to an existing table by ALTER TABLE, so this is the guard.
//
// The value is always bound as a parameter, never formatted into SQL.
use crate::domain::error::AppError;

/// Which kind of request a row in `requests` holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestKind {
    Http,
    WebSocket,
}

impl RequestKind {
    /// What the column stores. `http` is also the column's default, which is
    /// what makes every row written before migration 0010 an HTTP request.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::WebSocket => "websocket",
        }
    }

    /// How a refusal names the kind the row turned out to be.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Http => "an HTTP request",
            Self::WebSocket => "a WebSocket request",
        }
    }

    fn other(self) -> Self {
        match self {
            Self::Http => Self::WebSocket,
            Self::WebSocket => Self::Http,
        }
    }
}

/// Reads the result of an upsert whose `DO UPDATE` is guarded by
/// `WHERE requests.kind = <expected>`. Zero rows changed can only mean the
/// id already belongs to a row of the other kind: a new id inserts, and an
/// id of the right kind updates.
///
/// Refused rather than overwritten, because overwriting would leave a row
/// whose `kind` says one thing and whose columns say another — a WebSocket
/// that loads as a broken GET, or an HTTP request with no method at all.
pub fn refuse_other_kind(changed: usize, id: &str, expected: RequestKind) -> Result<(), AppError> {
    if changed > 0 {
        return Ok(());
    }
    Err(AppError::InvalidRequest(format!(
        "request {id} is {} and cannot be saved as {}",
        expected.other().describe(),
        expected.describe()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These strings reach the database, and `http` has to match the default
    /// in 0010_web_sockets.sql or every existing row changes kind.
    #[test]
    fn each_kind_stores_its_own_string() {
        assert_eq!(RequestKind::Http.as_str(), "http");
        assert_eq!(RequestKind::WebSocket.as_str(), "websocket");
    }

    #[test]
    fn a_changed_row_is_accepted() {
        assert!(refuse_other_kind(1, "req_1", RequestKind::Http).is_ok());
    }

    #[test]
    fn no_changed_row_is_refused_naming_both_kinds() {
        let error = refuse_other_kind(0, "req_1", RequestKind::Http).expect_err("should refuse");

        assert_eq!(
            error.to_string(),
            "request req_1 is a WebSocket request and cannot be saved as an HTTP request"
        );
    }
}
