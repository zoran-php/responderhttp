// http_client/src-tauri/src/domain/cancellation.rs
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Shared flag polled by the transport while a request is in flight. Cloning
/// shares the same flag, so the command layer can hold one copy and hand
/// another to the client.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clone_observes_cancellation() {
        let token = CancellationToken::new();
        let clone = token.clone();

        assert!(!clone.is_cancelled());
        token.cancel();

        assert!(clone.is_cancelled());
    }
}
