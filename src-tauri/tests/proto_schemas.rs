// http_client/src-tauri/tests/proto_schemas.rs
//
// The schema library repository against an in-memory SQLite database running
// the real migrations (PLAN-GRPC.md 16g-1, CLAUDE.md section 8).
use std::collections::BTreeMap;

use responderhttp_lib::domain::error::AppError;
use responderhttp_lib::domain::models::{SchemaOrigin, StoredProtoSchema};
use responderhttp_lib::domain::ports::ProtoSchemaRepository;
use responderhttp_lib::persistence::database::Database;
use responderhttp_lib::persistence::repositories::proto_schemas::SqliteProtoSchemaRepository;

fn database() -> Database {
    Database::open_in_memory().expect("in-memory database should open")
}

fn schema(id: &str, name: &str) -> StoredProtoSchema {
    let mut sources = BTreeMap::new();
    sources.insert(
        "shop/v1/shop.proto".to_string(),
        "syntax = \"proto3\";\npackage shop.v1;\n".to_string(),
    );
    StoredProtoSchema {
        id: id.into(),
        name: name.into(),
        origin: SchemaOrigin::Import,
        // Opaque to the repository; any bytes, including zeros, round-trip.
        encoded: vec![0x0a, 0x00, 0xff, 0x10],
        sources,
        updated_at: String::new(),
    }
}

/// A saved gRPC request row, written directly: saving gRPC requests is 16g-2.
fn insert_grpc_request(database: &Database, id: &str, name: &str, grpc_json: &str) {
    let connection = database.lock();
    connection
        .execute(
            "INSERT OR IGNORE INTO collections (id, name, created_at)
             VALUES ('c1', 'Shop', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("collection should insert");
    connection
        .execute(
            "INSERT INTO requests (
                 id, collection_id, name, method, url, headers_json,
                 query_params_json, body_json, settings_json, created_at,
                 updated_at, kind, grpc_json
             ) VALUES (?1, 'c1', ?2, 'POST', '', '[]', '[]', '{}', '{}',
                       '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'grpc', ?3)",
            [id, name, grpc_json],
        )
        .expect("request should insert");
}

#[test]
fn a_saved_schema_round_trips_with_its_bytes_and_sources() {
    let repository = SqliteProtoSchemaRepository::new(database());
    let saved = schema("proto_1", "Shop");
    repository.save(&saved).expect("should save");

    let loaded = repository.get("proto_1").expect("should load");
    assert_eq!(loaded.name, "Shop");
    assert_eq!(loaded.origin, SchemaOrigin::Import);
    assert_eq!(loaded.encoded, saved.encoded);
    assert_eq!(loaded.sources, saved.sources);
    assert!(!loaded.updated_at.is_empty());
}

#[test]
fn a_reflected_schema_keeps_its_origin_and_empty_sources() {
    let repository = SqliteProtoSchemaRepository::new(database());
    let mut reflected = schema("proto_2", "Live");
    reflected.origin = SchemaOrigin::Reflection;
    reflected.sources.clear();
    repository.save(&reflected).expect("should save");

    let loaded = repository.get("proto_2").expect("should load");
    assert_eq!(loaded.origin, SchemaOrigin::Reflection);
    assert!(loaded.sources.is_empty());
}

#[test]
fn saving_the_same_id_again_replaces_the_row() {
    let repository = SqliteProtoSchemaRepository::new(database());
    repository
        .save(&schema("proto_1", "Shop"))
        .expect("should save");
    let mut changed = schema("proto_1", "Shop v2");
    changed.encoded = vec![1, 2, 3];
    repository.save(&changed).expect("should save again");

    let listed = repository.list().expect("should list");
    assert_eq!(listed.len(), 1);
    let loaded = repository.get("proto_1").expect("should load");
    assert_eq!(loaded.name, "Shop v2");
    assert_eq!(loaded.encoded, vec![1, 2, 3]);
}

#[test]
fn the_library_lists_by_name_ignoring_case() {
    let repository = SqliteProtoSchemaRepository::new(database());
    repository.save(&schema("a", "zebra")).expect("should save");
    repository.save(&schema("b", "Apple")).expect("should save");
    repository.save(&schema("c", "mango")).expect("should save");

    let names: Vec<String> = repository
        .list()
        .expect("should list")
        .into_iter()
        .map(|summary| summary.name)
        .collect();
    assert_eq!(names, ["Apple", "mango", "zebra"]);
}

#[test]
fn an_unknown_id_is_not_found_for_get_rename_and_delete() {
    let repository = SqliteProtoSchemaRepository::new(database());
    assert!(matches!(repository.get("nope"), Err(AppError::NotFound(_))));
    assert!(matches!(
        repository.rename("nope", "X"),
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        repository.delete("nope"),
        Err(AppError::NotFound(_))
    ));
}

#[test]
fn rename_changes_only_the_name() {
    let repository = SqliteProtoSchemaRepository::new(database());
    let saved = schema("proto_1", "Shop");
    repository.save(&saved).expect("should save");
    repository
        .rename("proto_1", "Store")
        .expect("should rename");

    let loaded = repository.get("proto_1").expect("should load");
    assert_eq!(loaded.name, "Store");
    assert_eq!(loaded.encoded, saved.encoded);
}

#[test]
fn delete_removes_the_schema() {
    let repository = SqliteProtoSchemaRepository::new(database());
    repository
        .save(&schema("proto_1", "Shop"))
        .expect("should save");
    repository.delete("proto_1").expect("should delete");
    assert!(repository.list().expect("should list").is_empty());
}

#[test]
fn users_are_the_saved_requests_whose_grpc_json_names_the_schema() {
    let database = database();
    let repository = SqliteProtoSchemaRepository::new(database.clone());
    repository
        .save(&schema("proto_1", "Shop"))
        .expect("should save");
    insert_grpc_request(&database, "r1", "list orders", r#"{"schema_id":"proto_1"}"#);
    insert_grpc_request(&database, "r2", "Get order", r#"{"schema_id":"proto_1"}"#);
    insert_grpc_request(&database, "r3", "Other", r#"{"schema_id":"proto_9"}"#);

    let users = repository.users("proto_1").expect("should query");
    assert_eq!(users, ["Get order", "list orders"]);
    assert!(repository
        .users("proto_2")
        .expect("should query")
        .is_empty());
}

#[test]
fn existing_request_rows_get_an_empty_grpc_json_from_the_migration() {
    let database = database();
    let connection = database.lock();
    connection
        .execute(
            "INSERT INTO collections (id, name, created_at)
             VALUES ('c1', 'Shop', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("collection should insert");
    connection
        .execute(
            "INSERT INTO requests (
                 id, collection_id, name, method, url, headers_json,
                 query_params_json, body_json, settings_json, created_at, updated_at
             ) VALUES ('r1', 'c1', 'Plain', 'GET', 'https://example.com', '[]', '[]',
                       '{}', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .expect("request should insert");
    let grpc_json: String = connection
        .query_row(
            "SELECT grpc_json FROM requests WHERE id = 'r1'",
            [],
            |row| row.get(0),
        )
        .expect("row should exist");
    assert_eq!(grpc_json, "");
}
