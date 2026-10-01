// http_client/src-tauri/tests/grpc_requests.rs
//
// Saved gRPC requests against an in-memory SQLite database running the real
// migrations (PLAN-GRPC.md 16g-2, CLAUDE.md section 8).
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use responderhttp_lib::domain::error::AppError;
use responderhttp_lib::domain::models::{
    Auth, GrpcRequestDraft, GrpcSchemaRef, GrpcSettings, HttpMethod, HttpRequest, KeyValue,
    RequestBody, RequestSettings, SavedGrpcRequest, SavedRequest, SchemaOrigin, StoredProtoSchema,
};
use responderhttp_lib::domain::ports::{
    CollectionRepository, FolderRepository, GrpcRequestRepository, ProtoSchemaRepository,
    SavedRequestRepository, SecretCipher,
};
use responderhttp_lib::domain::secrets::SecretState;
use responderhttp_lib::domain::services::openapi::OpenApiExport;
use responderhttp_lib::openapi::document::OpenApiVersion;
use responderhttp_lib::openapi::format::ExportFormat;
use responderhttp_lib::persistence::database::Database;
use responderhttp_lib::persistence::repositories::collections::SqliteCollectionRepository;
use responderhttp_lib::persistence::repositories::examples::SqliteExampleRepository;
use responderhttp_lib::persistence::repositories::folders::SqliteFolderRepository;
use responderhttp_lib::persistence::repositories::grpc_requests::SqliteGrpcRequestRepository;
use responderhttp_lib::persistence::repositories::proto_schemas::SqliteProtoSchemaRepository;
use responderhttp_lib::persistence::repositories::saved_requests::SqliteSavedRequestRepository;
use responderhttp_lib::secrets::memory::MemoryDataKeyStore;
use responderhttp_lib::secrets::open_cipher;

const TOKEN: &str = "grpc-secret-token-4f9a";

struct Repos {
    database: Database,
    cipher: Arc<dyn SecretCipher>,
    collections: SqliteCollectionRepository,
    folders: SqliteFolderRepository,
    requests: SqliteSavedRequestRepository,
    grpc: SqliteGrpcRequestRepository,
    schemas: SqliteProtoSchemaRepository,
}

fn repos() -> Repos {
    let database = Database::open_in_memory().expect("in-memory database should open");
    let cipher: Arc<dyn SecretCipher> = open_cipher(&MemoryDataKeyStore::empty()).0;
    Repos {
        database: database.clone(),
        cipher: cipher.clone(),
        collections: SqliteCollectionRepository::new(database.clone()),
        folders: SqliteFolderRepository::new(database.clone()),
        requests: SqliteSavedRequestRepository::new(database.clone(), cipher.clone()),
        grpc: SqliteGrpcRequestRepository::new(database.clone(), cipher),
        schemas: SqliteProtoSchemaRepository::new(database),
    }
}

fn library_schema(repos: &Repos, id: &str) {
    repos
        .schemas
        .save(&StoredProtoSchema {
            id: id.into(),
            name: "Shop".into(),
            origin: SchemaOrigin::Import,
            encoded: vec![0x0a, 0x00],
            sources: BTreeMap::new(),
            updated_at: String::new(),
        })
        .expect("schema should save");
}

fn draft(schema: GrpcSchemaRef) -> GrpcRequestDraft {
    GrpcRequestDraft {
        url: "{{host}}:50051".into(),
        tls: false,
        method_path: "/shop.v1.Shop/GetOrder".into(),
        schema,
        metadata: vec![KeyValue::new("x-tenant", "acme")],
        auth: Auth::Bearer {
            token: TOKEN.into(),
        },
        message: "{\"id\": \"9007199254740993\"}".into(),
        settings: GrpcSettings {
            verify_tls: false,
            proxy: None,
            connect_timeout: Duration::from_secs(5),
            deadline: Some(Duration::from_millis(1500)),
            max_receive_bytes: 1024 * 1024,
            include_defaults: true,
        },
    }
}

fn saved(id: &str, collection_id: &str, name: &str, schema: GrpcSchemaRef) -> SavedGrpcRequest {
    SavedGrpcRequest {
        id: id.into(),
        collection_id: collection_id.into(),
        folder_id: None,
        name: name.into(),
        request: draft(schema),
        secret_state: SecretState::Ok,
    }
}

