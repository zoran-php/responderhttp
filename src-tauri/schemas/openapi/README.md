# Vendored OpenAPI schemas

Official JSON Schemas from the OpenAPI Initiative, used by the OpenAPI import
(PLAN.md Phase 8d) to validate a file before anything is imported. They are
compiled into the binary with `include_str!`; nothing is fetched at run time.

Source: https://github.com/OAI/spec.openapis.org (Apache License 2.0, see
LICENSE), downloaded 2026-09-17 from
`https://raw.githubusercontent.com/OAI/spec.openapis.org/main/oas/<path>`:

| File | `$id` path |
|---|---|
| oas-3.0_schema_2024-10-18.json | oas/3.0/schema/2024-10-18 |
| oas-3.1_schema_2025-09-15.json | oas/3.1/schema/2025-09-15 |
| oas-3.1_schema-base_2025-09-15.json | oas/3.1/schema-base/2025-09-15 |
| oas-3.1_dialect_2024-11-10.json | oas/3.1/dialect/2024-11-10 |
| oas-3.1_meta_2024-11-10.json | oas/3.1/meta/2024-11-10 |
| oas-3.2_schema_2025-09-17.json | oas/3.2/schema/2025-09-17 |
| oas-3.2_schema-base_2025-09-17.json | oas/3.2/schema-base/2025-09-17 |
| oas-3.2_dialect_2025-09-17.json | oas/3.2/dialect/2025-09-17 |
| oas-3.2_meta_2025-09-17.json | oas/3.2/meta/2025-09-17 |

Do not edit these files. To take a newer schema, add the new file, switch the
table in `src/openapi/import/validate.rs`, and keep the tests green.
