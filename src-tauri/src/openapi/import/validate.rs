// http_client/src-tauri/src/openapi/import/validate.rs
//
// Checks a document against the official OAI JSON Schema for its version.
// Any failure refuses the import (PLAN.md Phase 8d, step 2).
//
// The schemas are vendored in `schemas/openapi` and compiled into the binary.
// The loader boon would otherwise use is replaced with one that refuses every
// URL, so validation can never read a file or reach the network — only the
// vendored set below is resolvable.
//
// For 3.1 and 3.2 the root is the `schema-base` variant, which also checks
// every Schema Object against the OpenAPI dialect. A document that declares a
// different `jsonSchemaDialect` would be refused by that for reasons that
// have nothing to do with it being a valid OpenAPI document, so those are
// checked against the plain `schema` instead and the caller is told.
use std::error::Error;

use boon::{Compiler, ErrorKind, Schemas, ValidationError};
use serde_json::Value;

use crate::domain::error::AppError;
use crate::openapi::document::OpenApiVersion;
use crate::openapi::import::{Refusal, SchemaViolation};

/// How many violations the dialog lists. The total is always reported.
pub const MAX_SHOWN_VIOLATIONS: usize = 20;

struct Resource {
    id: &'static str,
    text: &'static str,
}

macro_rules! vendored {
    ($id:literal, $file:literal) => {
        Resource {
            id: $id,
            text: include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/schemas/openapi/",
                $file
            )),
        }
    };
}

const RESOURCES: &[Resource] = &[
    vendored!(
        "https://spec.openapis.org/oas/3.0/schema/2024-10-18",
        "oas-3.0_schema_2024-10-18.json"
    ),
    vendored!(
        "https://spec.openapis.org/oas/3.1/schema/2025-09-15",
        "oas-3.1_schema_2025-09-15.json"
    ),
    vendored!(
        "https://spec.openapis.org/oas/3.1/schema-base/2025-09-15",
        "oas-3.1_schema-base_2025-09-15.json"
    ),
    vendored!(
        "https://spec.openapis.org/oas/3.1/dialect/2024-11-10",
        "oas-3.1_dialect_2024-11-10.json"
    ),
    vendored!(
        "https://spec.openapis.org/oas/3.1/meta/2024-11-10",
        "oas-3.1_meta_2024-11-10.json"
    ),
    vendored!(
        "https://spec.openapis.org/oas/3.2/schema/2025-09-17",
        "oas-3.2_schema_2025-09-17.json"
    ),
    vendored!(
        "https://spec.openapis.org/oas/3.2/schema-base/2025-09-17",
        "oas-3.2_schema-base_2025-09-17.json"
    ),
    vendored!(
        "https://spec.openapis.org/oas/3.2/dialect/2025-09-17",
        "oas-3.2_dialect_2025-09-17.json"
    ),
    vendored!(
        "https://spec.openapis.org/oas/3.2/meta/2025-09-17",
        "oas-3.2_meta_2025-09-17.json"
    ),
];

/// Which schema a version is checked against, and the dialect that schema
/// expects `jsonSchemaDialect` to name when present.
struct Target {
    base: &'static str,
    plain: &'static str,
    dialect: Option<&'static str>,
}

fn target(version: OpenApiVersion) -> Target {
    match version {
        OpenApiVersion::V3_0 => Target {
            base: "https://spec.openapis.org/oas/3.0/schema/2024-10-18",
            plain: "https://spec.openapis.org/oas/3.0/schema/2024-10-18",
            dialect: None,
        },
        OpenApiVersion::V3_1 => Target {
            base: "https://spec.openapis.org/oas/3.1/schema-base/2025-09-15",
            plain: "https://spec.openapis.org/oas/3.1/schema/2025-09-15",
            dialect: Some("https://spec.openapis.org/oas/3.1/dialect/2024-11-10"),
        },
        OpenApiVersion::V3_2 => Target {
            base: "https://spec.openapis.org/oas/3.2/schema-base/2025-09-17",
            plain: "https://spec.openapis.org/oas/3.2/schema/2025-09-17",
            dialect: Some("https://spec.openapis.org/oas/3.2/dialect/2025-09-17"),
        },
    }
}

/// What a passing validation says about how thorough it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// Structure and every Schema Object.
    Full,
    /// Structure only: the document names a JSON Schema dialect other than
    /// OpenAPI's own, so its Schema Objects were not checked.
    StructureOnly,
}

