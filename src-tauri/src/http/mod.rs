// http_client/src-tauri/src/http/mod.rs
pub mod auth;
pub mod cookie_client;
pub mod cookie_websocket;
pub mod curl_client;
pub mod curl_websocket;
// The one module allowed `unsafe`: libcurl's WebSocket API has no Rust
// binding (PLAN.md Phase 13a). lib.rs denies unsafe code everywhere else.
#[allow(unsafe_code)]
pub mod curl_ws_ffi;
pub mod mapping;
