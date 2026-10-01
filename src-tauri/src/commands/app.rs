// http_client/src-tauri/src/commands/app.rs
//
// The app's own lifetime, as opposed to a feature's (PLAN-LINUX.md 17b-2).
use tauri::{AppHandle, Runtime};

use crate::desktop::window::EXIT_CODE_SUCCESS;

/// Ends the app. Called by the frontend once the close button's question,
/// if there was one, has been answered (desktop/window.rs,
/// services/app-lifecycle.ts). The same exit as Quit in the tray and the
/// File menu, so no close event runs and nothing is asked twice.
#[tauri::command]
pub fn quit_app<R: Runtime>(app: AppHandle<R>) {
    log::info!("quitting from the window's close button");
    app.exit(EXIT_CODE_SUCCESS);
}
