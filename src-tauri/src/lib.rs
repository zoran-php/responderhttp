// http_client/src-tauri/src/lib.rs
//
// App entry point as a library; main.rs is a thin binary wrapper. This is
// the composition root: concrete adapters are chosen here and nowhere else
// (CLAUDE.md section 7, dependency inversion).

// Unsafe code is refused everywhere except http/curl_ws_ffi.rs, which binds
// libcurl's WebSocket API by hand and is allowed in http/mod.rs. A second
// exception has to be argued for there, in review, rather than slipping in.
#![deny(unsafe_code)]

pub mod commands;
pub mod desktop;
pub mod domain;
pub mod http;
pub mod logging;
pub mod openapi;
pub mod persistence;
pub mod secrets;

use std::sync::Arc;

use tauri::Manager;

use crate::desktop::{menu, startup_error, toast, tray, webview, window};
use crate::domain::ports::{CookieRepository, HttpClient};
use crate::domain::services::collections::Collections;
use crate::domain::services::cookies::Cookies;
use crate::domain::services::docs::Docs;
use crate::domain::services::downloads::Downloads;
use crate::domain::services::environments::Environments;
use crate::domain::services::history::History;
use crate::domain::services::openapi::OpenApiExport;
use crate::domain::services::openapi_import::OpenApiImport;
use crate::domain::services::send_request::SendRequest;
use crate::domain::services::tray_notice::TrayNotice;
use crate::domain::services::websocket::WebSocketSessions;
use crate::http::cookie_client::CookieClient;
use crate::http::cookie_websocket::CookieWebSocketConnector;
use crate::http::curl_client::CurlClient;
use crate::http::curl_websocket::CurlWebSocketConnector;
use crate::persistence::database::Database;
use crate::persistence::repositories::app_settings::SqliteAppSettingsRepository;
use crate::persistence::repositories::collections::SqliteCollectionRepository;
use crate::persistence::repositories::cookies::SqliteCookieRepository;
use crate::persistence::repositories::environments::SqliteEnvironmentRepository;
use crate::persistence::repositories::examples::SqliteExampleRepository;
use crate::persistence::repositories::folders::SqliteFolderRepository;
use crate::persistence::repositories::history::SqliteHistoryRepository;
use crate::persistence::repositories::import::SqliteImportRepository;
use crate::persistence::repositories::saved_requests::SqliteSavedRequestRepository;
use crate::persistence::repositories::secret_upgrade::upgrade_plaintext_secrets;
use crate::persistence::repositories::web_sockets::SqliteWebSocketRepository;
use crate::secrets::keychain::KeychainDataKeyStore;
use crate::secrets::{open_cipher, without_key, CipherOrigin};

const DATABASE_FILE: &str = "responderhttp.sqlite3";

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

/// Injected dependencies, reachable from command handlers via `State`.
pub struct AppState {
    pub send_request: SendRequest,
    pub websockets: WebSocketSessions,
    pub collections: Collections,
    pub environments: Environments,
    pub cookies: Cookies,
    pub history: History,
    pub downloads: Downloads,
    pub openapi: OpenApiExport,
    pub docs: Docs,
    /// Read by the window close handler (desktop/window.rs), not by a
    /// command: nothing in the frontend needs it.
    pub tray_notice: TrayNotice,
    pub openapi_import: OpenApiImport,
}

/// Where log lines go.
///
/// The rolling file is the one that matters in a shipped build. Debug builds
/// add stdout so `run-app.bat`'s captured output carries the app's own log
/// rather than only whatever the webview writes to stderr — without it,
/// run-app-log.txt shows Chromium's teardown noise and nothing of ours.
///
/// Release deliberately has no stdout target: a Windows GUI-subsystem binary
/// has no console attached and discards it, which is the same reason
/// main.rs's `eprintln!` is a last resort rather than a diagnostic. Emitting
/// to a stream nobody can read is cost with no reader.
///
/// Both targets go through the same `format` closure, so redaction
/// (logging.rs) applies to stdout exactly as it does to the file — there is
/// no path that reaches a sink without passing it.
fn log_targets() -> Vec<tauri_plugin_log::Target> {
    let mut targets = vec![tauri_plugin_log::Target::new(
        tauri_plugin_log::TargetKind::LogDir {
            file_name: Some("responderhttp".to_string()),
        },
    )];
    // `cfg!` rather than `#[cfg]`: both branches then compile in both
    // profiles, so a mistake here cannot hide in the configuration nobody
    // built locally.
    if cfg!(debug_assertions) {
        targets.push(tauri_plugin_log::Target::new(
            tauri_plugin_log::TargetKind::Stdout,
        ));
    }
    targets
}

