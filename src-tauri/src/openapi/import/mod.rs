// http_client/src-tauri/src/openapi/import/mod.rs
//
// OpenAPI import (PLAN.md Phase 8d). A file goes through four steps, each its
// own module, and any of the first three can refuse it:
//
//   detect        bytes -> JSON value, deciding JSON or YAML from the content
//   validate      the value against the official OAI schema for its version
//   refs          every `$ref` must be internal and must resolve
//   to_collection the value -> an `ImportPlan` plus notes on what was left out
//
// Everything here is pure: no file system, no database, no network. The
// domain service reads the file and the repository writes the plan.
pub mod detect;
pub mod grouping;
pub mod refs;
pub mod sample;
pub mod to_collection;
pub mod validate;

use crate::openapi::document::OpenApiVersion;

/// Which syntax the file turned out to be. Decided from the content, never
/// from the extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Json,
    Yaml,
}

impl SourceFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Json => "JSON",
            Self::Yaml => "YAML",
        }
    }
}

/// One schema failure, as the dialog shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaViolation {
    /// JSON pointer into the document, `""` for the root.
    pub location: String,
    pub message: String,
}

/// Why a file was not imported. Every variant is something the user can act
/// on, so each gets its own sentence rather than one generic failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    Unreadable {
        message: String,
    },
    TooLarge {
        bytes: u64,
        limit: u64,
    },
    NotUtf8,
    Empty,
    InvalidJson {
        message: String,
    },
    InvalidYaml {
        message: String,
    },
    /// Parsed, but the top level is not a set of keys: TOML, XML, plain text
    /// and the like all end up here.
    NotJsonOrYaml,
    NotOpenApi,
    Swagger2,
    VersionNotString,
    UnsupportedVersion {
        version: String,
    },
    SchemaViolations {
        version: OpenApiVersion,
        shown: Vec<SchemaViolation>,
        total: usize,
    },
    ExternalReferences {
        refs: Vec<String>,
    },
    BrokenReferences {
        refs: Vec<String>,
    },
}

/// A document that got through detection: known syntax, known version.
#[derive(Debug, Clone)]
pub struct Detected {
    pub document: serde_json::Value,
    pub format: SourceFormat,
    pub version: OpenApiVersion,
}
