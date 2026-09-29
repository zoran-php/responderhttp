// http_client/src-tauri/src/persistence/repositories/request_kind.rs
//
// The values of `requests.kind` (migrations 0010 and 0011). A closed enum so
// the column can only ever be written with one of them: SQLite cannot add a
// CHECK constraint to an existing table by ALTER TABLE, so this is the guard.
//
// The value is always bound as a parameter, never formatted into SQL.
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::error::AppError;
use crate::persistence::database::to_storage_error;

/// Which kind of request a row in `requests` holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestKind {
    Http,
    WebSocket,
    Grpc,
}

impl RequestKind {
    /// What the column stores. `http` is also the column's default, which is
    /// what makes every row written before migration 0010 an HTTP request.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::WebSocket => "websocket",
            Self::Grpc => "grpc",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "http" => Some(Self::Http),
            "websocket" => Some(Self::WebSocket),
            "grpc" => Some(Self::Grpc),
            _ => None,
        }
    }

    /// How a refusal names the kind the row turned out to be.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Http => "an HTTP request",
            Self::WebSocket => "a WebSocket request",
            Self::Grpc => "a gRPC request",
        }
    }
}

/// Reads the result of an upsert whose `DO UPDATE` is guarded by
/// `WHERE requests.kind = <expected>`. Zero rows changed can only mean the
/// id already belongs to a row of another kind: a new id inserts, and an id
/// of the right kind updates. With three kinds, the row is asked which one
/// it is, so the refusal names it.
///
/// Refused rather than overwritten, because overwriting would leave a row
/// whose `kind` says one thing and whose columns say another — a WebSocket
/// that loads as a broken GET, or an HTTP request with no method at all.
pub fn refuse_other_kind(
    connection: &Connection,
    changed: usize,
    id: &str,
    expected: RequestKind,
) -> Result<(), AppError> {
    if changed > 0 {
        return Ok(());
    }
    let found: Option<String> = connection
        .query_row(
            "SELECT kind FROM requests WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()
        .map_err(to_storage_error)?;
    let found = found
        .as_deref()
        .and_then(RequestKind::parse)
        .map_or("a request of another kind", RequestKind::describe);
    Err(AppError::InvalidRequest(format!(
        "request {id} is {found} and cannot be saved as {}",
        expected.describe()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn requests_table(rows: &[(&str, &str)]) -> Connection {
        let connection = Connection::open_in_memory().expect("in-memory database should open");
        connection
            .execute(
                "CREATE TABLE requests (id TEXT PRIMARY KEY, kind TEXT NOT NULL)",
                [],
            )
            .expect("table should be created");
        for (id, kind) in rows {
            connection
                .execute(
                    "INSERT INTO requests (id, kind) VALUES (?1, ?2)",
                    params![id, kind],
                )
                .expect("row should insert");
        }
        connection
    }

    /// These strings reach the database, and `http` has to match the default
    /// in 0010_web_sockets.sql or every existing row changes kind.
    #[test]
    fn each_kind_stores_its_own_string_and_parses_back() {
        for kind in [RequestKind::Http, RequestKind::WebSocket, RequestKind::Grpc] {
            assert_eq!(RequestKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(RequestKind::Http.as_str(), "http");
        assert_eq!(RequestKind::WebSocket.as_str(), "websocket");
        assert_eq!(RequestKind::Grpc.as_str(), "grpc");
        assert_eq!(RequestKind::parse("ftp"), None);
    }

    #[test]
    fn a_changed_row_is_accepted() {
        let connection = requests_table(&[]);
        assert!(refuse_other_kind(&connection, 1, "req_1", RequestKind::Http).is_ok());
    }

    #[test]
    fn no_changed_row_is_refused_naming_both_kinds() {
        let connection = requests_table(&[("req_1", "websocket"), ("req_2", "grpc")]);

        let error = refuse_other_kind(&connection, 0, "req_1", RequestKind::Http)
            .expect_err("should refuse");
        assert_eq!(
            error.to_string(),
            "request req_1 is a WebSocket request and cannot be saved as an HTTP request"
        );

        let error = refuse_other_kind(&connection, 0, "req_2", RequestKind::WebSocket)
            .expect_err("should refuse");
        assert_eq!(
            error.to_string(),
            "request req_2 is a gRPC request and cannot be saved as a WebSocket request"
        );
    }
}
