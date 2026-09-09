//! Tauri's build script: it reads `tauri.conf.json` and generates the context the app's
//! `generate_context!` macro expands.

fn main() {
    tauri_build::build();
}
