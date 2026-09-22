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
//!
//! The webview names files only by job id. A path the Rust side opens, reveals or reads is always
//! one the queue recorded for that job, never a string from the webview.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use oc_core::thresholds::T;
use openconvert_desktop::config::UiConfig;
use openconvert_desktop::engine::{
    handshake, sidecar_path, Engine, Hello, ProcessLauncher, UiError,
};
use openconvert_desktop::fs_scope::{partition_drop, AppDirs};
use openconvert_desktop::jobqueue::{JobQueue, JobView, QueueSink};
use openconvert_desktop::settings::{self, Settings};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

/// What the startup check found. The UI shows a blocking error instead of the app when this is an
/// error, and the queue refuses work without it (RT A5.7).
struct Startup(Result<Hello, UiError>);

/// The queue, once the app knows where its directories and its engine are.
struct Queue(Mutex<Option<JobQueue<ProcessLauncher>>>);

/// The user's settings and where they are kept.
struct Prefs {
    path: Mutex<Option<PathBuf>>,
    current: Mutex<Settings>,
}

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
/// the native file picker, so they are absolute (Phase 12 detail 2). The preset and the resource
/// caps are the user's settings.
#[tauri::command]
fn enqueue(
    paths: Vec<PathBuf>,
    startup: tauri::State<'_, Startup>,
    queue: tauri::State<'_, Queue>,
    prefs: tauri::State<'_, Prefs>,
) -> Result<Enqueued, UiError> {
    startup.0.as_ref().map_err(Clone::clone)?;
    let drop = partition_drop(paths);
    let chosen = prefs
        .current
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let jobs = with_queue(&queue, |queue| {
        Ok(queue.enqueue_with(&drop.pdfs, chosen.preset, chosen.limits()))
    })?;
    Ok(Enqueued {
        jobs,
        skipped: drop.skipped,
    })
}

/// The native file picker, PDFs only ("Select PDF…"). Run off the main thread: a blocking dialog
/// on it would freeze the window it belongs to.
#[tauri::command]
async fn pick_pdfs(app: AppHandle) -> Result<Vec<PathBuf>, UiError> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("PDF", &["pdf", "PDF"])
            .blocking_pick_files()
    })
    .await
    .map_err(|error| UiError::Io(error.to_string()))?;
    Ok(picked
        .unwrap_or_default()
        .into_iter()
        .filter_map(|path| path.into_path().ok())
        .collect())
}

#[tauri::command]
fn cancel(job: String, queue: tauri::State<'_, Queue>) -> Result<(), UiError> {
    with_queue(&queue, |queue| queue.cancel(&job, Instant::now()))
}

#[tauri::command]
fn remove(job: String, queue: tauri::State<'_, Queue>) -> Result<(), UiError> {
    with_queue(&queue, |queue| queue.remove(&job))
}

/// Every row, for a webview that (re)loads.
#[tauri::command]
fn queue_rows(queue: tauri::State<'_, Queue>) -> Result<Vec<JobView>, UiError> {
    with_queue(&queue, |queue| Ok(queue.views()))
}

/// The job's `report.json`, as the engine wrote it (PIPELINE §13). The result panel and the report
/// view are renderings of this file; nothing is summarised on the way.
#[tauri::command]
fn read_report(job: String, queue: tauri::State<'_, Queue>) -> Result<serde_json::Value, UiError> {
    let (_, report) = with_queue(&queue, |queue| queue.outputs(&job))?;
    let text = std::fs::read_to_string(&report)?;
    serde_json::from_str(&text).map_err(|error| UiError::Io(error.to_string()))
}

/// "Open in reader": the OS default EPUB handler. An error means no reader is set up, which the UI
/// explains rather than hides (design proposal, `result.html` §5).
#[tauri::command]
fn open_output(job: String, app: AppHandle, queue: tauri::State<'_, Queue>) -> Result<(), UiError> {
    let (output, _) = with_queue(&queue, |queue| queue.outputs(&job))?;
    app.opener()
        .open_path(output.to_string_lossy(), None::<&str>)
        .map_err(|error| UiError::Io(error.to_string()))
}

/// "Show in folder".
#[tauri::command]
fn show_output(job: String, app: AppHandle, queue: tauri::State<'_, Queue>) -> Result<(), UiError> {
    let (output, _) = with_queue(&queue, |queue| queue.outputs(&job))?;
    app.opener()
        .reveal_item_in_dir(output)
        .map_err(|error| UiError::Io(error.to_string()))
}

/// The thresholds the webview needs (`openconvert_desktop::config`).
#[tauri::command]
fn ui_config() -> UiConfig {
    UiConfig::from_thresholds(env!("CARGO_PKG_VERSION"))
}

#[tauri::command]
fn settings_get(prefs: tauri::State<'_, Prefs>) -> Settings {
    prefs
        .current
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

#[tauri::command]
fn settings_set(next: Settings, prefs: tauri::State<'_, Prefs>) -> Result<(), UiError> {
    if let Some(path) = prefs
        .path
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
    {
        settings::save(path, &next)?;
    }
    *prefs.current.lock().unwrap_or_else(PoisonError::into_inner) = next;
    Ok(())
}

/// "Quit" on the blocking startup screen.
#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Startup(startup))
        .manage(Queue(Mutex::new(None)))
        .manage(Prefs {
            path: Mutex::new(None),
            current: Mutex::new(Settings::default()),
        })
        .setup(move |app| {
            let dirs = AppDirs::under(&app.path().app_data_dir()?)?;
            let settings_file = settings::settings_path(&app.path().app_config_dir()?);
            let prefs = app.state::<Prefs>();
            *prefs.current.lock().unwrap_or_else(PoisonError::into_inner) =
                settings::load(&settings_file);
            *prefs.path.lock().unwrap_or_else(PoisonError::into_inner) = Some(settings_file);

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
            settings_get,
            settings_set,
            quit,
            enqueue,
            pick_pdfs,
            cancel,
            remove,
            queue_rows,
            read_report,
            open_output,
            show_output
        ])
        .run(tauri::generate_context!())
        .expect("the Tauri application starts");
}
