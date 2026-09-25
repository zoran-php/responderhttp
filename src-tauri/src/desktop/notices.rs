// http_client/src-tauri/src/desktop/notices.rs
//
// The text of the About, Privacy Policy and Terms and Conditions dialogs
// opened from the menu bar (desktop/menu.rs).
//
// Condensed from store/privacy-policy.md and docs/terms.html, which remain
// the full versions. When either of those changes, this file changes with
// it — the dates at the top of each text are the reminder.
//
// Two rules the tests pin:
// - **No links.** No URL of any kind appears, so no platform's dialog can
//   turn one into something clickable. The contact address is plain text.
// - **Short enough for a message box.** A native message box does not
//   scroll: on Windows it grows until it runs off a 768 px screen.
//
// Shell only: plain strings and one pure function, testable without a
// display.

/// Named once so the About text, the dialogs' titles and the copyright
/// cannot drift apart.
const APP_NAME: &str = "ResponderHTTP";
pub const AUTHOR: &str = "Zoran Davidović";
pub const COPYRIGHT: &str = "© 2026 Zoran Davidović. All rights reserved.";

pub const ABOUT_TITLE: &str = "About ResponderHTTP";
pub const PRIVACY_TITLE: &str = "Privacy Policy";
pub const TERMS_TITLE: &str = "Terms and Conditions";

/// The About text. The version comes from the running binary rather than a
/// constant, so it can never report a release it is not.
pub fn about_message(version: &str) -> String {
    format!(
        "{APP_NAME}\n\
         Version {version}\n\n\
         A desktop API client for HTTP, WebSocket and server-sent events. \
         Requests run through libcurl built into the app, so nothing else \
         needs to be installed.\n\n\
         Author: {AUTHOR}\n\n\
         {COPYRIGHT}\n\n\
         Free to use. Built on open-source components, among them libcurl, \
         SQLite, Tauri, React and the Monaco editor, each under its own licence."
    )
}

pub const PRIVACY_TEXT: &str = "Last updated: 23 September 2026

The developer collects nothing. ResponderHTTP has no analytics, no telemetry, no crash reporting and no accounts. It does not phone home and contains no update checker.

Your data stays on your computer. Saved requests and collections, their documentation, environments, history, cookies and settings are kept in a local SQLite database in your Windows user profile. Uninstalling the app removes it.

Credentials are encrypted. Passwords, bearer tokens, API keys and secret variables are never stored in plain text. They are encrypted with a key held in Windows Credential Manager, which only your Windows account can read.

Requests go only where you send them. HTTP requests and WebSocket connections go directly from your computer to the address you enter, never through a service operated by the developer.

Logs stay local. The diagnostic log leaves out request bodies, authentication headers, tokens, cookies, WebSocket messages and streamed events, and it is never uploaded.

Files. Importing an OpenAPI document or saving a response reads and writes only the file you chose.

The app is a developer tool and is not directed at children.

Questions about this policy: zorandavidovic@outlook.com";

pub const TERMS_TEXT: &str = "Effective date: 23 September 2026

ResponderHTTP is published by Zoran Davidović. Use it freely, for anything, at no cost, but do not distribute a modified copy.

What you may do. Use it for any purpose, commercial work included, on any number of computers. Pass the installer on unchanged. Read the source code and build it yourself to check it.

What you may not do. Distribute a modified build, a fork, a repackaging or a rebranded copy; present the app as your own work; remove its name, authorship or notices; or sell it or charge for access to it. Changing your own copy for your own use is allowed.

No warranty. The app is provided as is, without warranty of any kind. You are responsible for the requests you send, for having permission to send them, and for the data in them.

Limitation of liability. To the fullest extent permitted by law, the author is not liable for any damages arising from the app or its use, including loss of data or profit. Rights the law does not allow to be waived are not affected.

Third-party components. libcurl, SQLite, Tauri, React, the Monaco editor and the other components remain under their own licences. The Microsoft Edge WebView2 Runtime is covered by Microsoft's terms.

The full licence is the LICENSE file in the source code. Where it and this summary differ, the licence governs.

Questions about these terms: zorandavidovic@outlook.com";

#[cfg(test)]
mod tests {
    use super::*;

    /// Beyond this a Windows message box outgrows a 768 px screen.
    const MAX_DIALOG_CHARS: usize = 2_000;

    fn every_text() -> [String; 3] {
        [
            about_message("1.0.0"),
            PRIVACY_TEXT.to_string(),
            TERMS_TEXT.to_string(),
        ]
    }

    #[test]
    fn no_dialog_carries_a_link() {
        for text in every_text() {
            let lower = text.to_lowercase();
            for marker in ["://", "www.", "<a", "href", ".com/", ".io/", ".rs/"] {
                assert!(!lower.contains(marker), "{marker:?} in {text}");
            }
        }
    }

    #[test]
    fn every_dialog_fits_a_message_box() {
        for text in every_text() {
            let length = text.chars().count();
            assert!(length <= MAX_DIALOG_CHARS, "{length} chars: {text}");
        }
    }

    #[test]
    fn about_names_the_app_the_version_the_author_and_the_copyright() {
        let about = about_message("9.8.7");

        assert!(about.starts_with(APP_NAME));
        assert!(about.contains("Version 9.8.7"));
        assert!(about.contains(&format!("Author: {AUTHOR}")));
        assert!(about.contains(COPYRIGHT));
    }

    /// The two statements each text exists to make.
    #[test]
    fn the_policies_keep_their_core_promises() {
        assert!(PRIVACY_TEXT.contains("The developer collects nothing"));
        assert!(TERMS_TEXT.contains("do not distribute a modified copy"));
        assert!(TERMS_TEXT.contains("without warranty"));
    }
}