fn http_request(id: &str, collection_id: &str, name: &str) -> SavedRequest {
    SavedRequest {
        id: id.into(),
        collection_id: collection_id.into(),
        folder_id: None,
        name: name.into(),
        request: HttpRequest {
            method: HttpMethod::Get,
            url: "https://api.example.com/orders".into(),
            headers: Vec::new(),
            query_params: Vec::new(),
            body: RequestBody::None,
            auth: Auth::None,
            settings: RequestSettings::default(),
        },
        secret_state: SecretState::Ok,
    }
}

fn raw_column(database: &Database, column: &str, id: &str) -> String {
    database
        .lock()
        .query_row(
            &format!("SELECT {column} FROM requests WHERE id = ?1"),
            [id],
            |row| row.get(0),
        )
        .expect("row should exist")
}

#[test]
fn a_grpc_request_round_trips_with_every_field() {
    let repos = repos();
    let collection = repos.collections.create("Shop").expect("collection");
    library_schema(&repos, "proto_1");
    let original = saved(
        "req_g1",
        &collection.id,
        "Get order",
        GrpcSchemaRef::Library("proto_1".into()),
    );
    repos.grpc.save(&original).expect("should save");

    let loaded = repos.grpc.get("req_g1").expect("should load");
    assert_eq!(loaded, original);
    assert_eq!(raw_column(&repos.database, "kind", "req_g1"), "grpc");
    assert_eq!(raw_column(&repos.database, "method", "req_g1"), "POST");
}

#[test]
fn a_reflection_request_round_trips_without_a_schema_row() {
    let repos = repos();
    let collection = repos.collections.create("Shop").expect("collection");
    let original = saved("req_g1", &collection.id, "Live", GrpcSchemaRef::Reflection);
    repos.grpc.save(&original).expect("should save");

    assert_eq!(repos.grpc.get("req_g1").expect("load"), original);
}

/// CLAUDE.md §5: a token never reaches the database file in the clear.
#[test]
fn the_auth_secret_is_sealed_in_the_database_and_opens_on_load() {
    let repos = repos();
    let collection = repos.collections.create("Shop").expect("collection");
    repos
        .grpc
        .save(&saved("req_g1", &collection.id, "Get", GrpcSchemaRef::None))
        .expect("should save");

    let raw = raw_column(&repos.database, "auth_json", "req_g1");
    assert!(!raw.contains(TOKEN), "token stored in the clear: {raw}");
    let loaded = repos.grpc.get("req_g1").expect("load");
    assert_eq!(loaded.secret_state, SecretState::Ok);
    assert_eq!(
        loaded.request.auth,
        Auth::Bearer {
            token: TOKEN.into()
        }
    );
}

/// A schema loaded only for the session would be gone after a restart.
#[test]
fn a_library_schema_that_is_not_in_the_library_is_refused_and_nothing_is_written() {
    let repos = repos();
    let collection = repos.collections.create("Shop").expect("collection");
    let error = repos
        .grpc
        .save(&saved(
            "req_g1",
            &collection.id,
            "Get",
            GrpcSchemaRef::Library("proto_session".into()),
        ))
        .expect_err("should refuse");

    assert!(matches!(error, AppError::InvalidRequest(_)), "{error:?}");
    assert!(error.to_string().contains("proto_session"));
    assert!(matches!(
        repos.grpc.get("req_g1"),
        Err(AppError::NotFound(_))
    ));
}

#[test]
fn saving_an_id_of_another_kind_is_refused_both_ways() {
    let repos = repos();
    let collection = repos.collections.create("Shop").expect("collection");
    repos
        .requests
        .save(&http_request("req_h1", &collection.id, "List"))
        .expect("http save");
    repos
        .grpc
        .save(&saved("req_g1", &collection.id, "Get", GrpcSchemaRef::None))
        .expect("grpc save");

    let error = repos
        .grpc
        .save(&saved("req_h1", &collection.id, "Get", GrpcSchemaRef::None))
        .expect_err("should refuse");
    assert_eq!(
        error.to_string(),
        "request req_h1 is an HTTP request and cannot be saved as a gRPC request"
    );
    let error = repos
        .requests
        .save(&http_request("req_g1", &collection.id, "List"))
        .expect_err("should refuse");
    assert_eq!(
        error.to_string(),
        "request req_g1 is a gRPC request and cannot be saved as an HTTP request"
    );
}

