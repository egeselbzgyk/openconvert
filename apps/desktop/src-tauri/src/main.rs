#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! OpenConvert desktop app (D2, D13.1).
//!
//! A thin shell. The conversion happens in the engine, spawned as a Tauri `externalBin`
//! sidecar, because that buys deterministic RAM reclamation and cancellation-by-kill for
//! free and keeps the C/C++ boundaries (PDFium, llama.cpp) out of this process (D13.1).
//!
//! The app passes the engine exactly one argument - a job-spec path in a directory the app
//! controls - and builds that command in Rust (`openconvert_desktop::engine`), so the webview
//! can never add an argument to it (RT B15).

use openconvert_desktop::engine::{handshake, sidecar_path, Hello, UiError};

/// What the startup check found. The UI shows a blocking error instead of the app when this is an
/// error, and nothing in the app runs a conversion without it (RT A5.7).
struct Startup(Result<Hello, UiError>);

/// The startup handshake's result, for the UI to render.
#[tauri::command]
fn startup_status(state: tauri::State<'_, Startup>) -> Result<Hello, UiError> {
    state.0.clone()
}

fn main() {
    let startup = sidecar_path().and_then(|engine| handshake(&engine, env!("CARGO_PKG_VERSION")));
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(Startup(startup))
        .invoke_handler(tauri::generate_handler![startup_status])
        .run(tauri::generate_context!())
        .expect("the Tauri application starts");
}
