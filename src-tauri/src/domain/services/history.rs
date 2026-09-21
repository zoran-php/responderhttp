// http_client/src-tauri/src/domain/services/history.rs
//
// Use-cases behind the History panel. Searching is done in the frontend over
// the capped list (lib/history-filter.ts) rather than in SQL: at 500 rows it
// is instant either way, and a pure function is testable.
use std::sync::Arc;

use crate::domain::error::AppError;
use crate::domain::models::{HistoryEntry, NewHistoryEntry};
use crate::domain::ports::HistoryRepository;
use crate::domain::secrets::request_without_literal_secrets;

/// How many entries survive. Weeks of normal use, and it means a token sent
/// months ago does not sit on disk forever.
pub const HISTORY_LIMIT: u32 = 500;

#[derive(Clone)]
pub struct History {
    history: Arc<dyn HistoryRepository>,
}

impl History {
    pub fn new(history: Arc<dyn HistoryRepository>) -> Self {
        Self { history }
    }

    pub fn list(&self) -> Result<Vec<HistoryEntry>, AppError> {
        self.history.list(HISTORY_LIMIT)
    }

    /// History keeps no secrets (PLAN.md Phase 9). A literal auth secret is
    /// blanked here; a `{{placeholder}}` is kept, so a re-run still works.
    pub fn record(&self, entry: &NewHistoryEntry) -> Result<HistoryEntry, AppError> {
        let entry = NewHistoryEntry {
            request: request_without_literal_secrets(&entry.request),
            ..entry.clone()
        };
        self.history.record(&entry, HISTORY_LIMIT)
    }

    pub fn delete(&self, id: &str) -> Result<(), AppError> {
        self.history.delete(id)
    }

    pub fn clear(&self) -> Result<(), AppError> {
        self.history.clear()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{Auth, HttpMethod, HttpRequest, RequestBody, RequestSettings};
    use std::sync::Mutex;

    #[derive(Default)]
    struct SpyHistory {
        recorded: Mutex<Vec<NewHistoryEntry>>,
    }

    impl HistoryRepository for SpyHistory {
        fn list(&self, _limit: u32) -> Result<Vec<HistoryEntry>, AppError> {
            Ok(Vec::new())
        }

        fn record(&self, entry: &NewHistoryEntry, _keep: u32) -> Result<HistoryEntry, AppError> {
            self.recorded
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(entry.clone());
            Ok(HistoryEntry {
                id: "his_1".into(),
                sent_at: "2026-09-16T00:00:00Z".into(),
                resolved_url: entry.resolved_url.clone(),
                status: entry.status,
                error_kind: entry.error_kind.clone(),
                duration_ms: entry.duration_ms,
                request: entry.request.clone(),
            })
        }

        fn delete(&self, _id: &str) -> Result<(), AppError> {
            Ok(())
        }

        fn clear(&self) -> Result<(), AppError> {
            Ok(())
        }
    }

    fn entry_with(auth: Auth) -> NewHistoryEntry {
        NewHistoryEntry {
            resolved_url: "https://example.test".into(),
            status: Some(200),
            error_kind: None,
            duration_ms: 12,
            request: HttpRequest {
                method: HttpMethod::Get,
                url: "https://example.test".into(),
                headers: Vec::new(),
                query_params: Vec::new(),
                body: RequestBody::None,
                auth,
                settings: RequestSettings::default(),
            },
        }
    }

    #[test]
    fn a_literal_secret_never_reaches_the_repository() {
        let spy = Arc::new(SpyHistory::default());
        let service = History::new(spy.clone());

        let stored = service
            .record(&entry_with(Auth::Bearer {
                token: "live-token".into(),
            }))
            .expect("should record");

        let recorded = spy
            .recorded
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let blank = Auth::Bearer {
            token: String::new(),
        };
        assert_eq!(recorded[0].request.auth, blank);
        assert_eq!(stored.request.auth, blank);
    }

    #[test]
    fn a_placeholder_is_kept_so_a_rerun_still_resolves() {
        let spy = Arc::new(SpyHistory::default());
        let service = History::new(spy.clone());
        let auth = Auth::Bearer {
            token: "{{token}}".into(),
        };

        service
            .record(&entry_with(auth.clone()))
            .expect("should record");

        let recorded = spy
            .recorded
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert_eq!(recorded[0].request.auth, auth);
    }
}
