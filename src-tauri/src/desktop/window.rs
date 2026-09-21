// http_client/src-tauri/src/desktop/window.rs
use tauri::{AppHandle, Manager, Runtime, Window, WindowEvent};

use crate::desktop::toast;
use crate::AppState;

/// Must match `app.windows[].label` in tauri.conf.json.
pub const MAIN_WINDOW_LABEL: &str = "main";

/// Brings the main window back from hidden (tray) or minimized, and focuses it.
/// Shared by the tray "Show" item and the single-instance callback.
pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return Ok(());
    };
    window.show()?;
    window.unminimize()?;
    window.set_focus()
}

/// Closing the main window hides it to the tray instead of exiting.
/// The tray "Quit" item is the only way out, via `AppHandle::exit`, which
/// does not go through `CloseRequested`.
///
/// The first closes also raise a toast saying so, because a window that
/// vanishes on X looks like a quit (desktop/toast.rs). Whether to show it is
/// TrayNotice's call, not this function's.
pub fn hide_main_window_on_close<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    let WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    if window.label() != MAIN_WINDOW_LABEL {
        return;
    }
    api.prevent_close();
    if let Err(err) = window.hide() {
        report_window_error("hide main window", &err);
    }

    // After hiding, never before: the toast describes a window that has
    // already gone, and showing it first would race the user's eyes.
    notify_hidden_to_tray(window.app_handle());
}

/// Raises the close-to-tray notice if the user has not turned it off.
///
/// The handle is cloned into the toast callback because that callback runs
/// later, on a Windows event thread, long after this function returns.
fn notify_hidden_to_tray<R: Runtime>(app: &AppHandle<R>) {
    // try_state, not state: Tauri builds the window declared in
    // tauri.conf.json inside its own setup, *before* the setup hook that
    // calls `manage` (tauri 2.11 app.rs — the same ordering that makes
    // desktop/webview.rs a pre-flight check). A close in that window would
    // panic on `state`, and a panic in an event handler takes the app with
    // it. There is nothing useful to say if it happens, so it stays quiet.
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if !state.tray_notice.should_show() {
        return;
    }

    let app = app.clone();
    toast::show(move || {
        if let Some(state) = app.try_state::<AppState>() {
            state.tray_notice.never_show_again();
        }
    });
}

/// Callbacks from the event loop can't propagate errors, so they are reported
/// here instead. Window errors carry no request data, so the message is safe
/// as written — `logging::redact` is a backstop, not the reason this is safe.
pub fn report_window_error(action: &str, err: &tauri::Error) {
    log::warn!("failed to {action}: {err}");
}