/// Refuses every URL. Only resources added up front can be resolved.
struct NoLoader;

impl boon::UrlLoader for NoLoader {
    fn load(&self, url: &str) -> Result<Value, Box<dyn Error>> {
        Err(format!("{url} is not one of the bundled OpenAPI schemas").into())
    }
}

/// `Err(AppError)` only if the vendored schemas themselves fail to load,
/// which is a build defect, not something about the user's file.
pub fn validate(
    document: &Value,
    version: OpenApiVersion,
) -> Result<Result<Coverage, Refusal>, AppError> {
    let target = target(version);
    let declared_dialect = document.get("jsonSchemaDialect").and_then(Value::as_str);
    let (root, coverage) = match (target.dialect, declared_dialect) {
        (Some(ours), Some(theirs)) if ours != theirs => (target.plain, Coverage::StructureOnly),
        _ => (target.base, Coverage::Full),
    };

    let mut schemas = Schemas::new();
    let index = compiler()?
        .compile(root, &mut schemas)
        .map_err(|error| AppError::Internal(format!("bundled OpenAPI schema: {error}")))?;

    Ok(match schemas.validate(document, index) {
        Ok(()) => Ok(coverage),
        Err(error) => {
            let all = violations(&error);
            Err(Refusal::SchemaViolations {
                version,
                total: all.len(),
                shown: all.into_iter().take(MAX_SHOWN_VIOLATIONS).collect(),
            })
        }
    })
}

fn compiler() -> Result<Compiler, AppError> {
    let mut compiler = Compiler::new();
    compiler.use_loader(Box::new(NoLoader));
    for resource in RESOURCES {
        let json: Value = serde_json::from_str(resource.text).map_err(|error| {
            AppError::Internal(format!(
                "bundled schema {} is not JSON: {error}",
                resource.id
            ))
        })?;
        compiler.add_resource(resource.id, json).map_err(|error| {
            AppError::Internal(format!("bundled schema {}: {error}", resource.id))
        })?;
    }
    Ok(compiler)
}

/// The leaves of boon's error tree, in the order it found them, without
/// repeats. Inner nodes ("oneOf failed", "validation failed") restate what
/// their leaves say less precisely.
fn violations(error: &ValidationError<'_, '_>) -> Vec<SchemaViolation> {
    let mut found: Vec<SchemaViolation> = Vec::new();
    collect(error, &mut found);
    if found.is_empty() {
        found.push(violation(error));
    }
    found
}

fn collect(error: &ValidationError<'_, '_>, found: &mut Vec<SchemaViolation>) {
    if error.causes.is_empty() {
        if is_summary(&error.kind) {
            return;
        }
        let candidate = violation(error);
        if !found.contains(&candidate) {
            found.push(candidate);
        }
        return;
    }
    for cause in &error.causes {
        collect(cause, found);
    }
}

fn is_summary(kind: &ErrorKind<'_, '_>) -> bool {
    matches!(
        kind,
        ErrorKind::Group | ErrorKind::Schema { .. } | ErrorKind::Reference { .. }
    )
}

