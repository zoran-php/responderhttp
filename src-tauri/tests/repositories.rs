// http_client/src-tauri/tests/repositories.rs
//
// Repository tests against an in-memory SQLite database running the real
// migrations (CLAUDE.md section 8).
use std::sync::Arc;
use std::time::Duration;

use responderhttp_lib::domain::error::AppError;
use responderhttp_lib::domain::models::{
    ApiKeyLocation, Auth, Cookie, EnvironmentVariable, HttpMethod, HttpRequest,
    HttpVersionPreference, KeyValue, NewExample, NewHistoryEntry, RequestBody, RequestSettings,
    SavedRequest, TlsMinimum,
};
use responderhttp_lib::domain::ports::{
    CollectionRepository, CookieRepository, EnvironmentRepository, ExampleRepository,
    FolderRepository, HistoryRepository, SavedRequestRepository, SecretCipher,
};
use responderhttp_lib::domain::secrets::SecretState;
use responderhttp_lib::persistence::database::Database;
use responderhttp_lib::persistence::repositories::collections::SqliteCollectionRepository;
use responderhttp_lib::persistence::repositories::cookies::SqliteCookieRepository;
use responderhttp_lib::persistence::repositories::environments::SqliteEnvironmentRepository;
use responderhttp_lib::persistence::repositories::examples::SqliteExampleRepository;
use responderhttp_lib::persistence::repositories::folders::SqliteFolderRepository;
use responderhttp_lib::persistence::repositories::history::SqliteHistoryRepository;
use responderhttp_lib::persistence::repositories::saved_requests::SqliteSavedRequestRepository;
use responderhttp_lib::secrets::memory::MemoryDataKeyStore;
use responderhttp_lib::secrets::open_cipher;

struct Repos {
    database: Database,
    cipher: Arc<dyn SecretCipher>,
    collections: SqliteCollectionRepository,
    folders: SqliteFolderRepository,
    requests: SqliteSavedRequestRepository,
    environments: SqliteEnvironmentRepository,
    cookies: SqliteCookieRepository,
    history: SqliteHistoryRepository,
    examples: SqliteExampleRepository,
}

fn repos() -> Repos {
    let database = Database::open_in_memory().expect("in-memory database should open");
    repos_on(database, test_cipher())
}

/// A real envelope cipher over an in-memory key store: the tests never touch
/// the OS credential store (CLAUDE.md section 8).
fn test_cipher() -> Arc<dyn SecretCipher> {
    open_cipher(&MemoryDataKeyStore::empty()).0
}

fn repos_on(database: Database, cipher: Arc<dyn SecretCipher>) -> Repos {
    Repos {
        database: database.clone(),
        cipher: cipher.clone(),
        collections: SqliteCollectionRepository::new(database.clone()),
        folders: SqliteFolderRepository::new(database.clone()),
        requests: SqliteSavedRequestRepository::new(database.clone(), cipher.clone()),
        environments: SqliteEnvironmentRepository::new(database.clone(), cipher),
        cookies: SqliteCookieRepository::new(database.clone()),
        history: SqliteHistoryRepository::new(database.clone()),
        examples: SqliteExampleRepository::new(database),
    }
}

/// Deliberately exercises every JSON column: headers, params, body, auth, settings.
fn rich_request() -> HttpRequest {
    HttpRequest {
        method: HttpMethod::Post,
        url: "https://api.example.com/users".into(),
        headers: vec![KeyValue::new("X-Api-Key", "secret")],
        query_params: vec![KeyValue::new("q", "a b&c")],
        body: RequestBody::Raw {
            content_type: "application/json".into(),
            text: "{\"a\":1}".into(),
        },
        auth: Auth::ApiKey {
            key: "X-Api-Key".into(),
            value: "secret".into(),
            location: ApiKeyLocation::Query,
        },
        // Spread from Default rather than listed exhaustively: every new
        // transport setting would otherwise break this fixture, and what the
        // round-trip test cares about is that non-default values survive.
        settings: RequestSettings {
            follow_redirects: false,
            max_redirects: 3,
            timeout: Duration::from_millis(1500),
            verify_tls: false,
            proxy: Some("http://127.0.0.1:8080".into()),
            send_cookies: false,
            http_version: HttpVersionPreference::Http11,
            keep_method_on_redirect: true,
            keep_auth_on_redirect: true,
            encode_url: false,
            allow_http_09: true,
            tls_minimum: TlsMinimum::Tls13,
        },
    }
}

#[test]
fn a_saved_request_round_trips_through_every_json_column() {
    let repos = repos();
    let collection = repos.collections.create("Work").expect("should create");
    let saved = SavedRequest {
        id: "req_1".into(),
        collection_id: collection.id.clone(),
        folder_id: None,
        name: "Create user".into(),
        request: rich_request(),
        secret_state: SecretState::Ok,
    };

    repos.requests.save(&saved).expect("should save");
    let loaded = repos.requests.get("req_1").expect("should load");

    assert_eq!(loaded.name, "Create user");
    assert_eq!(loaded.request.method, HttpMethod::Post);
    assert_eq!(loaded.request.headers, rich_request().headers);
    assert_eq!(loaded.request.query_params, rich_request().query_params);
    assert_eq!(loaded.request.body, rich_request().body);
    // A saved request keeps its secret: sealed on disk, opened on load.
    assert_eq!(loaded.request.auth, rich_request().auth);
    assert_eq!(loaded.secret_state, SecretState::Ok);
    assert_eq!(loaded.request.settings, rich_request().settings);
}

#[test]
fn saving_the_same_id_updates_rather_than_duplicating() {
    let repos = repos();
    let collection = repos.collections.create("Work").expect("should create");
    let mut saved = SavedRequest {
        id: "req_1".into(),
        collection_id: collection.id.clone(),
        folder_id: None,
        name: "First name".into(),
        request: rich_request(),
        secret_state: SecretState::Ok,
    };
    repos.requests.save(&saved).expect("should save");

    saved.name = "Second name".into();
    repos.requests.save(&saved).expect("should re-save");

    let all = repos
        .requests
        .list_by_collection(&collection.id)
        .expect("should list");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "Second name");
}

#[test]
fn deleting_a_collection_cascades_to_its_folders_and_requests() {
    let repos = repos();
    let collection = repos.collections.create("Work").expect("should create");
    let folder = repos
        .folders
        .create(&collection.id, None, "Users")
        .expect("should create folder");
    repos
        .requests
        .save(&SavedRequest {
            id: "req_1".into(),
            collection_id: collection.id.clone(),
            folder_id: Some(folder.id.clone()),
            name: "Create user".into(),
            request: rich_request(),
            secret_state: SecretState::Ok,
        })
        .expect("should save");

    repos
        .collections
        .delete(&collection.id)
        .expect("should delete");

    assert!(repos
        .folders
        .list_by_collection(&collection.id)
        .expect("should list")
        .is_empty());
    assert!(repos
        .requests
        .list_by_collection(&collection.id)
        .expect("should list")
        .is_empty());
}

#[test]
fn deleting_a_folder_cascades_to_the_requests_inside_it() {
    let repos = repos();
    let collection = repos.collections.create("Work").expect("should create");
    let folder = repos
        .folders
        .create(&collection.id, None, "Users")
        .expect("should create folder");
    repos
        .requests
        .save(&SavedRequest {
            id: "req_1".into(),
            collection_id: collection.id.clone(),
            folder_id: Some(folder.id.clone()),
            name: "Create user".into(),
            request: rich_request(),
            secret_state: SecretState::Ok,
        })
        .expect("should save");

    repos.folders.delete(&folder.id).expect("should delete");

    assert!(repos
        .requests
        .list_by_collection(&collection.id)
        .expect("should list")
        .is_empty());
}

#[test]
fn a_request_moves_between_a_folder_and_the_collection_root() {
    let repos = repos();
    let collection = repos.collections.create("Work").expect("should create");
    let folder = repos
        .folders
        .create(&collection.id, None, "Users")
        .expect("should create folder");
    repos
        .requests
        .save(&SavedRequest {
            id: "req_1".into(),
            collection_id: collection.id.clone(),
            folder_id: None,
            name: "Create user".into(),
            request: rich_request(),
            secret_state: SecretState::Ok,
        })
        .expect("should save");

    repos
        .requests
        .move_to("req_1", Some(&folder.id))
        .expect("should move into the folder");
    assert_eq!(
        repos.requests.get("req_1").expect("should load").folder_id,
        Some(folder.id)
    );

    repos
        .requests
        .move_to("req_1", None)
        .expect("should move back to the root");
    assert_eq!(
        repos.requests.get("req_1").expect("should load").folder_id,
        None
    );
}

#[test]
fn renaming_reaches_storage_and_listing_is_case_insensitive_by_name() {
    let repos = repos();
    repos.collections.create("zebra").expect("should create");
    let alpha = repos.collections.create("Alpha").expect("should create");

    repos
        .collections
        .rename(&alpha.id, "  Beta  ")
        .expect("should rename");

    let names: Vec<String> = repos
        .collections
        .list()
        .expect("should list")
        .into_iter()
        .map(|collection| collection.name)
        .collect();
    assert_eq!(names, vec!["Beta".to_string(), "zebra".to_string()]);
}

