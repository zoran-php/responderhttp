// http_client/src-tauri/src/commands/mod.rs
//
// #[tauri::command] adapters, grouped by feature. A command parses input,
// calls a domain service, and maps the result — no business logic, no curl
// calls, no SQL. See CLAUDE.md section 2 and section 11 rule 3.
pub mod collections;
pub mod cookies;
pub mod docs;
pub mod dto;
pub mod environments;
pub mod error;
pub mod files;
pub mod history;
pub mod openapi;
pub mod openapi_import;
pub mod request;
