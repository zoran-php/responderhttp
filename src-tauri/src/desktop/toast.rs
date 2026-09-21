// http_client/src-tauri/src/desktop/toast.rs
//
// The Windows toast that says closing the window did not quit the app.
//
// Shell only: the *decision* to show it belongs to TrayNotice
// (domain/services/tray_notice.rs); this module only draws it.
//
// **The Windows half of this file cannot be compiled outside Windows**, so
// everything that can be written platform-independently is: the strings and
// the action ids are plain constants with tests, and `show` has a non-Windows
// stub so `cargo check` and `cargo test` still cover the rest of the crate on
// other platforms.
//
// Why tauri-winrt-notification and not tauri-plugin-notification (decided
// 2026-09-19, PLAN.md Phase 11): the notice needs two buttons and a callback
// telling us which was pressed. The plugin's JS API describes actions but
// documents no Windows support for them; this crate — from the same
// tauri-apps org, and what that plugin wraps underneath — has `add_button`
// and `on_activated` outright.
//
// On the AppUserModelID: a toast is attributed to a registered AUMID. The
// Store build has one for free from its package identity, which is
// PackageFamilyName + "!" + the Application Id in Package.appxmanifest. The
// NSIS build is unpackaged and has none, so it falls back to the id the crate
// documents for that case, at the cost of the toast claiming to come from
// PowerShell. Rather than detecting which build this is, `show` tries the
// package id and falls back when Windows rejects it — one code path, and it
// cannot get the detection wrong.

/// PackageFamilyName + "!" + the Application Id from Package.appxmanifest.
/// Both halves are fixed by Partner Center; see PLAN.md Phase 10.
#[cfg(windows)]
const PACKAGED_APP_ID: &str = "ZoranDavidovi.ResponderHTTP_4ka2c3wj7wsgy!App";

/// What the user did with the toast, however the answer reached us.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// "Got it" — nothing to record, but the window must stay hidden.
    Dismiss,
    /// "Don't show this again".
    NeverAgain,
}

/// Button ids. They travel to Windows and can come back on this process's
/// **command line** (see `action_from_args`), so they are namespaced rather
/// than words like "dismiss" that a real argument could collide with.
pub const ACTION_DISMISS: &str = "tray-notice:dismiss";
pub const ACTION_NEVER_AGAIN: &str = "tray-notice:never-again";

/// Reads a toast button press out of a process's arguments.
///
/// Why arguments at all: a packaged desktop app with no COM activator does
/// not get its in-process `Activated` callback — Microsoft's own guidance is
/// that "Foreground/Background and Legacy notification activations will
/// activate your COM activator instead of your command line", so without one
/// the activation *is* the command line. Windows launches the exe again with
/// the button's id appended, tauri-plugin-single-instance catches that second
/// launch, and this is what tells it apart from a user double-clicking the
/// app.
///
/// Both paths are kept: unpackaged builds have no relaunch and only the
/// in-process callback, packaged builds are the other way round, and
/// recording the preference twice is harmless.
///
/// Matching is by substring because the id arrives however Windows chose to
/// split the command line, and "never again" is checked first so that an
/// argument list somehow carrying both does not silently downgrade to a
/// plain dismiss.
pub fn action_from_args<S: AsRef<str>>(args: &[S]) -> Option<Action> {
    let mentions = |needle: &str| args.iter().any(|arg| arg.as_ref().contains(needle));

    if mentions(ACTION_NEVER_AGAIN) {
        Some(Action::NeverAgain)
    } else if mentions(ACTION_DISMISS) {
        Some(Action::Dismiss)
    } else {
        None
    }
}

pub const TITLE: &str = "ResponderHTTP is still running";
pub const BODY: &str = "The window closed to the system tray. Open it again from the tray icon, or right-click the icon and choose Quit to exit.";
pub const BUTTON_OK: &str = "Got it";
pub const BUTTON_NEVER_AGAIN: &str = "Don't show this again";