#[test]
fn operating_on_an_unknown_id_reports_not_found() {
    let repos = repos();

    let rename = repos.collections.rename("col_missing", "New name");
    let delete = repos.requests.delete("req_missing");
    let load = repos.requests.get("req_missing");

    assert!(matches!(rename, Err(AppError::NotFound(_))));
    assert!(matches!(delete, Err(AppError::NotFound(_))));
    assert!(matches!(load, Err(AppError::NotFound(_))));
}

#[test]
fn environment_variables_round_trip_in_the_order_they_were_written() {
    let repos = repos();
    let environment = repos.environments.create("Staging").expect("should create");

    repos
        .environments
        .set_variables(
            &environment.id,
            &[
                EnvironmentVariable::plain("base_url", "https://staging.example.com"),
                EnvironmentVariable::plain("token", "abc123"),
            ],
        )
        .expect("should write");

    let loaded = repos
        .environments
        .variables(&environment.id)
        .expect("should read");
    assert_eq!(
        loaded,
        vec![
            EnvironmentVariable::plain("base_url", "https://staging.example.com"),
            EnvironmentVariable::plain("token", "abc123"),
        ]
    );
}

#[test]
fn writing_variables_replaces_the_previous_set_rather_than_appending() {
    let repos = repos();
    let environment = repos.environments.create("Staging").expect("should create");

    repos
        .environments
        .set_variables(&environment.id, &[EnvironmentVariable::plain("a", "1")])
        .expect("should write");
    repos
        .environments
        .set_variables(&environment.id, &[EnvironmentVariable::plain("b", "2")])
        .expect("should rewrite");

    let loaded = repos
        .environments
        .variables(&environment.id)
        .expect("should read");
    assert_eq!(loaded, vec![EnvironmentVariable::plain("b", "2")]);
}

#[test]
fn a_variable_with_a_blank_name_is_dropped_rather_than_stored() {
    let repos = repos();
    let environment = repos.environments.create("Staging").expect("should create");

    repos
        .environments
        .set_variables(
            &environment.id,
            &[
                EnvironmentVariable::plain("  ", "ignored"),
                EnvironmentVariable::plain(" kept ", "yes"),
            ],
        )
        .expect("should write");

    let loaded = repos
        .environments
        .variables(&environment.id)
        .expect("should read");
    assert_eq!(loaded, vec![EnvironmentVariable::plain("kept", "yes")]);
}

#[test]
fn deleting_an_environment_cascades_to_its_variables() {
    let repos = repos();
    let environment = repos.environments.create("Staging").expect("should create");
    repos
        .environments
        .set_variables(&environment.id, &[EnvironmentVariable::plain("a", "1")])
        .expect("should write");

    repos
        .environments
        .delete(&environment.id)
        .expect("should delete");

    // The rows are gone with the parent, so writing to the dead id is a
    // NotFound rather than a foreign-key error.
    let error = repos
        .environments
        .set_variables(&environment.id, &[])
        .expect_err("should not find the environment");
    assert!(matches!(error, AppError::NotFound(_)));
}

#[test]
fn renaming_an_environment_that_does_not_exist_is_not_found() {
    let repos = repos();

    let error = repos
        .environments
        .rename("env_missing", "Whatever")
        .expect_err("should not find the environment");
    assert!(matches!(error, AppError::NotFound(_)));
}

fn cookie(name: &str, value: &str, expires_at: Option<u64>, created_at: u64) -> Cookie {
    Cookie {
        name: name.into(),
        value: value.into(),
        domain: "example.com".into(),
        path: "/".into(),
        expires_at,
        secure: false,
        http_only: false,
        host_only: true,
        created_at,
    }
}

#[test]
fn re_setting_a_cookie_replaces_it_and_keeps_its_original_creation_time() {
    let repos = repos();
    repos
        .cookies
        .upsert(&cookie("sid", "first", None, 100))
        .expect("should insert");
    repos
        .cookies
        .upsert(&cookie("sid", "second", None, 999))
        .expect("should replace");

    let stored = repos.cookies.list().expect("should list");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].value, "second");
    // RFC 6265 section 5.3 step 11: the creation time survives, because it
    // decides the order cookies are sent in.
    assert_eq!(stored[0].created_at, 100);
}

#[test]
fn clearing_the_session_removes_only_cookies_without_an_expiry() {
    let repos = repos();
    repos
        .cookies
        .upsert(&cookie("session", "a", None, 1))
        .expect("should insert");
    repos
        .cookies
        .upsert(&cookie("persistent", "b", Some(4_000_000_000), 2))
        .expect("should insert");

    repos.cookies.clear_session().expect("should clear session");

    let stored = repos.cookies.list().expect("should list");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].name, "persistent");
}

#[test]
fn purging_removes_cookies_already_past_their_expiry() {
    let repos = repos();
    repos
        .cookies
        .upsert(&cookie("stale", "a", Some(500), 1))
        .expect("should insert");
    repos
        .cookies
        .upsert(&cookie("fresh", "b", Some(5_000), 2))
        .expect("should insert");

    repos.cookies.purge_expired(1_000).expect("should purge");

    let stored = repos.cookies.list().expect("should list");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].name, "fresh");
}

#[test]
fn deleting_a_cookie_that_is_not_there_reports_not_found() {
    let repos = repos();

    let error = repos
        .cookies
        .delete("example.com", "/", "missing")
        .expect_err("should not find the cookie");
    assert!(matches!(error, AppError::NotFound(_)));
}

#[test]
fn clearing_empties_the_whole_jar() {
    let repos = repos();
    repos
        .cookies
        .upsert(&cookie("a", "1", None, 1))
        .expect("should insert");

    repos.cookies.clear().expect("should clear");

    assert!(repos.cookies.list().expect("should list").is_empty());
}

fn api_key_without_value() -> Auth {
    Auth::ApiKey {
        key: "X-Api-Key".into(),
        value: String::new(),
        location: ApiKeyLocation::Query,
    }
}

fn history_entry(resolved_url: &str, status: Option<u16>) -> NewHistoryEntry {
    NewHistoryEntry {
        resolved_url: resolved_url.into(),
        status,
        error_kind: if status.is_some() {
            None
        } else {
            Some("timeout".into())
        },
        duration_ms: 42,
        request: rich_request(),
    }
}

#[test]
fn a_history_entry_round_trips_with_its_request_template_intact() {
    let repos = repos();

    let stored = repos
        .history
        .record(
            &history_entry("https://api.example.com/users?q=a+b", Some(201)),
            10,
        )
        .expect("should record");

    let listed = repos.history.list(10).expect("should list");
    assert_eq!(listed.len(), 1);
    let entry = &listed[0];
    assert_eq!(entry.id, stored.id);
    assert_eq!(entry.sent_at, stored.sent_at);
    assert_eq!(entry.status, Some(201));
    assert_eq!(entry.error_kind, None);
    assert_eq!(entry.duration_ms, 42);
    assert_eq!(entry.request.url, rich_request().url);
    assert_eq!(entry.request.headers, rich_request().headers);
    assert_eq!(entry.request.body, rich_request().body);
    // History keeps no secrets: the key name survives, the value does not.
    assert_eq!(entry.request.auth, api_key_without_value());
    assert_eq!(entry.request.settings, rich_request().settings);
}

#[test]
fn a_failed_send_stores_its_error_and_no_status() {
    let repos = repos();

    repos
        .history
        .record(&history_entry("https://api.example.com/down", None), 10)
        .expect("should record");

    let listed = repos.history.list(10).expect("should list");
    assert_eq!(listed[0].status, None);
    assert_eq!(listed[0].error_kind.as_deref(), Some("timeout"));
}

/// The cap is what keeps old credentials from sitting on disk forever, so it
/// has to hold on insert rather than at some later sweep.
#[test]
fn recording_past_the_cap_drops_the_oldest_entries() {
    let repos = repos();

    for index in 0..5 {
        repos
            .history
            .record(
                &history_entry(&format!("https://example.com/{index}"), Some(200)),
                3,
            )
            .expect("should record");
    }

    let listed = repos.history.list(10).expect("should list");
    assert_eq!(listed.len(), 3);
    let urls: Vec<&str> = listed
        .iter()
        .map(|entry| entry.resolved_url.as_str())
        .collect();
    assert_eq!(
        urls,
        vec![
            "https://example.com/4",
            "https://example.com/3",
            "https://example.com/2"
        ]
    );
}

#[test]
fn listing_honours_its_own_limit_and_returns_the_newest_first() {
    let repos = repos();

    for index in 0..4 {
        repos
            .history
            .record(
                &history_entry(&format!("https://example.com/{index}"), Some(200)),
                100,
            )
            .expect("should record");
    }

    let listed = repos.history.list(2).expect("should list");
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].resolved_url, "https://example.com/3");
    assert_eq!(listed[1].resolved_url, "https://example.com/2");
}

#[test]
fn deleting_one_entry_leaves_the_rest_and_clearing_empties_the_list() {
    let repos = repos();
    let first = repos
        .history
        .record(&history_entry("https://example.com/a", Some(200)), 10)
        .expect("should record");
    repos
        .history
        .record(&history_entry("https://example.com/b", Some(200)), 10)
        .expect("should record");

    repos.history.delete(&first.id).expect("should delete");
    assert_eq!(repos.history.list(10).expect("should list").len(), 1);

    repos.history.clear().expect("should clear");
    assert!(repos.history.list(10).expect("should list").is_empty());
}

