#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! OpenConvert desktop app (D2, D13.1).
//!
//! A thin shell. The conversion happens in the engine, spawned as a Tauri `externalBin`
//! sidecar, because that buys deterministic RAM reclamation and cancellation-by-kill for
//! free and keeps the C/C++ boundaries (PDFium, llama.cpp) out of this process (D13.1).
//!
//! The app passes the engine exactly one argument - a job-spec path in a directory the app
//! controls - so that the capability allow-list in `capabilities/engine.json` is meaningful
//! rather than decorative (RT B15).

fn main() {
    // The Rust side deliberately holds no logic yet: the version handshake that guards
    // against a stale staged sidecar lives in the UI, where it can be tested without a
    // window (see apps/desktop/ui/src/engine.ts and its test).
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .run(tauri::generate_context!())
        .expect("the Tauri application starts");
}
