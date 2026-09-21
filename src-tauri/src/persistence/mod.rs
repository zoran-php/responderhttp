// http_client/src-tauri/src/persistence/mod.rs
//
// SQL lives here and nowhere else in the codebase — never built by string
// concatenation, always parameterised, every write in a transaction
// (CLAUDE.md section 5, section 11 rule 4).
pub mod database;
pub mod repositories;