#[test]
fn deleting_a_history_entry_that_is_not_there_reports_not_found() {
    let repos = repos();

    let error = repos
        .history
        .delete("his_missing")
        .expect_err("should fail");

    assert!(matches!(error, AppError::NotFound(_)));
}

fn new_example(request_id: &str, name: &str, status: u16) -> NewExample {
    NewExample {
        request_id: request_id.into(),
        name: name.into(),
        request: rich_request(),
        status,
        response_headers: vec![KeyValue::new("Content-Type", "application/json")],
        response_body: "{\"ok\":true}".into(),
    }
}

/// A request to hang examples off, since every example needs one.
fn saved_request_in(repos: &Repos, name: &str) -> SavedRequest {
    let collection = repos.collections.create("Work").expect("should create");
    let saved = SavedRequest {
        id: format!("req_{name}"),
        collection_id: collection.id,
        folder_id: None,
        name: name.into(),
        request: rich_request(),
        secret_state: SecretState::Ok,
    };
    repos.requests.save(&saved).expect("should save");
    saved
}

#[test]
fn an_example_round_trips_with_its_request_snapshot_and_response() {
    let repos = repos();
    let request = saved_request_in(&repos, "list");

    let created = repos
        .examples
        .create(&new_example(&request.id, "Success", 200))
        .expect("should create");

    let loaded = repos.examples.get(&created.id).expect("should load");
    assert_eq!(loaded.name, "Success");
    assert_eq!(loaded.status, 200);
    assert_eq!(loaded.response_body, "{\"ok\":true}");
    assert_eq!(loaded.response_headers[0].name, "Content-Type");
    assert_eq!(loaded.request.url, rich_request().url);
    // Examples keep no secrets either.
    assert_eq!(loaded.request.auth, api_key_without_value());
    assert_eq!(loaded.request.settings, rich_request().settings);
}

/// The tree draws every example in a collection at once, so the summary
/// listing must not carry bodies with it.
#[test]
fn summaries_list_in_creation_order_across_the_whole_collection() {
    let repos = repos();
    let request = saved_request_in(&repos, "list");
    for (name, status) in [("Success", 200u16), ("Not found", 404), ("Broken", 500)] {
        repos
            .examples
            .create(&new_example(&request.id, name, status))
            .expect("should create");
    }

    let summaries = repos
        .examples
        .list_summaries_by_collection(&request.collection_id)
        .expect("should list");

    let names: Vec<&str> = summaries.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["Success", "Not found", "Broken"]);
    assert!(summaries.iter().all(|s| s.request_id == request.id));
}

#[test]
fn deleting_a_request_takes_its_examples_with_it() {
    let repos = repos();
    let request = saved_request_in(&repos, "list");
    let created = repos
        .examples
        .create(&new_example(&request.id, "Success", 200))
        .expect("should create");

    repos.requests.delete(&request.id).expect("should delete");

    assert!(matches!(
        repos.examples.get(&created.id).expect_err("should be gone"),
        AppError::NotFound(_)
    ));
}

/// Reported as NotFound rather than as an opaque foreign-key failure.
#[test]
fn saving_an_example_against_an_unknown_request_reports_not_found() {
    let repos = repos();

    let error = repos
        .examples
        .create(&new_example("req_missing", "Orphan", 200))
        .expect_err("should fail");

    assert!(matches!(error, AppError::NotFound(_)));
}

#[test]
fn renaming_an_example_reaches_storage_and_an_unknown_id_is_not_found() {
    let repos = repos();
    let request = saved_request_in(&repos, "list");
    let created = repos
        .examples
        .create(&new_example(&request.id, "Success", 200))
        .expect("should create");

    repos
        .examples
        .rename(&created.id, "Happy path")
        .expect("should rename");
    assert_eq!(
        repos.examples.get(&created.id).expect("should load").name,
        "Happy path"
    );

    assert!(matches!(
        repos
            .examples
            .rename("exa_missing", "Nope")
            .expect_err("should fail"),
        AppError::NotFound(_)
    ));
}

#[test]
fn deleting_one_example_leaves_the_others() {
    let repos = repos();
    let request = saved_request_in(&repos, "list");
    let first = repos
        .examples
        .create(&new_example(&request.id, "Success", 200))
        .expect("should create");
    repos
        .examples
        .create(&new_example(&request.id, "Not found", 404))
        .expect("should create");

    repos.examples.delete(&first.id).expect("should delete");

    let summaries = repos
        .examples
        .list_summaries_by_collection(&request.collection_id)
        .expect("should list");
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].name, "Not found");
}

// ---------------------------------------------------------------------------
// Phase 9: secrets encrypted at rest.
// ---------------------------------------------------------------------------

use responderhttp_lib::persistence::repositories::secret_upgrade::{
    upgrade_plaintext_secrets, UpgradeReport,
};
use responderhttp_lib::secrets::without_key;

const LIVE_TOKEN: &str = "live-token-7f3a9c";

fn bearer_request(token: &str) -> HttpRequest {
    HttpRequest {
        auth: Auth::Bearer {
            token: token.into(),
        },
        ..rich_request()
    }
}

fn save_bearer(repos: &Repos, id: &str, token: &str) -> SavedRequest {
    let collection = repos.collections.create("Work").expect("should create");
    let saved = SavedRequest {
        id: id.into(),
        collection_id: collection.id,
        folder_id: None,
        name: "Me".into(),
        request: bearer_request(token),
        secret_state: SecretState::Ok,
    };
    repos.requests.save(&saved).expect("should save");
    saved
}

fn raw_auth(database: &Database, table: &str, id: &str) -> String {
    database
        .lock()
        .query_row(
            &format!("SELECT auth_json FROM {table} WHERE id = ?1"),
            [id],
            |row| row.get(0),
        )
        .expect("row should exist")
}

#[test]
fn a_saved_auth_secret_is_sealed_in_the_database_and_opens_on_load() {
    let database = Database::open_in_memory().expect("should open");
    let repos = repos_on(database.clone(), test_cipher());

    save_bearer(&repos, "req_1", LIVE_TOKEN);

    assert!(!raw_auth(&database, "requests", "req_1").contains(LIVE_TOKEN));
    let loaded = repos.requests.get("req_1").expect("should load");
    assert_eq!(
        loaded.request.auth,
        Auth::Bearer {
            token: LIVE_TOKEN.into()
        }
    );
    assert_eq!(loaded.secret_state, SecretState::Ok);
}

#[test]
fn a_request_whose_key_is_gone_still_loads_and_asks_for_its_secret_again() {
    let database = Database::open_in_memory().expect("should open");
    save_bearer(
        &repos_on(database.clone(), test_cipher()),
        "req_1",
        LIVE_TOKEN,
    );

    // A different key, as after the Credential Manager entry was deleted.
    let later = repos_on(database, test_cipher());
    let loaded = later.requests.get("req_1").expect("should still load");

    assert_eq!(loaded.secret_state, SecretState::NeedsReentry);
    assert_eq!(
        loaded.request.auth,
        Auth::Bearer {
            token: String::new()
        }
    );
    assert_eq!(loaded.request.url, rich_request().url);
}

#[test]
fn without_a_usable_key_a_secret_is_neither_saved_nor_erased() {
    let database = Database::open_in_memory().expect("should open");
    let saved = save_bearer(
        &repos_on(database.clone(), test_cipher()),
        "req_1",
        LIVE_TOKEN,
    );
    let before = raw_auth(&database, "requests", "req_1");
    let locked = repos_on(database.clone(), without_key("locked".into()).0);

    let loaded = locked.requests.get("req_1").expect("should still load");
    assert_eq!(loaded.secret_state, SecretState::Unavailable);

    // Saving what was loaded — a blank token — must not overwrite the
    // sealed one that could not be read.
    let blank = SavedRequest {
        request: bearer_request(""),
        ..saved.clone()
    };
    let error = locked.requests.save(&blank).expect_err("must refuse");
    assert!(matches!(error, AppError::SecretStore(_)));
    let typed = SavedRequest {
        request: bearer_request("new"),
        ..saved
    };
    assert!(locked.requests.save(&typed).is_err());
    assert_eq!(raw_auth(&database, "requests", "req_1"), before);
}

#[test]
fn a_request_without_a_secret_saves_even_without_a_key() {
    let database = Database::open_in_memory().expect("should open");
    let locked = repos_on(database, without_key("locked".into()).0);
    let collection = locked.collections.create("Work").expect("should create");

    locked
        .requests
        .save(&SavedRequest {
            id: "req_1".into(),
            collection_id: collection.id,
            folder_id: None,
            name: "Open".into(),
            request: HttpRequest {
                auth: Auth::None,
                ..rich_request()
            },
            secret_state: SecretState::Ok,
        })
        .expect("nothing here needs a key");
}

#[test]
fn history_and_examples_never_store_a_literal_secret() {
    let database = Database::open_in_memory().expect("should open");
    let repos = repos_on(database.clone(), test_cipher());
    let request = save_bearer(&repos, "req_1", LIVE_TOKEN);

    let entry = repos
        .history
        .record(
            &NewHistoryEntry {
                request: bearer_request(LIVE_TOKEN),
                ..history_entry("https://api.example.com/me", Some(200))
            },
            10,
        )
        .expect("should record");
    let example = repos
        .examples
        .create(&NewExample {
            request: bearer_request(LIVE_TOKEN),
            ..new_example(&request.id, "Me", 200)
        })
        .expect("should create");

    assert!(!raw_auth(&database, "history", &entry.id).contains(LIVE_TOKEN));
    assert!(!raw_auth(&database, "examples", &example.id).contains(LIVE_TOKEN));
}

