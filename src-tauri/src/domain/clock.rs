// http_client/src-tauri/src/domain/clock.rs
//
// Wall-clock time in milliseconds, in one place. Live connections and
// streaming responses both stamp what they report the moment it happens, so
// a slow screen cannot reorder a log.
use std::time::{SystemTime, UNIX_EPOCH};

/// Unix time in milliseconds. A clock that cannot be read (before the epoch,
/// which would mean the machine's clock is set to the 1960s) reports 0
/// rather than failing something that is only a timestamp.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}