#[test]
fn each_repository_sees_only_its_own_kind() {
    let repos = repos();
    let collection = repos.collections.create("Shop").expect("collection");
    repos
        .requests
        .save(&http_request("req_h1", &collection.id, "List"))
        .expect("http save");
    repos
        .grpc
        .save(&saved("req_g1", &collection.id, "Get", GrpcSchemaRef::None))
        .expect("grpc save");

    let grpc = repos.grpc.list_by_collection(&collection.id).expect("list");
    let http = repos
        .requests
        .list_by_collection(&collection.id)
        .expect("list");
    assert_eq!(grpc.len(), 1);
    assert_eq!(grpc[0].id, "req_g1");
    assert_eq!(http.len(), 1);
    assert_eq!(http[0].id, "req_h1");
    assert!(matches!(
        repos.grpc.get("req_h1"),
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        repos.requests.get("req_g1"),
        Err(AppError::NotFound(_))
    ));
}

/// D8: OpenAPI export of a mixed collection leaves the gRPC request out.
#[test]
fn an_openapi_export_of_a_mixed_collection_leaves_the_grpc_request_out() {
    let repos = repos();
    let collection = repos.collections.create("Shop").expect("collection");
    repos
        .requests
        .save(&http_request("req_h1", &collection.id, "List orders"))
        .expect("http save");
    repos
        .grpc
        .save(&saved(
            "req_g1",
            &collection.id,
            "Get order",
            GrpcSchemaRef::None,
        ))
        .expect("grpc save");
    let exporter = OpenApiExport::new(
        Arc::new(SqliteCollectionRepository::new(repos.database.clone())),
        Arc::new(SqliteFolderRepository::new(repos.database.clone())),
        Arc::new(SqliteSavedRequestRepository::new(
            repos.database.clone(),
            repos.cipher.clone(),
        )),
        Arc::new(SqliteExampleRepository::new(repos.database.clone())),
    );

    let exported = exporter
        .export(
            &collection.id,
            OpenApiVersion::V3_1,
            ExportFormat::Json,
            false,
        )
        .expect("export");

    assert!(exported.text.contains("/orders"), "{}", exported.text);
    assert!(!exported.text.contains("GetOrder"), "{}", exported.text);
    assert!(!exported.text.contains("Get order"), "{}", exported.text);
    assert!(!exported.text.contains("50051"), "{}", exported.text);
}

/// Rename, move, delete and docs act on the row by id, whatever its kind,
/// and Save does not wipe the documentation.
#[test]
fn rename_move_docs_and_delete_work_on_a_grpc_row_through_the_request_repository() {
    let repos = repos();
    let collection = repos.collections.create("Shop").expect("collection");
    let folder = repos
        .folders
        .create(&collection.id, None, "Orders")
        .expect("folder");
    let original = saved("req_g1", &collection.id, "Get", GrpcSchemaRef::None);
    repos.grpc.save(&original).expect("save");

    repos
        .requests
        .rename("req_g1", "Get order")
        .expect("rename");
    repos
        .requests
        .move_to("req_g1", Some(&folder.id))
        .expect("move");
    repos
        .requests
        .set_docs("req_g1", "Looks up one order.")
        .expect("docs");
    let loaded = repos.grpc.get("req_g1").expect("load");
    assert_eq!(loaded.name, "Get order");
    assert_eq!(loaded.folder_id.as_deref(), Some(folder.id.as_str()));

    let mut resaved = loaded;
    resaved.request.message = "{}".into();
    repos.grpc.save(&resaved).expect("re-save");
    assert_eq!(
        repos.requests.docs("req_g1").expect("docs"),
        "Looks up one order."
    );

    repos.requests.delete("req_g1").expect("delete");
    assert!(matches!(
        repos.grpc.get("req_g1"),
        Err(AppError::NotFound(_))
    ));
}

#[test]
fn deleting_the_collection_deletes_its_grpc_requests() {
    let repos = repos();
    let collection = repos.collections.create("Shop").expect("collection");
    repos
        .grpc
        .save(&saved("req_g1", &collection.id, "Get", GrpcSchemaRef::None))
        .expect("save");

    repos.collections.delete(&collection.id).expect("delete");

    assert!(matches!(
        repos.grpc.get("req_g1"),
        Err(AppError::NotFound(_))
    ));
}

/// What stops a schema from being deleted while a request uses it.
#[test]
fn a_saved_request_is_a_user_of_its_library_schema() {
    let repos = repos();
    let collection = repos.collections.create("Shop").expect("collection");
    library_schema(&repos, "proto_1");
    repos
        .grpc
        .save(&saved(
            "req_g1",
            &collection.id,
            "Get order",
            GrpcSchemaRef::Library("proto_1".into()),
        ))
        .expect("save");
    repos
        .grpc
        .save(&saved(
            "req_g2",
            &collection.id,
            "Live",
            GrpcSchemaRef::Reflection,
        ))
        .expect("save");

    assert_eq!(
        repos.schemas.users("proto_1").expect("users"),
        ["Get order"]
    );
}