#[test]
fn history_keeps_a_placeholder_so_a_rerun_resolves_it() {
    let repos = repos();

    repos
        .history
        .record(
            &NewHistoryEntry {
                request: bearer_request("{{token}}"),
                ..history_entry("https://api.example.com/me", Some(200))
            },
            10,
        )
        .expect("should record");

    let listed = repos.history.list(10).expect("should list");
    assert_eq!(
        listed[0].request.auth,
        Auth::Bearer {
            token: "{{token}}".into()
        }
    );
}

#[test]
fn a_secret_variable_is_sealed_at_rest_and_a_plain_one_is_not() {
    let database = Database::open_in_memory().expect("should open");
    let repos = repos_on(database.clone(), test_cipher());
    let environment = repos.environments.create("Prod").expect("should create");

    repos
        .environments
        .set_variables(
            &environment.id,
            &[
                EnvironmentVariable::plain("base_url", "https://api.example.com"),
                EnvironmentVariable::secret("token", LIVE_TOKEN),
                EnvironmentVariable::secret("empty", ""),
            ],
        )
        .expect("should write");

    let stored: Vec<(String, Option<Vec<u8>>)> = {
        let guard = database.lock();
        let mut statement = guard
            .prepare("SELECT value, value_sealed FROM environment_variables ORDER BY position")
            .expect("should prepare");
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("should query");
        rows.collect::<Result<_, _>>().expect("should collect")
    };
    assert_eq!(stored[0], ("https://api.example.com".to_string(), None));
    assert_eq!(stored[1].0, "");
    let sealed = stored[1].1.as_ref().expect("the secret should be sealed");
    assert!(!sealed
        .windows(LIVE_TOKEN.len())
        .any(|window| window == LIVE_TOKEN.as_bytes()));
    assert_eq!(stored[2], (String::new(), None));

    let loaded = repos
        .environments
        .variables(&environment.id)
        .expect("should read");
    assert_eq!(
        loaded,
        vec![
            EnvironmentVariable::plain("base_url", "https://api.example.com"),
            EnvironmentVariable::secret("token", LIVE_TOKEN),
            EnvironmentVariable::secret("empty", ""),
        ]
    );
}

#[test]
fn a_secret_variable_whose_key_is_gone_loads_empty_and_flagged() {
    let database = Database::open_in_memory().expect("should open");
    let first = repos_on(database.clone(), test_cipher());
    let environment = first.environments.create("Prod").expect("should create");
    first
        .environments
        .set_variables(
            &environment.id,
            &[EnvironmentVariable::secret("token", LIVE_TOKEN)],
        )
        .expect("should write");

    let later = repos_on(database, test_cipher());
    let loaded = later
        .environments
        .variables(&environment.id)
        .expect("should read");

    assert_eq!(
        loaded,
        vec![EnvironmentVariable {
            name: "token".into(),
            value: String::new(),
            secret: true,
            state: SecretState::NeedsReentry,
        }]
    );
}

#[test]
fn without_a_usable_key_no_variable_set_containing_a_secret_is_written() {
    let database = Database::open_in_memory().expect("should open");
    let first = repos_on(database.clone(), test_cipher());
    let environment = first.environments.create("Prod").expect("should create");
    first
        .environments
        .set_variables(
            &environment.id,
            &[EnvironmentVariable::secret("token", LIVE_TOKEN)],
        )
        .expect("should write");
    let locked = repos_on(database, without_key("locked".into()).0);

    let error = locked
        .environments
        .set_variables(
            &environment.id,
            &[
                EnvironmentVariable::plain("base_url", "x"),
                EnvironmentVariable::secret("token", ""),
            ],
        )
        .expect_err("must refuse");

    assert!(matches!(error, AppError::SecretStore(_)));
    let still_there = first
        .environments
        .variables(&environment.id)
        .expect("should read");
    assert_eq!(
        still_there,
        vec![EnvironmentVariable::secret("token", LIVE_TOKEN)]
    );
}

