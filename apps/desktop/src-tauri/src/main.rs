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
use openconvert_desktop::corrections::{self, Patch};
use openconvert_desktop::diagnostics::{self, Bundle};
use openconvert_desktop::engine::{
    handshake, sidecar_path, Engine, Hello, ProcessLauncher, UiError,
};
use openconvert_desktop::fs_scope::{partition_drop, AppDirs, CacheUsage};
use openconvert_desktop::jobqueue::{JobQueue, JobView, QueueSink, Rebuild};
use openconvert_desktop::llm::{self, LlmHost};
use openconvert_desktop::preview::{self, PreviewIndex};
use openconvert_desktop::settings::{self, Settings};
use openconvert_desktop::tree;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

/// What the startup check found. The UI shows a blocking error instead of the app when this is an
/// error, and the queue refuses work without it (RT A5.7).
struct Startup(Result<Hello, UiError>);

/// The queue, once the app knows where its directories and its engine are.
struct Queue(Mutex<Option<JobQueue<ProcessLauncher>>>);

/// The app's own model server (`llm.rs`): started on the first job that wants AI assistance,
/// stopped when idle and when the app exits.
struct Llm(Mutex<Option<LlmHost>>);

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

/// "Unlock" on a password-protected row: convert that PDF again with the password typed there,
/// for that one job (design decision 13).
#[tauri::command]
fn unlock(
    job: String,
    password: String,
    queue: tauri::State<'_, Queue>,
) -> Result<String, UiError> {
    with_queue(&queue, |queue| queue.unlock(&job, password))
}

/// "Fix and rebuild": keep the corrections an editor made to job `job`'s book, merged into the ones
/// already kept for it, and convert it again with them in place of its output (Phase 12 detail 8).
/// The digest and IR version come from the report the editor showed, so the corrections name the
/// bytes and the block ids that report did.
#[tauri::command]
fn save_overrides(
    job: String,
    patch: Patch,
    dirs: tauri::State<'_, AppDirs>,
    queue: tauri::State<'_, Queue>,
) -> Result<String, UiError> {
    let (_, report) = with_queue(&queue, |queue| queue.outputs(&job))?;
    let report: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&report)?)
        .map_err(|error| UiError::Io(error.to_string()))?;
    let (sha256, ir_version) = corrections::keyed_by(&report)?;
    let path = dirs
        .overrides_for(&sha256)
        .ok_or_else(|| UiError::Io(format!("{sha256} is not a digest")))?;
    corrections::save(&path, &sha256, ir_version, patch)?;
    with_queue(&queue, |queue| {
        queue.rebuild(
            &job,
            Rebuild {
                overrides: path,
                sha256,
            },
        )
    })
}

/// Settings › Advanced: what the cache holds (SECURITY §10: disclosed, not hidden).
#[tauri::command]
fn cache_usage(dirs: tauri::State<'_, AppDirs>) -> CacheUsage {
    dirs.cache_usage()
}

/// Settings › Advanced › "Clear cache…": delete the saved text of every converted book. EPUBs and
/// corrections are not touched; a later "Fix and rebuild" reads the PDF again.
#[tauri::command]
fn clear_cache(dirs: tauri::State<'_, AppDirs>) -> Result<(), UiError> {
    dirs.clear_cache().map_err(UiError::from)
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

/// The book's chapters and page list, for the preview's navigation.
#[tauri::command]
fn preview_index(job: String, queue: tauri::State<'_, Queue>) -> Result<PreviewIndex, UiError> {
    let (output, _) = with_queue(&queue, |queue| queue.outputs(&job))?;
    preview::index(&output)
}

/// Where the preview protocol is reachable on this platform: WebView2 maps a custom scheme to
/// `http://<scheme>.localhost`, the other webviews use the scheme itself.
#[tauri::command]
fn preview_base() -> &'static str {
    if cfg!(windows) {
        "http://ocpreview.localhost/"
    } else {
        "ocpreview://localhost/"
    }
}

