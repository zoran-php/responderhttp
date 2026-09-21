// http_client/src-tauri/src/desktop/webview.rs
//
// Refuses to start with an explanation when the system webview is missing.
//
// Why this is a check and not an error handler: Tauri creates the window
// declared in tauri.conf.json inside its own `setup`, *before* the setup hook
// this app registers, and a failure there is a `panic!`, not a returned error
// (tauri 2.11 app.rs). The release binary is built for the Windows GUI
// subsystem, so a panic message reaches no console either — a missing
// WebView2 Runtime would look like the app doing nothing at all. Asking first
// costs one registry lookup per launch and turns that into a message.
//
// Decided 2026-09-19 (PLAN.md Phase 10): the Store build cannot install the
// runtime itself. MSIX has no install-time actions, and the one declarative
// route, `win32dependencies:ExternalDependency`, is ignored for anything
// other than App Installer — so the package relies on the Evergreen Runtime
// already being present. Windows 11 ships it as part of the OS and nearly
// every Windows 10 machine received it through Edge; this check is what the
// remainder sees.
//
// Shell only: no domain logic, no IPC. The message text is a pure function so
// it can be tested without a display.

/// The dialog's title. Short, because Windows shows it as the task dialog's
/// main instruction.
pub const UNAVAILABLE_TITLE: &str = "ResponderHTTP cannot start";

/// Microsoft's own download page for the Evergreen Runtime. Deliberately the
/// human-readable page rather than a direct installer link: the user has to
/// choose between the per-user and per-machine installer, and a link that
/// starts a download unasked is worse.
pub const WEBVIEW2_DOWNLOAD_URL: &str = "https://developer.microsoft.com/microsoft-edge/webview2/";

/// What the user is told when no webview can be reached.
///
/// `details` is the error from the platform, kept at the end so the sentence
/// the user acts on comes first. It names no request, header or token, so
/// §11 rule 6 is not in play.
pub fn unavailable_message(details: &str) -> String {
    let remedy = if cfg!(windows) {
        format!(
            "ResponderHTTP draws its window with the Microsoft Edge WebView2 \
             Runtime, and it is not installed on this PC.\n\n\
             Install it from {WEBVIEW2_DOWNLOAD_URL} and start ResponderHTTP again. \
             It is a Microsoft component, it is free, and Windows 11 normally \
             has it already."
        )
    } else {
        "ResponderHTTP draws its window with the system webview, and it could not \
         be loaded on this computer."
            .to_string()
    };

    format!("{remedy}\n\nThe system reported: {details}")
}

/// The webview's version when one is usable, or the message to show when it
/// is not.
///
/// The version is worth carrying: it goes into the log once logging is up, so
/// a bug report says which WebView2 build rendered the window.
pub fn check() -> Result<String, String> {
    tauri::webview_version().map_err(|error| unavailable_message(&error.to_string()))
}

/// Shows `message` in a native dialog.
///
/// rfd rather than tauri-plugin-dialog because there is no app handle yet:
/// this runs before the Tauri builder. rfd is the same crate that plugin uses
/// underneath, declared here with the identical version and features, so it
/// adds no code to the binary — Cargo.lock does not change.
pub fn report_unavailable(message: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(UNAVAILABLE_TITLE)
        .set_description(message)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_message_leads_with_what_to_do_and_ends_with_the_cause() {
        let message = unavailable_message("Could not find the runtime");

        assert!(message.starts_with("ResponderHTTP draws its window"));
        assert!(message.ends_with("The system reported: Could not find the runtime"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_is_told_where_to_get_the_runtime() {
        let message = unavailable_message("anything");

        assert!(message.contains("WebView2"));
        assert!(message.contains(WEBVIEW2_DOWNLOAD_URL));
    }

    /// The title is the dialog's main instruction, so an empty one would
    /// leave the user with a nameless error box.
    #[test]
    fn the_title_names_the_app() {
        assert!(UNAVAILABLE_TITLE.contains("ResponderHTTP"));
    }
}
