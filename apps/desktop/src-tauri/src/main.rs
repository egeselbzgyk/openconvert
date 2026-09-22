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
//! can never add an argument to it (RT B15). This file is only the Tauri wiring: the commands the
//! webview may call, and the two events it receives — `engine-line` (one NDJSON line of one job's
//! engine, parsed and judged by the UI) and `job-changed` (a queue row's state).

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use oc_core::thresholds::T;
use oc_model::document::PresetName;
use openconvert_desktop::config::UiConfig;
use openconvert_desktop::engine::{
    handshake, sidecar_path, Engine, Hello, ProcessLauncher, UiError,
};
use openconvert_desktop::fs_scope::{partition_drop, AppDirs};
use openconvert_desktop::jobqueue::{JobQueue, JobView, QueueSink};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// What the startup check found. The UI shows a blocking error instead of the app when this is an
/// error, and the queue refuses work without it (RT A5.7).
struct Startup(Result<Hello, UiError>);

/// The queue, once the app knows where its directories and its engine are.
struct Queue(Mutex<Option<JobQueue<ProcessLauncher>>>);

/// Relays the queue to the webview.
struct WebviewSink(AppHandle);

#[derive(Clone, Serialize)]
struct LinePayload<'a> {
    job: &'a str,
    line: String,
}

impl QueueSink for WebviewSink {
    fn line(&self, job: &str, line: String) {
        let _ = self.0.emit("engine-line", LinePayload { job, line });
    }
    fn changed(&self, job: &JobView) {
        let _ = self.0.emit("job-changed", job);
    }
}

/// The startup handshake's result, for the UI to render.
#[tauri::command]
fn startup_status(state: tauri::State<'_, Startup>) -> Result<Hello, UiError> {
    state.0.clone()
}

/// What a drop or a pick added, and what it skipped (named, never silently).
#[derive(Serialize)]
struct Enqueued {
    jobs: Vec<String>,
    skipped: Vec<PathBuf>,
}

/// Queue the PDFs among `paths`; name everything else. Paths come from Tauri's native drop or
/// the native file picker, so they are absolute (Phase 12 detail 2).
#[tauri::command]
fn enqueue(
    paths: Vec<PathBuf>,
    preset: Option<PresetName>,
    startup: tauri::State<'_, Startup>,
    queue: tauri::State<'_, Queue>,
) -> Result<Enqueued, UiError> {
    startup.0.as_ref().map_err(Clone::clone)?;
    let drop = partition_drop(paths);
    let jobs = with_queue(&queue, |queue| {
        Ok(queue.enqueue(&drop.pdfs, preset.unwrap_or_default()))
    })?;
    Ok(Enqueued {
        jobs,
        skipped: drop.skipped,
    })
}

#[tauri::command]
fn cancel(job: String, queue: tauri::State<'_, Queue>) -> Result<(), UiError> {
    with_queue(&queue, |queue| queue.cancel(&job, Instant::now()))
}

#[tauri::command]
fn remove(job: String, queue: tauri::State<'_, Queue>) -> Result<(), UiError> {
    with_queue(&queue, |queue| queue.remove(&job))
}

/// The thresholds the webview needs (`openconvert_desktop::config`).
#[tauri::command]
fn ui_config() -> UiConfig {
    UiConfig::from_thresholds(env!("CARGO_PKG_VERSION"))
}

/// "Quit" on the blocking startup screen.
#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

/// Every row, for a webview that (re)loads.
#[tauri::command]
fn queue_rows(queue: tauri::State<'_, Queue>) -> Result<Vec<JobView>, UiError> {
    with_queue(&queue, |queue| Ok(queue.views()))
}

fn with_queue<R>(
    queue: &Queue,
    work: impl FnOnce(&mut JobQueue<ProcessLauncher>) -> Result<R, UiError>,
) -> Result<R, UiError> {
    let mut guard = queue.0.lock().unwrap_or_else(PoisonError::into_inner);
    let queue = guard
        .as_mut()
        .ok_or_else(|| UiError::Io("the queue is not ready".to_owned()))?;
    work(queue)
}

fn main() {
    let engine = sidecar_path();
    let startup = engine
        .clone()
        .and_then(|engine| handshake(&engine, env!("CARGO_PKG_VERSION")));
    let tick =
        Duration::from_millis(u64::try_from(T.desktop.supervisor_tick_ms).unwrap_or(u64::MAX));

    tauri::Builder::default()
        .manage(Startup(startup))
        .manage(Queue(Mutex::new(None)))
        .setup(move |app| {
            let dirs = AppDirs::under(&app.path().app_data_dir()?)?;
            if let Ok(engine) = engine {
                let launcher = ProcessLauncher::new(engine, dirs.jobs.clone());
                let sink = Arc::new(WebviewSink(app.handle().clone()));
                let queue = JobQueue::new(Engine::new(dirs.jobs, launcher), sink);
                *app.state::<Queue>()
                    .0
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(queue);
            }
            // The supervisor's clock: notice exits, enforce the kill deadline, start the next job.
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(tick);
                let state = handle.state::<Queue>();
                let mut guard = state.0.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some(queue) = guard.as_mut() {
                    queue.tick(Instant::now());
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            startup_status,
            ui_config,
            quit,
            enqueue,
            cancel,
            remove,
            queue_rows
        ])
        .run(tauri::generate_context!())
        .expect("the Tauri application starts");
}
