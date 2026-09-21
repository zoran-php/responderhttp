// http_client/src-tauri/src/domain/ids.rs
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Identifiers for stored entities. Unique is all they need to be: the wall
/// clock separates runs, the counter separates ids minted in the same
/// nanosecond, so no uuid crate is required.
pub fn new_id(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    format!("{prefix}_{nanos:x}{count:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_carry_their_prefix_and_do_not_repeat() {
        let first = new_id("col");
        let second = new_id("col");

        assert!(first.starts_with("col_"));
        assert_ne!(first, second);
    }
}