/// Rows as a pre-Phase-9 build wrote them: auth secrets in plain JSON.
fn seed_legacy_rows(database: &Database) {
    let repos = repos_on(database.clone(), test_cipher());
    let request = save_bearer(&repos, "req_legacy", "placeholder");
    let entry = repos
        .history
        .record(&history_entry("https://api.example.com/me", Some(200)), 10)
        .expect("should record");
    let example = repos
        .examples
        .create(&new_example(&request.id, "Me", 200))
        .expect("should create");
    let plain = format!(r#"{{"kind":"Bearer","token":"{LIVE_TOKEN}"}}"#);
    let guard = database.lock();
    for (table, id) in [
        ("requests", request.id.as_str()),
        ("history", entry.id.as_str()),
        ("examples", example.id.as_str()),
    ] {
        guard
            .execute(
                &format!("UPDATE {table} SET auth_json = ?1 WHERE id = ?2"),
                [plain.as_str(), id],
            )
            .expect("should seed");
    }
}

#[test]
fn the_upgrade_seals_old_requests_and_strips_old_history_and_examples() {
    let database = Database::open_in_memory().expect("should open");
    seed_legacy_rows(&database);
    let cipher = test_cipher();

    let report = upgrade_plaintext_secrets(&database, cipher.as_ref()).expect("should upgrade");

    assert_eq!(report.sealed, 1);
    assert_eq!(report.stripped, 2);
    assert!(report.vacuumed);
    for table in ["requests", "history", "examples"] {
        let raw: Vec<String> = {
            let guard = database.lock();
            let mut statement = guard
                .prepare(&format!("SELECT auth_json FROM {table}"))
                .expect("should prepare");
            let rows = statement
                .query_map([], |row| row.get(0))
                .expect("should query");
            rows.collect::<Result<_, _>>().expect("should collect")
        };
        assert!(
            raw.iter().all(|auth| !auth.contains(LIVE_TOKEN)),
            "{table} still holds the token"
        );
    }
    let repos = repos_on(database, cipher);
    assert_eq!(
        repos
            .requests
            .get("req_legacy")
            .expect("should load")
            .request
            .auth,
        Auth::Bearer {
            token: LIVE_TOKEN.into()
        }
    );
}

#[test]
fn the_upgrade_is_safe_to_run_again() {
    let database = Database::open_in_memory().expect("should open");
    seed_legacy_rows(&database);
    let cipher = test_cipher();
    upgrade_plaintext_secrets(&database, cipher.as_ref()).expect("should upgrade");
    let sealed_once = raw_auth(&database, "requests", "req_legacy");

    let second = upgrade_plaintext_secrets(&database, cipher.as_ref()).expect("should run");

    assert_eq!(second, UpgradeReport::default());
    assert_eq!(raw_auth(&database, "requests", "req_legacy"), sealed_once);
}

#[test]
fn without_a_key_the_upgrade_leaves_requests_for_later_but_still_strips_history() {
    let database = Database::open_in_memory().expect("should open");
    seed_legacy_rows(&database);
    let (locked, _) = without_key("locked".into());

    let report = upgrade_plaintext_secrets(&database, locked.as_ref()).expect("should run");

    assert_eq!(report.sealed, 0);
    assert_eq!(report.waiting_for_key, 1);
    assert_eq!(report.stripped, 2);
    // Kept as it was: blanking it would lose it for good.
    assert!(raw_auth(&database, "requests", "req_legacy").contains(LIVE_TOKEN));
}

/// The Done-when of Phase 9 as a test: after the upgrade, the token is
/// nowhere in the database file — not in a live row, not in a free page,
/// not in a journal. The only test here that checks the goal rather than
/// the mechanism.
#[test]
fn after_the_upgrade_the_token_is_nowhere_in_the_database_file() {
    let directory = std::env::temp_dir().join(format!(
        "responderhttp-secrets-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after 1970")
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).expect("should create a temp directory");
    let path = directory.join("responderhttp.sqlite3");

    {
        let database = Database::open(&path).expect("should open a file database");
        seed_legacy_rows(&database);
        // A second copy of the token in a row that is then deleted, which is
        // what leaves plain text in free pages.
        let repos = repos_on(database.clone(), test_cipher());
        let extra = repos
            .history
            .record(&history_entry("https://api.example.com/x", Some(200)), 10)
            .expect("should record");
        database
            .lock()
            .execute(
                "UPDATE history SET auth_json = ?1 WHERE id = ?2",
                [
                    format!(r#"{{"kind":"Bearer","token":"{LIVE_TOKEN}"}}"#).as_str(),
                    extra.id.as_str(),
                ],
            )
            .expect("should seed");
        repos.history.delete(&extra.id).expect("should delete");

        upgrade_plaintext_secrets(&database, test_cipher().as_ref()).expect("should upgrade");
    }

    let mut found = Vec::new();
    for entry in std::fs::read_dir(&directory).expect("should list") {
        let file = entry.expect("should read entry").path();
        let bytes = std::fs::read(&file).expect("should read file");
        if bytes
            .windows(LIVE_TOKEN.len())
            .any(|window| window == LIVE_TOKEN.as_bytes())
        {
            found.push(file);
        }
    }
    let _ = std::fs::remove_dir_all(&directory);
    assert!(found.is_empty(), "token found in {found:?}");
}

// ---------------------------------------------------------------------------
// Phase 8e: OpenAPI export as JSON or YAML, through the real service.
// ---------------------------------------------------------------------------

use responderhttp_lib::domain::services::openapi::OpenApiExport;
use responderhttp_lib::openapi::document::OpenApiVersion;
use responderhttp_lib::openapi::format::ExportFormat;

/// A collection shaped like a real one: a folder, a templated URL, bearer
/// auth, a JSON body, and a saved response whose body has a status-code-like
/// key and YAML's favourite trap words.
fn exportable_collection(repos: &Repos) -> (OpenApiExport, String) {
    let collection = repos.collections.create("Work API").expect("should create");
    let folder = repos
        .folders
        .create(&collection.id, None, "Users")
        .expect("should create folder");
    let request = HttpRequest {
        method: HttpMethod::Post,
        url: "{{base_url}}/users/{{userId}}?active=yes".into(),
        headers: vec![KeyValue::new("Accept", "application/json")],
        query_params: vec![KeyValue::new("limit", "007")],
        body: RequestBody::Raw {
            content_type: "application/json".into(),
            text: "{\n  \"name\": \"no\",\n  \"version\": \"3.0\"\n}".into(),
        },
        auth: Auth::Bearer {
            token: "{{token}}".into(),
        },
        settings: RequestSettings::default(),
    };
    let saved = SavedRequest {
        id: "req_users".into(),
        collection_id: collection.id.clone(),
        folder_id: Some(folder.id),
        name: "Update user".into(),
        request: request.clone(),
        secret_state: SecretState::Ok,
    };
    repos.requests.save(&saved).expect("should save");
    repos
        .examples
        .create(&NewExample {
            request_id: saved.id.clone(),
            name: "Updated".into(),
            request,
            status: 200,
            response_headers: vec![KeyValue::new("Content-Type", "application/json")],
            response_body: "{\"200\": \"on\", \"id\": 7, \"tags\": [\"null\", \"~\"]}".into(),
        })
        .expect("should create example");

    (
        OpenApiExport::new(
            Arc::new(SqliteCollectionRepository::new(repos.database.clone())),
            Arc::new(SqliteFolderRepository::new(repos.database.clone())),
            Arc::new(SqliteSavedRequestRepository::new(
                repos.database.clone(),
                repos.cipher.clone(),
            )),
            Arc::new(SqliteExampleRepository::new(repos.database.clone())),
        ),
        collection.id,
    )
}

#[test]
fn a_yaml_export_describes_exactly_what_the_json_export_does() {
    let repos = repos();
    let (service, collection_id) = exportable_collection(&repos);

    for version in [
        OpenApiVersion::V3_0,
        OpenApiVersion::V3_1,
        OpenApiVersion::V3_2,
    ] {
        let json = service
            .export(&collection_id, version, ExportFormat::Json, true)
            .expect("JSON export should succeed");
        let yaml = service
            .export(&collection_id, version, ExportFormat::Yaml, true)
            .expect("YAML export should succeed");

        let from_json: serde_json::Value =
            serde_json::from_str(&json.text).expect("JSON should parse");
        let from_yaml: serde_json::Value =
            serde_saphyr::from_str(&yaml.text).expect("YAML should parse");
        assert_eq!(from_yaml, from_json, "{}\n{}", version.label(), yaml.text);
        assert_eq!(yaml.notes.len(), json.notes.len());
        assert!(json.file_name.ends_with(".json"), "{}", json.file_name);
        assert!(yaml.file_name.ends_with(".yaml"), "{}", yaml.file_name);
        assert_eq!(
            json.file_name.trim_end_matches(".json"),
            yaml.file_name.trim_end_matches(".yaml")
        );

        if let Ok(directory) = std::env::var("RESPONDERHTTP_DUMP_EXPORTS") {
            let label = version.label();
            std::fs::write(format!("{directory}/export-{label}.yaml"), &yaml.text)
                .expect("should dump");
            std::fs::write(format!("{directory}/export-{label}.json"), &json.text)
                .expect("should dump");
        }
    }
}

#[test]
fn a_yaml_export_keeps_json_the_default_output_unchanged() {
    let repos = repos();
    let (service, collection_id) = exportable_collection(&repos);

    let json = service
        .export(
            &collection_id,
            OpenApiVersion::V3_2,
            ExportFormat::Json,
            false,
        )
        .expect("should export");

    assert!(
        json.text.starts_with("{\n  \"openapi\": \"3.2.0\""),
        "{}",
        json.text
    );
}

// ---------------------------------------------------------------------------
// Phase 8d: OpenAPI import, written in one transaction.
// ---------------------------------------------------------------------------

use responderhttp_lib::domain::import_plan::{
    ImportPlan, PlannedEnvironment, PlannedExample, PlannedFolder, PlannedRequest,
};
use responderhttp_lib::domain::ports::ImportRepository;
use responderhttp_lib::domain::services::openapi_import::check;
use responderhttp_lib::openapi::import::grouping::Grouping;
use responderhttp_lib::openapi::import::to_collection::{to_plan, ImportOptions};
use responderhttp_lib::persistence::repositories::import::SqliteImportRepository;

fn importer(repos: &Repos) -> SqliteImportRepository {
    SqliteImportRepository::new(repos.database.clone(), repos.cipher.clone())
}

fn planned_request(name: &str, folder: Option<usize>, auth: Auth) -> PlannedRequest {
    PlannedRequest {
        name: name.into(),
        docs: String::new(),
        folder,
        request: HttpRequest {
            method: HttpMethod::Get,
            url: "{{baseUrl}}/pets".into(),
            headers: vec![KeyValue::new("X-Trace", "abc")],
            query_params: Vec::new(),
            body: RequestBody::None,
            auth,
            settings: RequestSettings::default(),
        },
        examples: Vec::new(),
    }
}

fn row_count(database: &Database, table: &str) -> i64 {
    database
        .lock()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .expect("should count")
}

#[test]
fn an_import_plan_is_written_with_nesting_examples_and_environment() {
    let repos = repos();
    let mut listed = planned_request(
        "List pets",
        Some(1),
        Auth::Bearer {
            token: "{{token}}".into(),
        },
    );
    listed.examples.push(PlannedExample {
        name: "Two pets".into(),
        status: 200,
        response_headers: vec![KeyValue::new("Content-Type", "application/json")],
        response_body: "[1, 2]".into(),
    });
    let plan = ImportPlan {
        collection_name: "Pet Store".into(),
        collection_docs: String::new(),
        folders: vec![
            PlannedFolder {
                name: "store".into(),
                parent: None,
                docs: String::new(),
            },
            PlannedFolder {
                name: "pets".into(),
                parent: Some(0),
                docs: String::new(),
            },
        ],
        requests: vec![listed, planned_request("Health", None, Auth::None)],
        environment: Some(PlannedEnvironment {
            name: "Pet Store".into(),
            variables: vec![
                EnvironmentVariable::plain("baseUrl", "https://api.example.com"),
                EnvironmentVariable::secret("token", ""),
            ],
        }),
    };

    let ids = importer(&repos).import(&plan).expect("should import");

    let collections = repos.collections.list().expect("should list");
    assert_eq!(collections.len(), 1);
    assert_eq!(collections[0].id, ids.collection_id);
    assert_eq!(collections[0].name, "Pet Store");

    let folders = repos
        .folders
        .list_by_collection(&ids.collection_id)
        .expect("should list folders");
    let store = folders.iter().find(|f| f.name == "store").expect("store");
    let pets = folders.iter().find(|f| f.name == "pets").expect("pets");
    assert_eq!(store.parent_folder_id, None);
    assert_eq!(pets.parent_folder_id.as_deref(), Some(store.id.as_str()));

    let requests = repos
        .requests
        .list_by_collection(&ids.collection_id)
        .expect("should list requests");
    let list = requests
        .iter()
        .find(|r| r.name == "List pets")
        .expect("list");
    assert_eq!(list.folder_id.as_deref(), Some(pets.id.as_str()));
    assert_eq!(
        list.request.auth,
        Auth::Bearer {
            token: "{{token}}".into()
        }
    );
    assert_eq!(list.secret_state, SecretState::Ok);
    let health = requests
        .iter()
        .find(|r| r.name == "Health")
        .expect("health");
    assert_eq!(health.folder_id, None);

    let examples = repos
        .examples
        .list_summaries_by_collection(&ids.collection_id)
        .expect("should list examples");
    assert_eq!(examples.len(), 1);
    assert_eq!(examples[0].request_id, list.id);
    assert_eq!(examples[0].status, 200);

    let environment_id = ids.environment_id.expect("an environment");
    let variables = repos
        .environments
        .variables(&environment_id)
        .expect("should load variables");
    assert_eq!(
        variables,
        vec![
            EnvironmentVariable::plain("baseUrl", "https://api.example.com"),
            EnvironmentVariable::secret("token", ""),
        ]
    );
}

#[test]
fn a_failure_part_way_through_leaves_nothing_behind() {
    let database = Database::open_in_memory().expect("should open");
    // No usable key: the second request's auth cannot be sealed, after the
    // collection, the folder and the first request are already written.
    let locked = repos_on(database.clone(), without_key("locked".into()).0);
    let plan = ImportPlan {
        collection_name: "Half".into(),
        collection_docs: String::new(),
        folders: vec![PlannedFolder {
            name: "f".into(),
            parent: None,
            docs: String::new(),
        }],
        requests: vec![
            planned_request("First", Some(0), Auth::None),
            planned_request(
                "Second",
                None,
                Auth::Bearer {
                    token: "{{token}}".into(),
                },
            ),
        ],
        environment: None,
    };

    let error = importer(&locked).import(&plan).expect_err("must fail");

    assert!(matches!(error, AppError::SecretStore(_)), "{error:?}");
    for table in [
        "collections",
        "folders",
        "requests",
        "examples",
        "environments",
    ] {
        assert_eq!(row_count(&database, table), 0, "{table}");
    }
}

#[test]
fn a_plan_with_a_child_before_its_parent_is_refused_before_writing() {
    let repos = repos();
    let plan = ImportPlan {
        collection_name: "Bad".into(),
        collection_docs: String::new(),
        folders: vec![PlannedFolder {
            name: "orphan".into(),
            parent: Some(3),
            docs: String::new(),
        }],
        requests: Vec::new(),
        environment: None,
    };

    assert!(matches!(
        importer(&repos).import(&plan),
        Err(AppError::Internal(_))
    ));
    assert_eq!(row_count(&repos.database, "collections"), 0);
}

/// A collection that survives export → import → export unchanged in every
/// version and syntax: everything in it is something OpenAPI can carry.
/// Markdown at each level, with the whitespace and punctuation that a
/// careless mapping would eat: a fenced block, a hard line break, a quote.
const COLLECTION_DOCS: &str = "# Round Trip\n\nThe whole API, \"quoted\".";
const FOLDER_DOCS: &str = "User management.\n\n- create\n- update";
const REQUEST_DOCS: &str =
    "Updates a user.\n\n```json\n{\"name\": \"x\"}\n```\n\nReturns 409 on a duplicate.";

fn round_trip_collection(repos: &Repos) -> String {
    let collection = repos.collections.create("Round Trip").expect("create");
    let folder = repos
        .folders
        .create(&collection.id, None, "Users")
        .expect("folder");
    let update = HttpRequest {
        method: HttpMethod::Put,
        url: "{{base_url}}/users/{{userId}}?active=yes".into(),
        headers: vec![KeyValue::new("X-Trace", "abc")],
        query_params: vec![KeyValue::new("limit", "007")],
        body: RequestBody::Raw {
            content_type: "application/json".into(),
            text: "{\n  \"name\": \"no\",\n  \"version\": \"3.0\"\n}".into(),
        },
        auth: Auth::Bearer {
            token: "{{token}}".into(),
        },
        settings: RequestSettings::default(),
    };
    let folder_id = folder.id.clone();
    let saved = SavedRequest {
        id: "req_update".into(),
        collection_id: collection.id.clone(),
        folder_id: Some(folder.id),
        name: "Update user".into(),
        request: update.clone(),
        secret_state: SecretState::Ok,
    };
    repos.requests.save(&saved).expect("save");
    // Documented at all three levels, so the round-trip below covers the
    // Phase 12 mapping as well — collection docs to info.description, folder
    // docs to the tag, request docs to the operation — rather than a second
    // near-identical round-trip test existing alongside this one.
    repos
        .collections
        .set_docs(&collection.id, COLLECTION_DOCS)
        .expect("collection docs");
    repos
        .folders
        .set_docs(&folder_id, FOLDER_DOCS)
        .expect("folder docs");
    repos
        .requests
        .set_docs(&saved.id, REQUEST_DOCS)
        .expect("request docs");
    repos
        .examples
        .create(&NewExample {
            request_id: saved.id.clone(),
            name: "Updated".into(),
            request: update,
            status: 200,
            response_headers: vec![KeyValue::new("Content-Type", "application/json")],
            response_body: "{\"200\": \"on\", \"id\": 7, \"tags\": [\"null\", \"~\"]}".into(),
        })
        .expect("example");
    repos
        .requests
        .save(&SavedRequest {
            id: "req_health".into(),
            collection_id: collection.id.clone(),
            folder_id: None,
            name: "Health".into(),
            request: HttpRequest {
                method: HttpMethod::Get,
                url: "{{base_url}}/health".into(),
                headers: Vec::new(),
                query_params: Vec::new(),
                body: RequestBody::None,
                auth: Auth::None,
                settings: RequestSettings::default(),
            },
            secret_state: SecretState::Ok,
        })
        .expect("save");
    collection.id
}

fn export_service(repos: &Repos) -> OpenApiExport {
    OpenApiExport::new(
        Arc::new(SqliteCollectionRepository::new(repos.database.clone())),
        Arc::new(SqliteFolderRepository::new(repos.database.clone())),
        Arc::new(SqliteSavedRequestRepository::new(
            repos.database.clone(),
            repos.cipher.clone(),
        )),
        Arc::new(SqliteExampleRepository::new(repos.database.clone())),
    )
}

fn parse_either(text: &str) -> serde_json::Value {
    serde_json::from_str(text)
        .or_else(|_| serde_saphyr::from_str(text))
        .expect("export should parse")
}

#[test]
fn export_then_import_then_export_changes_nothing() {
    for version in [
        OpenApiVersion::V3_0,
        OpenApiVersion::V3_1,
        OpenApiVersion::V3_2,
    ] {
        for format in [ExportFormat::Json, ExportFormat::Yaml] {
            let repos = repos();
            let original_id = round_trip_collection(&repos);
            let exporter = export_service(&repos);
            let first = exporter
                .export(&original_id, version, format, true)
                .expect("first export");

            // The export must pass the same checks an imported file does:
            // this is the official-schema conformance test for the exporter.
            let (detected, _) = check(first.text.as_bytes())
                .expect("schemas load")
                .unwrap_or_else(|refusal| {
                    panic!("{} export refused: {refusal:?}", version.label())
                });
            let mapped = to_plan(
                &detected.document,
                &ImportOptions {
                    grouping: Grouping::Tags,
                    include_examples: true,
                    create_environment: true,
                },
            );
            let ids = importer(&repos).import(&mapped.plan).expect("import");

            let second = exporter
                .export(&ids.collection_id, version, format, true)
                .expect("second export");

            assert_eq!(
                parse_either(&second.text),
                parse_either(&first.text),
                "{} {:?}\n--- first\n{}\n--- second\n{}",
                version.label(),
                format,
                first.text,
                second.text
            );
            assert_eq!(second.notes, first.notes);

            let variables = repos
                .environments
                .variables(&ids.environment_id.expect("environment"))
                .expect("variables");
            let names: Vec<(&str, bool)> = variables
                .iter()
                .map(|v| (v.name.as_str(), v.secret))
                .collect();
            assert_eq!(
                names,
                vec![("base_url", false), ("token", true), ("userId", false)]
            );
            assert!(variables.iter().all(|v| v.value.is_empty()));
        }
    }
}

/// Real-world specs are too large to commit. Point RESPONDERHTTP_BIG_SPEC at one
/// (the GitHub or Stripe description, say) and run with `--ignored`.
#[test]
#[ignore = "needs RESPONDERHTTP_BIG_SPEC=<path to a large OpenAPI file>"]
fn a_large_real_spec_imports_in_reasonable_time() {
    let Ok(path) = std::env::var("RESPONDERHTTP_BIG_SPEC") else {
        return;
    };
    let bytes = std::fs::read(&path).expect("should read the spec");
    let started = std::time::Instant::now();

    let (detected, _) = check(&bytes)
        .expect("schemas load")
        .unwrap_or_else(|refusal| panic!("refused: {refusal:?}"));
    let checked = started.elapsed();
    let mapped = to_plan(
        &detected.document,
        &ImportOptions {
            grouping: Grouping::Tags,
            include_examples: true,
            create_environment: true,
        },
    );
    let planned = started.elapsed();
    let repos = repos();
    let ids = importer(&repos).import(&mapped.plan).expect("import");
    let written = started.elapsed();

    let requests = repos
        .requests
        .list_by_collection(&ids.collection_id)
        .expect("list");
    assert_eq!(requests.len(), mapped.plan.requests.len());
    println!(
        "{path}: {} requests, {} folders, {} examples, {} notes; \
         checked {checked:?}, planned {planned:?}, written {written:?}",
        mapped.plan.requests.len(),
        mapped.plan.folders.len(),
        mapped.plan.example_count(),
        mapped.notes.len()
    );
    for note in &mapped.notes {
        println!("  {note:?}");
    }
    assert!(written < Duration::from_secs(120), "{written:?}");
}

// --- app settings (PLAN.md Phase 11) ------------------------------------

use responderhttp_lib::domain::ports::AppSettingsRepository;
use responderhttp_lib::domain::services::tray_notice::{TrayNotice, TRAY_NOTICE_DISMISSED};
use responderhttp_lib::persistence::repositories::app_settings::SqliteAppSettingsRepository;

fn settings() -> SqliteAppSettingsRepository {
    SqliteAppSettingsRepository::new(
        Database::open_in_memory().expect("in-memory database should open"),
    )
}

#[test]
fn an_unset_setting_reads_as_none() {
    assert_eq!(settings().get("nothing-here").expect("read"), None);
}

#[test]
fn a_setting_round_trips() {
    let settings = settings();

    settings.set("colour", "nord").expect("write");

    assert_eq!(
        settings.get("colour").expect("read"),
        Some("nord".to_string())
    );
}

/// The upsert is the part worth testing: a second write to the same key must
/// replace the value rather than fail the primary-key constraint.
#[test]
fn writing_a_setting_twice_replaces_it() {
    let settings = settings();

    settings.set("colour", "nord").expect("first write");
    settings.set("colour", "solarized").expect("second write");

    assert_eq!(
        settings.get("colour").expect("read"),
        Some("solarized".to_string())
    );
}

/// The whole close-to-tray preference, over the real table and the real
/// migrations rather than a mock.
#[test]
fn the_tray_notice_stops_after_it_is_dismissed() {
    let database = Database::open_in_memory().expect("in-memory database should open");
    let notice = TrayNotice::new(Arc::new(SqliteAppSettingsRepository::new(database.clone())));

    assert!(notice.should_show(), "a fresh install should see it");

    notice.never_show_again();

    assert!(!notice.should_show());
    assert_eq!(
        SqliteAppSettingsRepository::new(database)
            .get(TRAY_NOTICE_DISMISSED)
            .expect("read"),
        Some("true".to_string()),
        "the flag should survive in the table, not just in memory"
    );
}

// --- Item documentation (PLAN.md Phase 12) ---------------------------------

#[test]
fn documentation_round_trips_for_each_kind_of_item() {
    let repos = repos();
    let collection = repos.collections.create("Work").expect("should create");
    let folder = repos
        .folders
        .create(&collection.id, None, "Users")
        .expect("should create");
    let saved = SavedRequest {
        id: "req_1".into(),
        collection_id: collection.id.clone(),
        folder_id: Some(folder.id.clone()),
        name: "Create user".into(),
        request: rich_request(),
        secret_state: SecretState::Ok,
    };
    repos.requests.save(&saved).expect("should save");

    repos
        .collections
        .set_docs(&collection.id, "# Overview\n\nThe whole API.")
        .expect("should write collection docs");
    repos
        .folders
        .set_docs(&folder.id, "User management endpoints.")
        .expect("should write folder docs");
    repos
        .requests
        .set_docs("req_1", "Returns 409 when the email is taken.")
        .expect("should write request docs");

    assert_eq!(
        repos.collections.docs(&collection.id).expect("should read"),
        "# Overview\n\nThe whole API."
    );
    assert_eq!(
        repos.folders.docs(&folder.id).expect("should read"),
        "User management endpoints."
    );
    assert_eq!(
        repos.requests.docs("req_1").expect("should read"),
        "Returns 409 when the email is taken."
    );
}

/// The column is NOT NULL DEFAULT '', so an item that has never been
/// documented reads as empty rather than failing or returning a null.
#[test]
fn an_item_that_was_never_documented_reads_as_empty() {
    let repos = repos();
    let collection = repos.collections.create("Work").expect("should create");

    assert_eq!(
        repos.collections.docs(&collection.id).expect("should read"),
        ""
    );
}

/// Writing or reading an id that is not there is a not-found, not a silent
/// no-op — the same contract as every other update in this layer.
#[test]
fn documentation_for_an_unknown_id_is_not_found() {
    let repos = repos();

    assert!(matches!(
        repos.collections.docs("col_nope"),
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        repos.folders.set_docs("fld_nope", "text"),
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        repos.requests.docs("req_nope"),
        Err(AppError::NotFound(_))
    ));
}

/// Docs live on the item's own row, so the cascade that removes the item has
/// to take them with it. A polymorphic docs table was rejected for exactly
/// this reason (PLAN.md Phase 12) — this test is what proves the column got
/// the behaviour the table could not.
#[test]
fn deleting_a_collection_takes_its_documentation_with_it() {
    let repos = repos();
    let collection = repos.collections.create("Work").expect("should create");
    let folder = repos
        .folders
        .create(&collection.id, None, "Users")
        .expect("should create");
    repos
        .folders
        .set_docs(&folder.id, "Folder documentation.")
        .expect("should write");

    repos
        .collections
        .delete(&collection.id)
        .expect("should delete");

    assert!(matches!(
        repos.folders.docs(&folder.id),
        Err(AppError::NotFound(_))
    ));
}

/// **Regression test.** The requests upsert names its DO UPDATE SET columns
/// one by one and does not mention docs_md, so pressing Save in the request
/// builder leaves documentation alone. That is true by accident of how the
/// statement happens to be written, which is exactly the kind of thing a
/// later edit breaks without noticing.
#[test]
fn saving_a_request_does_not_wipe_its_documentation() {
    let repos = repos();
    let collection = repos.collections.create("Work").expect("should create");
    let mut saved = SavedRequest {
        id: "req_1".into(),
        collection_id: collection.id.clone(),
        folder_id: None,
        name: "Create user".into(),
        request: rich_request(),
        secret_state: SecretState::Ok,
    };
    repos.requests.save(&saved).expect("should save");
    repos
        .requests
        .set_docs("req_1", "Documentation that must survive a re-save.")
        .expect("should write docs");

    saved.name = "Renamed in the builder".into();
    repos.requests.save(&saved).expect("should re-save");

    assert_eq!(
        repos.requests.docs("req_1").expect("should read"),
        "Documentation that must survive a re-save."
    );
}

/// The spec's third acceptance scenario, end to end: document a collection,
/// export it, import it back, and read the documentation off the new items.
///
/// Distinct from `export_then_import_then_export_changes_nothing`, which
/// compares two documents. This one goes back to storage and asks the
/// repositories, which is what the user actually does when they reopen the
/// collection.
#[test]
fn documentation_survives_an_export_and_a_re_import() {
    let repos = repos();
    let original_id = round_trip_collection(&repos);
    let exported = export_service(&repos)
        .export(
            &original_id,
            OpenApiVersion::V3_1,
            ExportFormat::Yaml,
            false,
        )
        .expect("export");

    let (detected, _) = check(exported.text.as_bytes())
        .expect("schemas load")
        .unwrap_or_else(|refusal| panic!("export refused: {refusal:?}"));
    let mapped = to_plan(
        &detected.document,
        &ImportOptions {
            grouping: Grouping::Tags,
            include_examples: false,
            create_environment: false,
        },
    );
    let ids = importer(&repos).import(&mapped.plan).expect("import");

    assert_eq!(
        repos.collections.docs(&ids.collection_id).expect("read"),
        COLLECTION_DOCS
    );

    let folders = repos
        .folders
        .list_by_collection(&ids.collection_id)
        .expect("folders");
    assert_eq!(folders.len(), 1);
    assert_eq!(
        repos.folders.docs(&folders[0].id).expect("read"),
        FOLDER_DOCS
    );

    // The fixture has two requests and documents one of them, so this also
    // shows the documentation landing on the right row rather than on
    // whichever was written first.
    let requests = repos
        .requests
        .list_by_collection(&ids.collection_id)
        .expect("requests");
    let documented = requests
        .iter()
        .find(|request| request.name == "Update user")
        .expect("the documented request");
    let undocumented = requests
        .iter()
        .find(|request| request.name == "Health")
        .expect("the undocumented request");

    assert_eq!(
        repos.requests.docs(&documented.id).expect("read"),
        REQUEST_DOCS
    );
    assert_eq!(repos.requests.docs(&undocumented.id).expect("read"), "");
}

/// An undocumented collection must not come back from a round trip carrying
/// the provenance sentence the exporter writes into `info.description`.
#[test]
fn a_round_trip_does_not_invent_documentation_for_an_undocumented_collection() {
    let repos = repos();
    let collection = repos.collections.create("Plain").expect("create");
    repos
        .requests
        .save(&SavedRequest {
            id: "req_1".into(),
            collection_id: collection.id.clone(),
            folder_id: None,
            name: "List".into(),
            request: rich_request(),
            secret_state: SecretState::Ok,
        })
        .expect("save");

    let exported = export_service(&repos)
        .export(
            &collection.id,
            OpenApiVersion::V3_1,
            ExportFormat::Json,
            false,
        )
        .expect("export");
    let (detected, _) = check(exported.text.as_bytes())
        .expect("schemas load")
        .unwrap_or_else(|refusal| panic!("export refused: {refusal:?}"));
    let mapped = to_plan(
        &detected.document,
        &ImportOptions {
            grouping: Grouping::Tags,
            include_examples: false,
            create_environment: false,
        },
    );
    let ids = importer(&repos).import(&mapped.plan).expect("import");

    assert_eq!(
        repos.collections.docs(&ids.collection_id).expect("read"),
        ""
    );
}

// ---------------------------------------------------------------------------
// WebSocket requests (PLAN.md Phase 13d). They share the `requests` table with
// HTTP requests, so most of what is tested here is that the two kinds can live
// side by side without either one ever being read, overwritten or exported as
// the other.
// ---------------------------------------------------------------------------

use responderhttp_lib::domain::models::{
    SavedWebSocket, WebSocketRequest, WebSocketSettings, WsDraft, WsMessageFormat,
};
use responderhttp_lib::domain::ports::WebSocketRepository;
use responderhttp_lib::persistence::repositories::web_sockets::SqliteWebSocketRepository;

fn web_sockets(repos: &Repos) -> SqliteWebSocketRepository {
    SqliteWebSocketRepository::new(repos.database.clone())
}

/// Every field away from its default, and draft text outside Latin script.
fn rich_web_socket(id: &str, collection_id: &str, folder_id: Option<String>) -> SavedWebSocket {
    SavedWebSocket {
        id: id.into(),
        collection_id: collection_id.into(),
        folder_id,
        name: "Echo".into(),
        request: WebSocketRequest {
            url: "wss://echo.websocket.org/?room={{room}}".into(),
            headers: vec![
                KeyValue::new("Sec-WebSocket-Protocol", "chat"),
                KeyValue::new("X-Trace", "1"),
            ],
            settings: WebSocketSettings {
                verify_tls: false,
                proxy: Some("http://127.0.0.1:8080".into()),
                send_cookies: false,
                connect_timeout: Duration::from_millis(2500),
                max_message_bytes: 4096,
                auto_reconnect: true,
            },
        },
        draft: WsDraft {
            format: WsMessageFormat::Json,
            text: "{\"поздрав\": \"здраво\"}".into(),
            ..WsDraft::default()
        },
    }
}

fn http_request_in(repos: &Repos, id: &str, collection_id: &str) -> SavedRequest {
    let saved = SavedRequest {
        id: id.into(),
        collection_id: collection_id.into(),
        folder_id: None,
        name: "Create user".into(),
        request: rich_request(),
        secret_state: SecretState::Ok,
    };
    repos
        .requests
        .save(&saved)
        .expect("should save the HTTP request");
    saved
}

#[test]
fn a_web_socket_round_trips_through_every_field() {
    let repos = repos();
    let collection = repos.collections.create("Work").expect("should create");
    let folder = repos
        .folders
        .create(&collection.id, None, "Realtime")
        .expect("should create folder");
    let saved = rich_web_socket("req_ws", &collection.id, Some(folder.id));

    web_sockets(&repos).save(&saved).expect("should save");
    let loaded = web_sockets(&repos).get("req_ws").expect("should load");

    assert_eq!(loaded, saved);
}

/// The spec's second scenario, at the storage layer: one collection, both
/// kinds, and each listing sees exactly its own.
#[test]
fn a_collection_holds_both_kinds_and_each_listing_sees_only_its_own() {
    let repos = repos();
    let collection = repos
        .collections
        .create("Auth Service")
        .expect("should create");
    http_request_in(&repos, "req_http", &collection.id);
    web_sockets(&repos)
        .save(&rich_web_socket("req_ws", &collection.id, None))
        .expect("should save");

    let http = repos
        .requests
        .list_by_collection(&collection.id)
        .expect("should list HTTP");
    let ws = web_sockets(&repos)
        .list_by_collection(&collection.id)
        .expect("should list WebSockets");

    assert_eq!(http.len(), 1);
    assert_eq!(http[0].id, "req_http");
    assert_eq!(ws.len(), 1);
    assert_eq!(ws[0].id, "req_ws");
}

#[test]
fn loading_an_id_of_the_other_kind_is_not_found() {
    let repos = repos();
    let collection = repos.collections.create("Work").expect("should create");
    http_request_in(&repos, "req_http", &collection.id);
    web_sockets(&repos)
        .save(&rich_web_socket("req_ws", &collection.id, None))
        .expect("should save");

    assert!(matches!(
        repos
            .requests
            .get("req_ws")
            .expect_err("a WebSocket is not an HTTP request"),
        AppError::NotFound(_)
    ));
    assert!(matches!(
        web_sockets(&repos)
            .get("req_http")
            .expect_err("an HTTP request is not a WebSocket"),
        AppError::NotFound(_)
    ));
}

/// Overwriting would leave a row whose kind and columns disagree. Both
/// directions are refused and both rows survive untouched.
#[test]
fn neither_kind_can_be_saved_over_the_other() {
    let repos = repos();
    let collection = repos.collections.create("Work").expect("should create");
    let http = http_request_in(&repos, "req_http", &collection.id);
    let ws = rich_web_socket("req_ws", &collection.id, None);
    web_sockets(&repos).save(&ws).expect("should save");

    let http_over_ws = repos.requests.save(&SavedRequest {
        id: "req_ws".into(),
        ..http.clone()
    });
    let ws_over_http = web_sockets(&repos).save(&SavedWebSocket {
        id: "req_http".into(),
        ..ws.clone()
    });

    assert!(matches!(http_over_ws, Err(AppError::InvalidRequest(_))));
    assert!(matches!(ws_over_http, Err(AppError::InvalidRequest(_))));
    assert_eq!(web_sockets(&repos).get("req_ws").expect("still there"), ws);
    assert_eq!(
        repos
            .requests
            .get("req_http")
            .expect("still there")
            .request
            .url,
        http.request.url
    );
}

#[test]
fn saving_a_web_socket_again_updates_it_rather_than_duplicating() {
    let repos = repos();
    let collection = repos.collections.create("Work").expect("should create");
    let mut saved = rich_web_socket("req_ws", &collection.id, None);
    web_sockets(&repos).save(&saved).expect("should save");

    saved.name = "Echo (renamed by save)".into();
    saved.draft.text = "second draft".into();
    web_sockets(&repos).save(&saved).expect("should re-save");

    let all = web_sockets(&repos)
        .list_by_collection(&collection.id)
        .expect("should list");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0], saved);
}

