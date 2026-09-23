// http_client/src-tauri/src/desktop/mod.rs
//
// Desktop shell behaviour: main window lifecycle, system tray, single
// instance. Window/OS plumbing only — no domain logic, no HTTP, no SQL.
pub mod startup_error;
pub mod toast;
pub mod tray;
pub mod webview;
pub mod window;
