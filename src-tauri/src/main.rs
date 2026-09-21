// http_client/src-tauri/src/main.rs
//
// Binary entry point only — no logic here. The app itself lives in lib.rs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    // Deliberately still eprintln! and not log::error!: if run() returned an
    // error the Tauri builder never finished, so tauri-plugin-log was never
    // installed and a log macro here would go nowhere. There is also no window
    // to show it in, hence stderr and a non-zero exit rather than a panic.
    if let Err(error) = responderhttp_lib::run() {
        eprintln!("failed to start ResponderHTTP: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
