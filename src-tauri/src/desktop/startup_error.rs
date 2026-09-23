// http_client/src-tauri/src/desktop/startup_error.rs
//
// Tells the user why the app did not start when its database cannot be
// opened, instead of exiting without a word. The case this exists for is a
// database written by a newer version of the app (persistence/database.rs,
// `refuse_newer_schema`): the installer blocks downgrades, but a copied or
// restored app-data folder does not go through the installer. Any other
// failure to open the file gets the same dialog, with its own cause.
//
// Shell only, like webview.rs: the text is a pure function so it can be
// tested without a display.
use std::path::Path;

use crate::desktop::webview::UNAVAILABLE_TITLE;

/// What the user is told. The sentence they act on comes first; the cause
/// and the folder follow for a bug report. The cause is a storage error,
/// which names no request, header or token (CLAUDE.md section 11, rule 6).
pub fn database_message(details: &str, data_dir: &Path) -> String {
    format!(
        "ResponderHTTP could not open its data, so it has stopped without \
         changing anything.\n\n\
         The reason: {details}\n\n\
         The data is in {}",
        data_dir.display()
    )
}

/// Shows the message in a native dialog. rfd rather than
/// tauri-plugin-dialog for the reason webview.rs gives: this runs inside
/// `setup`, and a failed setup never gets as far as a usable app handle.
pub fn report_database_failure(details: &str, data_dir: &Path) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(UNAVAILABLE_TITLE)
        .set_description(database_message(details, data_dir))
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_message_says_nothing_was_changed_and_names_the_cause_and_the_folder() {
        let message = database_message(
            "this database was written by a newer version of ResponderHTTP \
             (schema 11, this version knows 10); update the app to open it",
            Path::new("C:/data/responderhttp"),
        );

        assert!(message.starts_with("ResponderHTTP could not open its data"));
        assert!(message.contains("without changing anything"));
        assert!(message.contains("update the app to open it"));
        assert!(message.ends_with("The data is in C:/data/responderhttp"));
    }
}
