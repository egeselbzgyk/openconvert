#![forbid(unsafe_code)]
//! OpenConvert desktop app: the parts of the shell that are not Tauri (D2, D13.1).
//!
//! The conversion happens in the engine, spawned as a sidecar with exactly one argument (RT B15).
//! Everything here is plain Rust and tested without a window: how the engine is started
//! ([`engine`]), where the app keeps its files and which paths it will hand the engine
//! ([`fs_scope`]). `main.rs` is the Tauri wiring around them.

pub mod config;
pub mod engine;
pub mod fs_scope;
pub mod jobqueue;