/// `%XX` decoding for the preview protocol's path segments; anything malformed is left as is and
/// then fails to name an entry.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        let escaped = (byte == b'%')
            .then(|| text.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(decoded) => {
                out.push(decoded);
                index += 3;
            }
            None => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The preview protocol: `/<job>/<path in that job's EPUB>`, served with the preview's own CSP.
fn serve_preview(app: &AppHandle, path: &str) -> tauri::http::Response<Vec<u8>> {
    let decoded = percent_decode(path.trim_start_matches('/'));
    let served = decoded
        .split_once('/')
        .ok_or_else(|| UiError::Io(decoded.clone()))
        .and_then(|(job, entry)| {
            let state = app.state::<Queue>();
            let (output, _) = with_queue(&state, |queue| queue.outputs(job))?;
            preview::serve(&output, entry)
        });
    let response = tauri::http::Response::builder()
        .header("Content-Security-Policy", preview::SERVED_CSP)
        .header("X-Content-Type-Options", "nosniff");
    match served {
        Ok((bytes, media)) => response
            .header(tauri::http::header::CONTENT_TYPE, media)
            .body(bytes),
        Err(_) => response
            .status(tauri::http::StatusCode::NOT_FOUND)
            .body(Vec::new()),
    }
    .unwrap_or_default()
}

/// The last bundle written, so "Show in folder" reveals it without the webview naming a path.
struct LastBundle(Mutex<Option<PathBuf>>);

/// "Export diagnostic bundle" / "Report a problem…": the native save dialog, then the bundle —
/// the job's report and event log when there is a job, the versions and the system always. The
/// review screen lists what was written. Nothing is sent anywhere (SECURITY §10).
#[tauri::command]
async fn export_diagnostics(
    job: Option<String>,
    app: AppHandle,
) -> Result<Option<Bundle>, UiError> {
    let picker = app.clone();
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        picker
            .dialog()
            .file()
            .set_file_name(diagnostics::default_name())
            .add_filter("ZIP", &["zip"])
            .blocking_save_file()
    })
    .await
    .map_err(|error| UiError::Io(error.to_string()))?;
    let Some(path) = chosen.and_then(|path| path.into_path().ok()) else {
        return Ok(None);
    };

    let (report, events) = match &job {
        Some(id) => {
            let state = app.state::<Queue>();
            let (_, report) = with_queue(&state, |queue| queue.outputs(id))?;
            let events = with_queue(&state, |queue| Ok(queue.events(id)))?;
            (std::fs::read(report).ok(), Some(events))
        }
        None => (None, None),
    };
    let hello = app.state::<Startup>().0.as_ref().ok().cloned();
    let files = diagnostics::contents(report, events, env!("CARGO_PKG_VERSION"), hello.as_ref());
    let bundle = diagnostics::write(&path, &files)?;
    *app.state::<LastBundle>()
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(path);
    Ok(Some(bundle))
}

/// "Show in folder" on the bundle review screen.
#[tauri::command]
fn show_bundle(app: AppHandle, last: tauri::State<'_, LastBundle>) -> Result<(), UiError> {
    let path = last
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .ok_or_else(|| UiError::Io("no bundle has been written".to_owned()))?;
    app.opener()
        .reveal_item_in_dir(path)
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
    // Every engine's process tree ends with the app, however it ends: this teardown runs from the
    // panic hook and the signal handler `supervise` installs (D13.2), and at `RunEvent::Exit`.
    oc_core::sidecar::supervise::on_teardown(tree::end_all);
    let engine = sidecar_path();
    let startup = engine
        .clone()
        .and_then(|engine| handshake(&engine, env!("CARGO_PKG_VERSION")));
    let tick =
        Duration::from_millis(u64::try_from(T.desktop.supervisor_tick_ms).unwrap_or(u64::MAX));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .register_uri_scheme_protocol("ocpreview", |context, request| {
            serve_preview(context.app_handle(), request.uri().path())
        })
        .manage(Startup(startup))
        .manage(Queue(Mutex::new(None)))
        .manage(LastBundle(Mutex::new(None)))
        .manage(Llm(Mutex::new(None)))
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

            // A rebuild is offered only on this session's rows, so last session's saves are the
            // text of books kept on disk for nothing (SECURITY §10). A failure here costs a later
            // rebuild nothing: every save is keyed and checked before it is resumed from.
            let _ = dirs.clear_cache();
            let _ = dirs.clear_run();
            app.manage(dirs.clone());
            if let Ok(program) = llm::server_path() {
                *app.state::<Llm>()
                    .0
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) =
                    Some(LlmHost::new(program, dirs.run.clone()));
            }

            if let Ok(engine) = engine {
                let launcher =
                    ProcessLauncher::new(engine, dirs.jobs.clone()).with_cache(dirs.cache.clone());
                let sink = Arc::new(WebviewSink(app.handle().clone()));
                let queue = JobQueue::new(Engine::new(dirs.jobs, launcher), sink);
                *app.state::<Queue>()
                    .0
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(queue);
            }
            // The supervisor's clock: notice exits, enforce the kill deadline, start the next job,
            // and stop the model server once no job has used it for `llm.idle_kill_secs`.
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(tick);
                let now = Instant::now();
                let state = handle.state::<Queue>();
                let mut guard = state.0.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some(queue) = guard.as_mut() {
                    queue.tick(now);
                }
                drop(guard);
                let llm = handle.state::<Llm>();
                let mut guard = llm.0.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some(host) = guard.as_mut() {
                    host.tick(now);
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
            unlock,
            save_overrides,
            cache_usage,
            clear_cache,
            cancel,
            remove,
            queue_rows,
            read_report,
            open_output,
            show_output,
            preview_index,
            preview_base,
            export_diagnostics,
            show_bundle
        ])
        .build(tauri::generate_context!())
        .expect("the Tauri application starts")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // Nothing the app started outlives it: every engine's tree, then the model server
                // and its key.
                tree::end_all();
                let llm = app.state::<Llm>();
                let mut guard = llm.0.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some(host) = guard.as_mut() {
                    host.shutdown();
                }
            }
        });
}