/// The Phase 12 regression, repeated for the new write path: Save in the
/// builder must never wipe the documentation.
#[test]
fn saving_a_web_socket_again_leaves_its_docs_alone() {
    let repos = repos();
    let collection = repos.collections.create("Work").expect("should create");
    let saved = rich_web_socket("req_ws", &collection.id, None);
    web_sockets(&repos).save(&saved).expect("should save");
    repos
        .requests
        .set_docs("req_ws", "Echoes every frame back.")
        .expect("docs should reach a WebSocket by id");

    web_sockets(&repos).save(&saved).expect("should re-save");

    assert_eq!(
        repos.requests.docs("req_ws").expect("should read docs"),
        "Echoes every frame back."
    );
}

/// Rename, move and delete are not duplicated for WebSockets: they act on a
/// row by id through the request repository, whatever its kind.
#[test]
fn rename_move_and_delete_reach_a_web_socket_through_the_request_repository() {
    let repos = repos();
    let collection = repos.collections.create("Work").expect("should create");
    let folder = repos
        .folders
        .create(&collection.id, None, "Realtime")
        .expect("should create folder");
    web_sockets(&repos)
        .save(&rich_web_socket("req_ws", &collection.id, None))
        .expect("should save");

    repos
        .requests
        .rename("req_ws", "Live feed")
        .expect("should rename");
    repos
        .requests
        .move_to("req_ws", Some(&folder.id))
        .expect("should move");
    let moved = web_sockets(&repos).get("req_ws").expect("should load");
    assert_eq!(moved.name, "Live feed");
    assert_eq!(moved.folder_id, Some(folder.id));

    repos.requests.delete("req_ws").expect("should delete");
    assert!(matches!(
        web_sockets(&repos)
            .get("req_ws")
            .expect_err("should be gone"),
        AppError::NotFound(_)
    ));
}

