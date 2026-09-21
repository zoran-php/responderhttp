// http_client/src-tauri/src/openapi/mod.rs
//
// OpenAPI document generation. A wire format and its mapping, kept beside
// http/ and persistence/ rather than inside domain/ for the same reason
// `Stored*` lives in persistence: the domain does not deform to match a file
// spec (CLAUDE.md section 2).
//
// 3.1 and 3.2 share one struct set in `document`, because the only difference
// between them that this exporter can produce is a single field on the Example
// Object. 3.0 (still ahead) and the importer (8d) hang off the same
// `from_collection` output — see PLAN.md.
//
// `import` is the other direction (Phase 8d): a JSON or YAML file in, checked
// against the official schema, mapped to a collection plan.
pub mod document;
pub mod format;
pub mod from_collection;
pub mod import;
pub mod infer;
pub mod url;