fn violation(error: &ValidationError<'_, '_>) -> SchemaViolation {
    SchemaViolation {
        location: error.instance_location.to_string(),
        message: error.kind.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn minimal(version: &str) -> Value {
        json!({
            "openapi": version,
            "info": {"title": "t", "version": "1"},
            "paths": {}
        })
    }

    fn check(document: &Value, version: OpenApiVersion) -> Result<Coverage, Refusal> {
        validate(document, version).expect("the bundled schemas should load")
    }

    fn violations_of(document: &Value, version: OpenApiVersion) -> Vec<SchemaViolation> {
        match check(document, version) {
            Err(Refusal::SchemaViolations { shown, .. }) => shown,
            other => panic!("expected schema violations, got {other:?}"),
        }
    }

    #[test]
    fn every_bundled_schema_compiles_for_every_version() {
        for version in [
            OpenApiVersion::V3_0,
            OpenApiVersion::V3_1,
            OpenApiVersion::V3_2,
        ] {
            let target = target(version);
            for root in [target.base, target.plain] {
                let mut schemas = Schemas::new();
                compiler()
                    .expect("resources load")
                    .compile(root, &mut schemas)
                    .unwrap_or_else(|error| panic!("{root}: {error}"));
            }
        }
    }

    #[test]
    fn a_minimal_document_passes_in_every_version() {
        assert_eq!(
            check(&minimal("3.0.3"), OpenApiVersion::V3_0),
            Ok(Coverage::Full)
        );
        assert_eq!(
            check(&minimal("3.1.1"), OpenApiVersion::V3_1),
            Ok(Coverage::Full)
        );
        assert_eq!(
            check(&minimal("3.2.0"), OpenApiVersion::V3_2),
            Ok(Coverage::Full)
        );
    }

    #[test]
    fn a_missing_title_is_reported_at_info() {
        let mut document = minimal("3.1.0");
        document["info"].as_object_mut().unwrap().remove("title");

        let found = violations_of(&document, OpenApiVersion::V3_1);

        assert!(
            found
                .iter()
                .any(|v| v.location == "/info" && v.message.contains("title")),
            "{found:?}"
        );
    }

    #[test]
    fn a_body_parameter_is_not_valid_openapi_3() {
        let mut document = minimal("3.0.3");
        document["paths"] = json!({"/a/{id}": {"get": {
            "parameters": [{"name": "id", "in": "body", "required": true, "schema": {"type": "string"}}],
            "responses": {"200": {"description": "ok"}}
        }}});

        let found = violations_of(&document, OpenApiVersion::V3_0);

        assert!(
            found
                .iter()
                .all(|v| v.location.starts_with("/paths/~1a~1{id}/get/parameters/0")),
            "{found:?}"
        );
    }

    #[test]
    fn schema_objects_are_checked_against_the_dialect() {
        let mut document = minimal("3.1.0");
        document["components"] =
            json!({"schemas": {"X": {"type": "object", "properties": {"a": {"type": "strnig"}}}}});

        let found = violations_of(&document, OpenApiVersion::V3_1);

        assert!(
            found
                .iter()
                .any(|v| v.location == "/components/schemas/X/properties/a/type"),
            "{found:?}"
        );
    }

    #[test]
    fn a_3_2_only_field_is_refused_in_3_1() {
        let mut document = minimal("3.1.0");
        document["components"] = json!({"examples": {"e": {"serializedValue": "{}"}}});

        assert!(check(&document, OpenApiVersion::V3_1).is_err());

        let mut as_3_2 = document.clone();
        as_3_2["openapi"] = json!("3.2.0");
        assert_eq!(check(&as_3_2, OpenApiVersion::V3_2), Ok(Coverage::Full));
    }

    #[test]
    fn a_foreign_dialect_falls_back_to_structure_only() {
        let mut document = minimal("3.1.0");
        document["jsonSchemaDialect"] = json!("https://json-schema.org/draft/2020-12/schema");
        document["components"] = json!({"schemas": {"X": {"type": "object", "x-anything": true}}});

        assert_eq!(
            check(&document, OpenApiVersion::V3_1),
            Ok(Coverage::StructureOnly)
        );
    }

    #[test]
    fn the_declared_openapi_dialect_keeps_full_coverage() {
        let mut document = minimal("3.2.0");
        document["jsonSchemaDialect"] =
            json!("https://spec.openapis.org/oas/3.2/dialect/2025-09-17");

        assert_eq!(check(&document, OpenApiVersion::V3_2), Ok(Coverage::Full));
    }

    #[test]
    fn the_version_pattern_is_the_schemas_to_enforce() {
        assert!(check(&minimal("3.2"), OpenApiVersion::V3_2).is_err());
    }

    #[test]
    fn the_list_is_capped_but_the_total_is_not() {
        let mut document = minimal("3.1.0");
        let bad: serde_json::Map<String, Value> = (0..30)
            .map(|i| (format!("/p{i}"), json!({"get": {"responses": {"abc": {}}}})))
            .collect();
        document["paths"] = Value::Object(bad);

        let Err(Refusal::SchemaViolations { shown, total, .. }) =
            check(&document, OpenApiVersion::V3_1)
        else {
            panic!("expected violations");
        };

        assert_eq!(shown.len(), MAX_SHOWN_VIOLATIONS);
        assert!(total >= 30, "{total}");
    }

    #[test]
    fn nothing_outside_the_bundle_can_be_loaded() {
        let mut schemas = Schemas::new();
        let mut compiler = compiler().expect("resources load");

        for url in ["file:///etc/passwd", "https://example.com/schema.json"] {
            assert!(compiler.compile(url, &mut schemas).is_err(), "{url}");
        }
    }
}
