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
// The texts differ by platform only where the facts do: where the data lives,
// what holds the key, and what draws the window (PLAN-LINUX.md 17b). Every
// platform's text is built and tested on every build, so a Windows run still
// checks the Linux wording and the other way round.
//
// Shell only: plain strings and pure functions, testable without a display.

/// Named once so the About text, the dialogs' titles and the copyright
/// cannot drift apart.
const APP_NAME: &str = "ResponderHTTP";
pub const AUTHOR: &str = "Zoran Davidović";
pub const COPYRIGHT: &str = "© 2026 Zoran Davidović. All rights reserved.";

/// Which operating system's facts a text states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Linux,
}

impl Platform {
    /// The build's own platform. macOS is still deferred (PLAN.md Phase 0)
    /// and reads the Linux wording until it has its own.
    pub const CURRENT: Self = if cfg!(windows) {
        Self::Windows
    } else {
        Self::Linux
    };
    pub const ALL: [Self; 2] = [Self::Windows, Self::Linux];
}

pub const ABOUT_TITLE: &str = "About ResponderHTTP";
pub const PRIVACY_TITLE: &str = "Privacy Policy";
pub const TERMS_TITLE: &str = "Terms and Conditions";

/// The About text. The version comes from the running binary rather than a
/// constant, so it can never report a release it is not.
pub fn about_message(version: &str) -> String {
    format!(
        "{APP_NAME}\n\
         Version {version}\n\n\
         A desktop API client for HTTP, WebSocket, gRPC and server-sent events. \
         Requests run through libcurl built into the app, so curl does not \
         need to be installed.\n\n\
         Author: {AUTHOR}\n\n\
         {COPYRIGHT}\n\n\
         Free to use. Built on open-source components, among them libcurl, \
         SQLite, Tauri, React and the Monaco editor, each under its own licence."
    )
}

/// The Privacy Policy dialog's text for `platform`.
pub fn privacy_text(platform: Platform) -> String {
    let (storage, credentials) = match platform {
        Platform::Windows => (PRIVACY_STORAGE_WINDOWS, PRIVACY_CREDENTIALS_WINDOWS),
        Platform::Linux => (PRIVACY_STORAGE_LINUX, PRIVACY_CREDENTIALS_LINUX),
    };
    format!("{PRIVACY_OPENING}\n\n{storage}\n\n{credentials}\n\n{PRIVACY_CLOSING}")
}

const PRIVACY_STORAGE_WINDOWS: &str = "Your data stays on your computer. Saved requests and collections, their documentation, gRPC schemas, environments, history, cookies and settings are kept in a local SQLite database in your Windows user profile. Uninstalling the app removes it.";

const PRIVACY_CREDENTIALS_WINDOWS: &str = "Credentials are encrypted. Passwords, bearer tokens, API keys and secret variables are never stored in plain text. They are encrypted with a key held in Windows Credential Manager, which only your Windows account can read.";

/// Neither the RPM nor a Flatpak removes the home-folder data on uninstall,
/// so this says how to remove it rather than promising it goes.
const PRIVACY_STORAGE_LINUX: &str = "Your data stays on your computer. Saved requests and collections, their documentation, gRPC schemas, environments, history, cookies and settings are kept in a local SQLite database in your home folder. Uninstalling the app leaves it there; delete the app's data folder to remove it.";

/// A Flatpak keeps the key in a keyring file of its own, locked with a
/// secret the portal gets from the desktop's keyring (secrets/keychain.rs).
const PRIVACY_CREDENTIALS_LINUX: &str = "Credentials are encrypted. Passwords, bearer tokens, API keys and secret variables are never stored in plain text. They are encrypted with a key held in your desktop's keyring (GNOME Keyring or KWallet, reached through the secret portal when the app runs as a Flatpak), which only your user account can read.";

const PRIVACY_OPENING: &str = "Last updated: 28 September 2026

The developer collects nothing. ResponderHTTP has no analytics, no telemetry, no crash reporting and no accounts. It does not phone home and contains no update checker.";

const PRIVACY_CLOSING: &str = "Requests go only where you send them. HTTP requests, WebSocket connections and gRPC calls, server reflection included, go directly from your computer to the address you enter, never through a service operated by the developer.