/// Why the app did not start.
///
/// `NoWebview` is separate from `Tauri` because the user has already been
/// shown the reason in a dialog by the time it is returned: main.rs only has
/// to end the process with a failing exit code, and its `eprintln!` is then a
/// second copy for `run-app.bat`'s captured output rather than the only one.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    #[error("{0}")]
    NoWebview(String),
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
}

pub fn run() -> Result<(), StartupError> {
    // Before anything else: Tauri builds the window from tauri.conf.json
    // inside its own setup and panics if that fails, so a missing webview has
    // to be caught here or not at all (desktop/webview.rs).
    let webview_version = match webview::check() {
        Ok(version) => version,
        Err(message) => {
            webview::report_unavailable(&message);
            return Err(StartupError::NoWebview(message));
        }
    };

    tauri::Builder::default()
        // Must be registered first: a second launch exits before any other
        // plugin initialises and hands control to this callback in the
        // already-running instance.
        .plugin(tauri_plugin_dialog::init())
        // Rolling JSON file in the OS log directory, plus stdout in debug —
        // see log_targets() above. Ported from the Lockoncam desktop app; see
        // logging.rs for the format and for the redaction this app adds on
        // top of it.
        .plugin(
            tauri_plugin_log::Builder::new()
                .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                .format(|out, message, record| {
                    out.finish(format_args!(
                        "{}",
                        logging::format_log_entry(message, record)
                    ))
                })
                .level(log::LevelFilter::Debug)
                .max_file_size(1_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepAll)
                .targets(log_targets())
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // A second launch is normally the user starting the app again,
            // and the right answer is to show the window they already have.
            //
            // Except when Windows launched it: pressing a button on the
            // close-to-tray toast relaunches the exe with the button's id on
            // the command line, because a packaged app without a COM
            // activator has no other way to be told (desktop/toast.rs). That
            // is not someone asking for the window back — the whole point of
            // the toast is that the app stays in the tray.
            if let Some(action) = toast::action_from_args(&args) {
                if action == toast::Action::NeverAgain {
                    if let Some(state) = app.try_state::<AppState>() {
                        state.tray_notice.never_show_again();
                    }
                }
                return;
            }

            if let Err(err) = window::show_main_window(app) {
                window::report_window_error("show main window", &err);
            }
        }))
        .setup(move |app| {
            // Logged here rather than where it is read, because the log
            // plugin only exists from this point on. A bug report that
            // mentions rendering is worth little without it.
            log::info!("webview: {webview_version}");

            // The database lives in the OS app-data directory, which Tauri
            // resolves per platform; it is created on first run.
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            // A database this build cannot open ends the app, but with a
            // dialog saying why. Before this, a database written by a newer
            // version quit without a word (PLAN-WEBSOCKET.md 13d).
            let database = match Database::open(&data_dir.join(DATABASE_FILE)) {
                Ok(database) => database,
                Err(error) => {
                    startup_error::report_database_failure(&error.to_string(), &data_dir);
                    return Err(error.into());
                }
            };

            // Secrets are sealed with a data key from the OS credential store
            // (PLAN.md Phase 9). A store that cannot be used does not stop the
            // app: secrets then load as unavailable and cannot be saved, and
            // nothing is ever written in plain text instead.
            let (cipher, origin) = match KeychainDataKeyStore::new() {
                Ok(store) => open_cipher(&store),
                Err(error) => without_key(error.to_string()),
            };
            match &origin {
                CipherOrigin::ExistingKey => log::info!("secrets: data key loaded"),
                CipherOrigin::NewKey => log::info!("secrets: no data key found, created one"),
                CipherOrigin::Unavailable(reason) => {
                    log::warn!("secrets: no usable data key this session: {reason}")
                }
            }
            // Not fatal either: a row it cannot convert still loads, and the
            // next start tries again.
            match upgrade_plaintext_secrets(&database, cipher.as_ref()) {
                Ok(report) => log::info!("secrets: startup upgrade {report:?}"),
                Err(error) => log::error!("secrets: startup upgrade failed: {error}"),
            }

            // Session cookies belong to one run of the app (RFC 6265), and a
            // cookie past its expiry should never be offered to a server.
            let cookie_jar = Arc::new(SqliteCookieRepository::new(database.clone()));
            cookie_jar.clear_session()?;
            cookie_jar.purge_expired(unix_now())?;

            // The jar wraps the transport, so nothing upstream knows cookies
            // exist (CLAUDE.md section 7, Liskov).
            let http_client: Arc<dyn HttpClient> = Arc::new(CookieClient::new(
                Arc::new(CurlClient::new()),
                cookie_jar.clone(),
            ));
            // The same jar, wrapped the same way: a handshake is an HTTP
            // request and gets the host's cookies like any other.
            let websockets = WebSocketSessions::new(Arc::new(CookieWebSocketConnector::new(
                Arc::new(CurlWebSocketConnector::new()),
                cookie_jar.clone(),
            )));

            app.manage(AppState {
                send_request: SendRequest::new(http_client),
                websockets,
                collections: Collections::new(
                    Arc::new(SqliteCollectionRepository::new(database.clone())),
                    Arc::new(SqliteFolderRepository::new(database.clone())),
                    Arc::new(SqliteSavedRequestRepository::new(
                        database.clone(),
                        cipher.clone(),
                    )),
                    Arc::new(SqliteExampleRepository::new(database.clone())),
                    Arc::new(SqliteWebSocketRepository::new(database.clone())),
                ),
                environments: Environments::new(Arc::new(SqliteEnvironmentRepository::new(
                    database.clone(),
                    cipher.clone(),
                ))),
                cookies: Cookies::new(cookie_jar),
                history: History::new(Arc::new(SqliteHistoryRepository::new(database.clone()))),
                downloads: Downloads::new(),
                // Every repository above takes a clone rather than the last
                // one consuming `database`: whichever service is added next
                // should not have to move a line to compile.
                openapi: OpenApiExport::new(
                    Arc::new(SqliteCollectionRepository::new(database.clone())),
                    Arc::new(SqliteFolderRepository::new(database.clone())),
                    Arc::new(SqliteSavedRequestRepository::new(
                        database.clone(),
                        cipher.clone(),
                    )),
                    Arc::new(SqliteExampleRepository::new(database.clone())),
                ),
                docs: Docs::new(
                    Arc::new(SqliteCollectionRepository::new(database.clone())),
                    Arc::new(SqliteFolderRepository::new(database.clone())),
                    Arc::new(SqliteSavedRequestRepository::new(
                        database.clone(),
                        cipher.clone(),
                    )),
                ),
                tray_notice: TrayNotice::new(Arc::new(SqliteAppSettingsRepository::new(
                    database.clone(),
                ))),
                openapi_import: OpenApiImport::new(Arc::new(SqliteImportRepository::new(
                    database, cipher,
                ))),
            });

            tray::build_tray(app)?;
            Ok(())
        })
        // Set on the builder rather than in setup: Tauri creates the window
        // from tauri.conf.json before the setup hook runs (see webview.rs),
        // and a menu given here is in place by then (desktop/menu.rs).
        .menu(menu::build_menu)
        .on_menu_event(menu::handle_menu_event)
        .on_window_event(window::hide_main_window_on_close)
        .invoke_handler(tauri::generate_handler![
            commands::request::send_request,
            commands::request::cancel_request,
            commands::request::send_and_download,
            commands::websocket::connect_web_socket,
            commands::websocket::send_web_socket_message,
            commands::websocket::disconnect_web_socket,
            commands::websocket::disconnect_all_web_sockets,
            commands::files::choose_file,
            commands::collections::list_collections,
            commands::collections::collection_contents,
            commands::collections::create_collection,
            commands::collections::rename_collection,
            commands::collections::delete_collection,
            commands::collections::create_folder,
            commands::collections::rename_folder,
            commands::collections::delete_folder,
            commands::collections::save_request,
            commands::collections::load_request,
            commands::collections::rename_request,
            commands::collections::move_request,
            commands::collections::delete_request,
            commands::collections::save_web_socket,
            commands::collections::load_web_socket,
            commands::collections::save_example,
            commands::collections::load_example,
            commands::collections::rename_example,
            commands::collections::delete_example,
            commands::environments::list_environments,
            commands::environments::create_environment,
            commands::environments::rename_environment,
            commands::environments::delete_environment,
            commands::environments::environment_variables,
            commands::environments::set_environment_variables,
            commands::cookies::list_cookies,
            commands::cookies::delete_cookie,
            commands::cookies::clear_cookies,
            commands::history::list_history,
            commands::history::record_history,
            commands::history::delete_history_entry,
            commands::history::clear_history,
            commands::openapi::export_collection_openapi,
            commands::openapi_import::pick_openapi_import,
            commands::openapi_import::preview_openapi_import,
            commands::openapi_import::import_openapi,
            commands::openapi_import::discard_openapi_import,
            commands::docs::item_docs,
            commands::docs::set_item_docs
        ])
        .run(tauri::generate_context!())?;

    Ok(())
}