/// Shows the notice. `on_never_again` runs if the user presses
/// "Don't show this again".
///
/// Failures are logged and swallowed: this is called from a window event
/// handler that cannot report anything, and a toast that did not appear must
/// never stop the window from hiding.
#[cfg(windows)]
pub fn show<F>(on_never_again: F)
where
    F: Fn() + Send + Clone + 'static,
{
    if let Err(error) = try_show(PACKAGED_APP_ID, on_never_again.clone()) {
        // Expected in the NSIS build and in `tauri dev`, where no package
        // identity exists, so it is not a warning.
        log::debug!("tray notice: no package identity ({error}), falling back");

        if let Err(error) = try_show(
            tauri_winrt_notification::Toast::POWERSHELL_APP_ID,
            on_never_again,
        ) {
            log::warn!("tray notice: could not show the toast: {error}");
        }
    }
}

#[cfg(windows)]
fn try_show<F>(app_id: &str, on_never_again: F) -> tauri_winrt_notification::Result<()>
where
    F: Fn() + Send + 'static,
{
    tauri_winrt_notification::Toast::new(app_id)
        .title(TITLE)
        .text1(BODY)
        .add_button(BUTTON_OK, ACTION_DISMISS)
        .add_button(BUTTON_NEVER_AGAIN, ACTION_NEVER_AGAIN)
        .on_activated(move |action| {
            if action.as_deref() == Some(ACTION_NEVER_AGAIN) {
                on_never_again();
            }
            Ok(())
        })
        .show()
}

/// macOS and Linux have no tray notice yet; the window still hides, silently.
/// Kept so the module compiles and the rest of the crate stays testable off
/// Windows (PLAN.md Phase 0 defers both platforms).
#[cfg(not(windows))]
pub fn show<F>(_on_never_again: F)
where
    F: Fn() + Send + Clone + 'static,
{
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_arguments_are_not_a_toast_press() {
        assert_eq!(action_from_args::<String>(&[]), None);
        assert_eq!(
            action_from_args(&["C:\\ResponderHTTP.exe".to_string()]),
            None
        );
    }

    #[test]
    fn a_button_id_on_the_command_line_is_read_back() {
        assert_eq!(
            action_from_args(&["exe".to_string(), ACTION_DISMISS.to_string()]),
            Some(Action::Dismiss)
        );
        assert_eq!(
            action_from_args(&["exe".to_string(), ACTION_NEVER_AGAIN.to_string()]),
            Some(Action::NeverAgain)
        );
    }

    /// Windows appends the id to a command line it composed itself; it may
    /// arrive glued to whatever it put around it.
    #[test]
    fn the_id_is_found_inside_a_larger_argument() {
        assert_eq!(
            action_from_args(&[format!("-ToastActivated {ACTION_NEVER_AGAIN}")]),
            Some(Action::NeverAgain)
        );
    }

    /// The ids must not be words a user could plausibly pass themselves.
    #[test]
    fn the_ids_are_namespaced() {
        for id in [ACTION_DISMISS, ACTION_NEVER_AGAIN] {
            assert!(id.starts_with("tray-notice:"), "{id}");
        }
    }

    /// Never-again outranks dismiss: losing the preference is the worse error.
    #[test]
    fn never_again_wins_over_dismiss() {
        assert_eq!(
            action_from_args(&[ACTION_DISMISS.to_string(), ACTION_NEVER_AGAIN.to_string()]),
            Some(Action::NeverAgain)
        );
    }

    /// The whole point of the notice is that the app is still running and
    /// that there is a way out. Neither sentence may go missing in an edit.
    #[test]
    fn the_notice_says_it_is_running_and_how_to_quit() {
        assert!(TITLE.contains("still running"));
        assert!(BODY.contains("system tray"));
        assert!(BODY.contains("Quit"));
    }

    /// The action id travels to Windows and back as a string; an empty one
    /// would be indistinguishable from "the toast body was clicked".
    #[test]
    fn the_action_id_is_not_empty() {
        assert!(!ACTION_NEVER_AGAIN.is_empty());
    }

    #[test]
    fn the_buttons_are_labelled_for_what_they_do() {
        assert_eq!(BUTTON_NEVER_AGAIN, "Don't show this again");
        assert!(!BUTTON_OK.is_empty());
    }
}
