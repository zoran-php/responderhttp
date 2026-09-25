// http_client/src-tauri/src/desktop/menu.rs
//
// The window's native menu bar: File > Quit, and Help > Privacy Policy,
// Terms and Conditions, About. Each Help item opens a native message box
// with text from desktop/notices.rs.
//
// Native dialogs rather than a React modal: the text is static, needs no
// IPC, and a native box cannot render a link (the texts carry none either).
//
// Menu events in Tauri 2 are delivered to every registered handler, the
// tray's included, so the ids here are namespaced: the tray already owns
// "quit" (desktop/tray.rs), and a bare id would be handled twice.
use tauri::menu::{Menu, MenuBuilder, MenuEvent, SubmenuBuilder};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::desktop::notices;
use crate::desktop::window::MAIN_WINDOW_LABEL;

const MENU_ID_ABOUT: &str = "app-menu:about";
const MENU_ID_PRIVACY: &str = "app-menu:privacy";
const MENU_ID_TERMS: &str = "app-menu:terms";
const MENU_ID_QUIT: &str = "app-menu:quit";
const EXIT_CODE_SUCCESS: i32 = 0;

/// Handed to `tauri::Builder::menu`, so the menu exists before Tauri creates
/// the window from tauri.conf.json and that window gets it from the start.
pub fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let file = SubmenuBuilder::new(app, "File")
        .text(MENU_ID_QUIT, "Quit")
        .build()?;
    let help = SubmenuBuilder::new(app, "Help")
        .text(MENU_ID_PRIVACY, notices::PRIVACY_TITLE)
        .text(MENU_ID_TERMS, notices::TERMS_TITLE)
        .separator()
        .text(MENU_ID_ABOUT, "About")
        .build()?;

    MenuBuilder::new(app).items(&[&file, &help]).build()
}

pub fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    match event.id().as_ref() {
        MENU_ID_ABOUT => {
            let version = app.package_info().version.to_string();
            show_notice(app, notices::ABOUT_TITLE, notices::about_message(&version));
        }
        MENU_ID_PRIVACY => show_notice(app, notices::PRIVACY_TITLE, notices::PRIVACY_TEXT),
        MENU_ID_TERMS => show_notice(app, notices::TERMS_TITLE, notices::TERMS_TEXT),
        // Unlike closing the window, this really exits: `exit` does not go
        // through CloseRequested, so the close-to-tray handler never sees it.
        MENU_ID_QUIT => app.exit(EXIT_CODE_SUCCESS),
        _ => {}
    }
}

/// Non-blocking `show`, not `blocking_show`: menu events arrive on the main
/// thread, and blocking it would stall the event loop the dialog needs.
/// Parented to the main window so the box is modal to it and centred on it.
fn show_notice<R: Runtime>(app: &AppHandle<R>, title: &str, text: impl Into<String>) {
    let mut dialog = app
        .dialog()
        .message(text)
        .title(title)
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::Ok);
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        dialog = dialog.parent(&window);
    }
    dialog.show(|_| {});
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_IDS: [&str; 4] = [MENU_ID_ABOUT, MENU_ID_PRIVACY, MENU_ID_TERMS, MENU_ID_QUIT];

    /// Every handler sees every menu event, the tray's included; an id
    /// without the prefix could be claimed by two of them.
    #[test]
    fn menu_ids_are_namespaced() {
        for id in ALL_IDS {
            assert!(id.starts_with("app-menu:"), "{id}");
        }
    }

    #[test]
    fn menu_ids_are_distinct() {
        for (index, id) in ALL_IDS.iter().enumerate() {
            assert!(!ALL_IDS[index + 1..].contains(id), "{id} repeated");
        }
    }
}