#[test]
fn deleting_a_folder_or_a_collection_cascades_to_its_web_sockets() {
    let repos = repos();
    let collection = repos.collections.create("Work").expect("should create");
    let folder = repos
        .folders
        .create(&collection.id, None, "Realtime")
        .expect("should create folder");
    web_sockets(&repos)
        .save(&rich_web_socket(
            "req_in_folder",
            &collection.id,
            Some(folder.id.clone()),
        ))
        .expect("should save");
    web_sockets(&repos)
        .save(&rich_web_socket("req_at_root", &collection.id, None))
        .expect("should save");

    repos
        .folders
        .delete(&folder.id)
        .expect("should delete folder");
    let left: Vec<String> = web_sockets(&repos)
        .list_by_collection(&collection.id)
        .expect("should list")
        .into_iter()
        .map(|saved| saved.id)
        .collect();
    assert_eq!(left, vec!["req_at_root".to_string()]);

    repos
        .collections
        .delete(&collection.id)
        .expect("should delete collection");
    assert!(web_sockets(&repos)
        .list_by_collection(&collection.id)
        .expect("should list")
        .is_empty());
}

/// A WebSocket has no single response to keep, so no example can hang off one.
#[test]
fn no_example_can_hang_off_a_web_socket() {
    let repos = repos();
    let collection = repos.collections.create("Work").expect("should create");
    web_sockets(&repos)
        .save(&rich_web_socket("req_ws", &collection.id, None))
        .expect("should save");

    let error = repos
        .examples
        .create(&new_example("req_ws", "Frame", 101))
        .expect_err("should refuse");

    assert!(matches!(error, AppError::NotFound(_)));
}

/// OpenAPI cannot describe a WebSocket. Until 13g adds a note saying so, the
/// guarantee is that one in the collection changes nothing at all: not a
/// path, not a note, not a byte.
#[test]
fn a_web_socket_in_the_collection_changes_nothing_in_an_openapi_export() {
    let repos = repos();
    let (service, collection_id) = exportable_collection(&repos);
    let before = service
        .export(
            &collection_id,
            OpenApiVersion::V3_2,
            ExportFormat::Json,
            true,
        )
        .expect("export should succeed");

    web_sockets(&repos)
        .save(&rich_web_socket("req_ws", &collection_id, None))
        .expect("should save");
    let after = service
        .export(
            &collection_id,
            OpenApiVersion::V3_2,
            ExportFormat::Json,
            true,
        )
        .expect("export should still succeed");

    assert_eq!(after.text, before.text);
    assert_eq!(after.notes.len(), before.notes.len());
    assert!(!after.text.contains("wss://"));
}