Logs stay local. The diagnostic log leaves out request bodies, authentication headers, tokens, cookies, WebSocket and gRPC messages, gRPC metadata and streamed events, and it is never uploaded.

Files. Importing an OpenAPI document or saving a response reads and writes only the file you chose. Importing .proto files reads those files and the files they import, from their own folders and the import folders you chose.

The app is a developer tool and is not directed at children.

Questions about this policy: zorandavidovic@outlook.com";

/// The Terms and Conditions dialog's text for `platform`.
pub fn terms_text(platform: Platform) -> String {
    let (pass_on, webview) = match platform {
        Platform::Windows => (
            "Pass the installer on unchanged.",
            "The Microsoft Edge WebView2 Runtime is covered by Microsoft's terms.",
        ),
        Platform::Linux => (
            "Pass the package on unchanged.",
            "WebKitGTK, GTK and the other system libraries that draw the window remain under their own licences.",
        ),
    };
    TERMS_TEMPLATE
        .replace(TERMS_PASS_ON, pass_on)
        .replace(TERMS_WEBVIEW, webview)
}

/// Placeholders in TERMS_TEMPLATE, filled per platform by `terms_text`.
const TERMS_PASS_ON: &str = "{pass_on}";
const TERMS_WEBVIEW: &str = "{webview}";

const TERMS_TEMPLATE: &str = "Effective date: 28 September 2026

ResponderHTTP is published by Zoran Davidović. Use it freely, for anything, at no cost, but do not distribute a modified copy.

What you may do. Use it for any purpose, commercial work included, on any number of computers. {pass_on} Read the source code and build it yourself to check it.

What you may not do. Distribute a modified build, a fork, a repackaging or a rebranded copy; present the app as your own work; remove its name, authorship or notices; or sell it or charge for access to it. Changing your own copy for your own use is allowed.

No warranty. The app is provided as is, without warranty of any kind. You are responsible for the requests you send, for having permission to send them, and for the data in them.

Limitation of liability. To the fullest extent permitted by law, the author is not liable for any damages arising from the app or its use, including loss of data or profit. Rights the law does not allow to be waived are not affected.

Third-party components. libcurl, SQLite, Tauri, React, the Monaco editor, protox, prost-reflect and the other components remain under their own licences. {webview}

The full licence is the LICENSE file in the source code. Where it and this summary differ, the licence governs.

Questions about these terms: zorandavidovic@outlook.com";

#[cfg(test)]
mod tests {
    use super::*;

    /// Beyond this a Windows message box outgrows a 768 px screen.
    const MAX_DIALOG_CHARS: usize = 2_000;

    fn every_text() -> Vec<String> {
        let mut texts = vec![about_message("1.1.0")];
        for platform in Platform::ALL {
            texts.push(privacy_text(platform));
            texts.push(terms_text(platform));
        }
        texts
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

    /// The two statements each text exists to make, on every platform.
    #[test]
    fn the_policies_keep_their_core_promises() {
        for platform in Platform::ALL {
            let (privacy, terms) = (privacy_text(platform), terms_text(platform));
            assert!(privacy.contains("The developer collects nothing"));
            assert!(privacy.contains("never stored in plain text"));
            assert!(terms.contains("do not distribute a modified copy"));
            assert!(terms.contains("without warranty"));
        }
    }

    #[test]
    fn every_placeholder_is_filled() {
        for platform in Platform::ALL {
            for text in [privacy_text(platform), terms_text(platform)] {
                assert!(!text.contains('{') && !text.contains('}'), "{text}");
            }
        }
    }

    /// A Linux user must not be told about Windows, nor the other way round.
    #[test]
    fn each_platform_names_only_its_own_system() {
        for text in [privacy_text(Platform::Linux), terms_text(Platform::Linux)] {
            for windows_only in ["Windows", "WebView2", "Credential Manager"] {
                assert!(!text.contains(windows_only), "{windows_only:?} in {text}");
            }
        }
        for text in [
            privacy_text(Platform::Windows),
            terms_text(Platform::Windows),
        ] {
            for linux_only in ["GNOME", "KWallet", "Flatpak", "WebKitGTK"] {
                assert!(!text.contains(linux_only), "{linux_only:?} in {text}");
            }
        }
    }

    #[test]
    fn the_build_reads_its_own_platform() {
        let expected = if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Linux
        };
        assert_eq!(Platform::CURRENT, expected);
    }
}
