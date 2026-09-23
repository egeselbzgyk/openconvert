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

use oc_core::sidecar::readiness::ModelReadiness;
use oc_core::thresholds::T;
use openconvert_desktop::ai::JobAi;
use openconvert_desktop::config::UiConfig;
use openconvert_desktop::corrections::{self, Patch};
use openconvert_desktop::diagnostics::{self, Bundle};
use openconvert_desktop::engine::{
    handshake, sidecar_path, Engine, Hello, ProcessLauncher, UiError,
};
use openconvert_desktop::fs_scope::{partition_drop, AppDirs, CacheUsage};
use openconvert_desktop::jobqueue::{JobQueue, JobView, QueueSink, Rebuild};
use openconvert_desktop::llm::{self, AppModelServer, LlmHost};
use openconvert_desktop::models::{self, LicenseView, ModelManager, ModelsView, Row, RowSink};
use openconvert_desktop::netlog::{self, NetworkLog};
use openconvert_desktop::packs::{self, PackManager, PackReadiness, PacksView};
use openconvert_desktop::preview::{self, PreviewIndex};
use openconvert_desktop::providers::ProviderCli;
use openconvert_desktop::settings::{self, Settings};
#[cfg(feature = "updater")]
use openconvert_desktop::updater;
use openconvert_desktop::{smoke, tree};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

/// What the startup check found. The UI shows a blocking error instead of the app when this is an
/// error, and the queue refuses work without it (RT A5.7).
struct Startup(Result<Hello, UiError>);

/// The queue, once the app knows where its directories and its engine are.
struct Queue(Mutex<Option<JobQueue<ProcessLauncher>>>);

/// The app's own model server (`llm.rs`): started on the first job that wants built-in AI
/// assistance, stopped when idle and when the app exits. Shared with the queue's
/// [`AppModelServer`], which leases it to jobs.
struct Llm(Arc<Mutex<Option<LlmHost>>>);

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

/// The model manager (Phase 12 detail 9), once the app knows its config directory.
struct Models(ModelManager);

/// Relays the model manager's rows to the webview.
struct WebviewModels(AppHandle);

impl RowSink<ModelReadiness> for WebviewModels {
    fn changed(&self, row: &Row<ModelReadiness>) {
        let _ = self.0.emit("model-changed", row);
    }
}

/// The pack manager: the model manager's mechanism over the pack registry.
struct Packs(PackManager);

/// Relays the pack manager's rows to the webview.
struct WebviewPacks(AppHandle);

impl RowSink<PackReadiness> for WebviewPacks {
    fn changed(&self, row: &Row<PackReadiness>) {
        let _ = self.0.emit("pack-changed", row);
    }
}

/// Settings › Packs: the pack registry's rows, or why there are none.
#[tauri::command]
fn packs_list(packs: tauri::State<'_, Packs>) -> PacksView {
    packs.0.view()
}

#[tauri::command]
fn pack_license(id: String, packs: tauri::State<'_, Packs>) -> Result<LicenseView, UiError> {
    packs.0.license(&id)
}

#[tauri::command]
fn pack_accept_license(id: String, packs: tauri::State<'_, Packs>) -> Result<(), UiError> {
    packs.0.accept_license(&id)
}

#[tauri::command]
fn pack_pull(id: String, packs: tauri::State<'_, Packs>) -> Result<(), UiError> {
    packs.0.pull(&id)
}

#[tauri::command]
fn pack_cancel(id: String, packs: tauri::State<'_, Packs>) -> Result<(), UiError> {
    packs.0.cancel(&id)
}

#[tauri::command]
fn pack_remove(id: String, packs: tauri::State<'_, Packs>) -> Result<(), UiError> {
    packs.0.remove(&id)
}

/// The models screen: every registry entry's `ModelReadiness`, or why there are none.
#[tauri::command]
fn models_list(models: tauri::State<'_, Models>) -> ModelsView {
    models.0.view()
}

/// A model's licence, in full, to show before its download (UI_UX §2.4).
#[tauri::command]
fn model_license(id: String, models: tauri::State<'_, Models>) -> Result<LicenseView, UiError> {
    models.0.license(&id)
}

/// "Accept licence and download", first half: the acceptance, kept in the app's local state.
#[tauri::command]
fn model_accept_license(id: String, models: tauri::State<'_, Models>) -> Result<(), UiError> {
    models.0.accept_license(&id)
}

/// Start a download; progress arrives as `model-changed` events. Refused until the licence has
/// been accepted.
#[tauri::command]
fn model_pull(id: String, models: tauri::State<'_, Models>) -> Result<(), UiError> {
    models.0.pull(&id)
}

/// Cancel a download: the transfer stops and its `.part` is deleted (row 12.12).
#[tauri::command]
fn model_cancel(id: String, models: tauri::State<'_, Models>) -> Result<(), UiError> {
    models.0.cancel(&id)
}

/// Delete an installed model and its licence files.
#[tauri::command]
fn model_remove(id: String, models: tauri::State<'_, Models>) -> Result<(), UiError> {
    models.0.remove(&id)
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
    // AI assistance as the settings say at the moment of the drop: a job keeps what it was queued
    // with, whatever changes before its turn.
    let ai = JobAi::plan(&chosen);
    let jobs = with_queue(&queue, |queue| {
        Ok(queue.enqueue_as(&drop.pdfs, chosen.preset, chosen.limits(), &ai))
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

/// Save the settings the webview sends — all but the key file and the consent, which only the Rust
/// side sets (`Settings::merged_from_webview`). Returns what was saved.
#[tauri::command]
fn settings_set(next: Settings, prefs: tauri::State<'_, Prefs>) -> Result<Settings, UiError> {
    let current = prefs
        .current
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let next = current.merged_from_webview(next);
    store_settings(&prefs, next)
}

/// Keep `next` as the settings, on disk and in memory.
fn store_settings(prefs: &Prefs, next: Settings) -> Result<Settings, UiError> {
    if let Some(path) = prefs
        .path
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
    {
        settings::save(path, &next)?;
    }
    *prefs.current.lock().unwrap_or_else(PoisonError::into_inner) = next.clone();
    Ok(next)
}

/// Settings › Provider asks the engine (`providers.rs`); `None` until the engine is known.
struct Providers(Mutex<Option<ProviderCli>>);

fn provider_cli(providers: &Providers) -> Result<ProviderCli, UiError> {
    providers
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
        .ok_or_else(|| UiError::Provider("the converter is not available".to_owned()))
}

/// Run a provider question off the main thread: it waits on the engine, which may wait on a network.
async fn off_main<R: Send + 'static>(
    work: impl FnOnce() -> Result<R, UiError> + Send + 'static,
) -> Result<R, UiError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| UiError::Io(error.to_string()))?
}

/// Is Ollama running on this computer, and which models does it serve (`provider detect`).
#[tauri::command]
async fn provider_detect(app: AppHandle) -> Result<serde_json::Value, UiError> {
    let cli = provider_cli(&app.state::<Providers>())?;
    off_main(move || cli.detect()).await
}

/// Does `url` need consent, and would the engine use it (`provider check`). Sends nothing.
#[tauri::command]
async fn provider_check(url: String, app: AppHandle) -> Result<serde_json::Value, UiError> {
    let cli = provider_cli(&app.state::<Providers>())?;
    off_main(move || cli.check(&url)).await
}

/// "Test connection": what a conversion would open with the saved settings (`provider probe`).
#[tauri::command]
async fn provider_probe(app: AppHandle) -> Result<serde_json::Value, UiError> {
    let cli = provider_cli(&app.state::<Providers>())?;
    let settings = app
        .state::<Prefs>()
        .current
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    off_main(move || cli.probe(&settings)).await
}

/// "Change…" on the API key file: the native picker, and the path kept in the settings. The key
/// itself is never read here and never shown; the engine reads the file when it needs it.
#[tauri::command]
async fn pick_key_file(app: AppHandle) -> Result<Settings, UiError> {
    let picker = app.clone();
    let picked =
        tauri::async_runtime::spawn_blocking(move || picker.dialog().file().blocking_pick_file())
            .await
            .map_err(|error| UiError::Io(error.to_string()))?;
    let prefs = app.state::<Prefs>();
    let mut next = prefs
        .current
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    if let Some(path) = picked.and_then(|path| path.into_path().ok()) {
        next.custom.api_key_file = Some(path);
    }
    store_settings(&prefs, next)
}

/// "Remove" on the API key file.
#[tauri::command]
fn clear_key_file(prefs: tauri::State<'_, Prefs>) -> Result<Settings, UiError> {
    let mut next = prefs
        .current
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    next.custom.api_key_file = None;
    store_settings(&prefs, next)
}

/// The consent dialog's "Allow {host}": consent, now, to the saved custom endpoint's own host — the
/// one the dialog named — kept with that configuration (D10).
#[tauri::command]
fn grant_consent(prefs: tauri::State<'_, Prefs>) -> Result<Settings, UiError> {
    let current = prefs
        .current
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let next = current.granting_consent().ok_or_else(|| {
        UiError::Provider(
            "the endpoint is this computer, or not a URL: no consent is needed".to_owned(),
        )
    })?;
    store_settings(&prefs, next)
}

/// Settings › Network log. The hook PHASE 14 detail 12's audit log fills (`netlog.rs`).
#[tauri::command]
fn network_log() -> NetworkLog {
    netlog::read()
}

/// Check for an update, when the user asks (PHASE 15 detail 5). Downloaded and verified against the
/// key in `tauri.conf.json` before it is offered; held until [`update_install`].
#[cfg(feature = "updater")]
#[tauri::command]
async fn update_check(app: AppHandle) -> Result<updater::Checked, UiError> {
    let setup = updater::Setup::from_plugin_config(app.config().plugins.0.get(updater::CONFIG_KEY));
    let Some(setup) = setup else {
        return Ok(updater::Checked::Failed {
            code: "no_key".to_owned(),
        });
    };
    let handle = app.clone();
    off_main(move || {
        let connect =
            Duration::from_secs(u64::try_from(T.net.connect_timeout_secs).unwrap_or_default());
        let fetch = oc_net::download::HttpFetch::new(connect);
        let (checked, verified) = updater::check(&setup, env!("CARGO_PKG_VERSION"), &fetch);
        *handle
            .state::<updater::Pending>()
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = verified;
        Ok(checked)
    })
    .await
}

/// Install the update [`update_check`] verified, and restart into it.
#[cfg(feature = "updater")]
#[tauri::command]
async fn update_install(app: AppHandle) -> Result<(), UiError> {
    let pending = app
        .state::<updater::Pending>()
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
        .ok_or_else(|| UiError::Io("no verified update is waiting".to_owned()))?;
    let scratch = app.state::<AppDirs>().run.clone();
    off_main(move || updater::install(&pending, &scratch).map_err(UiError::Io)).await?;
    app.restart()
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

/// Queue `pdf` as a drop would, wait for its row to finish, report it and exit with the engine's
/// code ([`smoke`]). The supervisor's clock, started beside this, runs the job.
fn smoke_convert(handle: AppHandle, pdf: PathBuf, tick: Duration) {
    std::thread::spawn(move || {
        if let Err(error) = &handle.state::<Startup>().0 {
            eprintln!("smoke: the app did not start its engine: {error}");
            handle.exit(smoke::EXIT_NOT_STARTED);
            return;
        }
        let preset = handle
            .state::<Prefs>()
            .current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .preset;
        let queue = handle.state::<Queue>();
        let Ok(ids) = with_queue(&queue, |queue| Ok(queue.enqueue(&[pdf], preset))) else {
            eprintln!("smoke: the queue is not ready");
            handle.exit(smoke::EXIT_NOT_STARTED);
            return;
        };
        loop {
            std::thread::sleep(tick);
            let finished = with_queue(&queue, |queue| {
                Ok(queue
                    .views()
                    .into_iter()
                    .find(|view| ids.contains(&view.id))
                    .and_then(|view| smoke::finished(&view).map(|code| (view, code))))
            });
            if let Ok(Some((view, code))) = finished {
                println!(
                    "smoke: {} -> {} (exit {code})",
                    view.input.display(),
                    view.output.display()
                );
                handle.exit(code);
                return;
            }
        }
    });
}

fn main() {
    // The app's own connections — its model manager's downloads — go in the same audit log the
    // engine writes, and Settings › Network log reads it (PHASE 14 detail 12).
    oc_net::audit::install(netlog::log());
    // Every engine's process tree ends with the app, however it ends: this teardown runs from the
    // panic hook and the signal handler `supervise` installs (D13.2), and at `RunEvent::Exit`.
    oc_core::sidecar::supervise::on_teardown(tree::end_all);
    // `--smoke-convert <pdf>` (rows 15.7, 15.19): everything below runs as it always does, and the
    // book is dropped from the command line instead of on the window.
    let smoke_pdf = match smoke::requested(std::env::args_os()) {
        Ok(pdf) => pdf,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(smoke::EXIT_NOT_STARTED);
        }
    };
    let engine = sidecar_path();
    let startup = engine
        .clone()
        .and_then(|engine| handshake(&engine, env!("CARGO_PKG_VERSION")));
    let tick =
        Duration::from_millis(u64::try_from(T.desktop.supervisor_tick_ms).unwrap_or(u64::MAX));

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .register_uri_scheme_protocol("ocpreview", |context, request| {
            serve_preview(context.app_handle(), request.uri().path())
        })
        .manage(Startup(startup))
        .manage(Queue(Mutex::new(None)))
        .manage(LastBundle(Mutex::new(None)))
        .manage(Llm(Arc::new(Mutex::new(None))))
        .manage(Providers(Mutex::new(None)))
        .manage(Prefs {
            path: Mutex::new(None),
            current: Mutex::new(Settings::default()),
        })
        .setup(move |app| {
            #[cfg(feature = "updater")]
            app.manage(updater::Pending::default());
            let dirs = AppDirs::under(&app.path().app_data_dir()?)?;
            let config_dir = app.path().app_config_dir()?;
            let settings_file = settings::settings_path(&config_dir);
            // One store for the app and `openconvert model`, so a model is fetched once.
            let model_manager = ModelManager::new(
                models::bundled_registry(),
                oc_net::store::ModelStore::new(oc_net::store::default_root()),
                models::http_fetch(),
                Some(models::accepted_path(&config_dir)),
                Arc::new(WebviewModels(app.handle().clone())),
            );
            app.manage(Models(model_manager.clone()));
            app.manage(Packs(PackManager::new(
                packs::bundled_registry(),
                oc_net::store::ModelStore::new(oc_net::store::default_packs_root()),
                models::http_fetch(),
                Some(packs::accepted_path(&config_dir)),
                Arc::new(WebviewPacks(app.handle().clone())),
            )));
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
            // Only a build that ships `llama-server` beside the app has a server to start; without
            // one, built-in AI assistance converts without AI and says so.
            let host = Arc::clone(&app.state::<Llm>().0);
            if let Ok(program) = llm::server_path() {
                if program.is_file() {
                    *host.lock().unwrap_or_else(PoisonError::into_inner) =
                        Some(LlmHost::new(program, dirs.run.clone()));
                }
            }
            let model_server = Arc::new(AppModelServer::new(host, model_manager));

            if let Ok(engine) = &engine {
                *app.state::<Providers>()
                    .0
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) =
                    Some(ProviderCli::new(engine.clone()));
            }
            if let Ok(engine) = engine {
                let launcher =
                    ProcessLauncher::new(engine, dirs.jobs.clone()).with_cache(dirs.cache.clone());
                let sink = Arc::new(WebviewSink(app.handle().clone()));
                let queue = JobQueue::new(Engine::new(dirs.jobs, launcher), sink)
                    .with_model_server(model_server);
                *app.state::<Queue>()
                    .0
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = Some(queue);
            }
            if let Some(pdf) = smoke_pdf.clone() {
                smoke_convert(app.handle().clone(), pdf, tick);
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
                // Only tried: a job's lease holds the lock while a model loads, and the clock must
                // not stop for it.
                let llm = handle.state::<Llm>();
                let guard = match llm.0.try_lock() {
                    Ok(guard) => Some(guard),
                    Err(std::sync::TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
                    Err(std::sync::TryLockError::WouldBlock) => None,
                };
                if let Some(mut guard) = guard {
                    if let Some(host) = guard.as_mut() {
                        host.tick(now);
                    }
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
            show_bundle,
            models_list,
            model_license,
            model_accept_license,
            model_pull,
            model_cancel,
            model_remove,
            packs_list,
            pack_license,
            pack_accept_license,
            pack_pull,
            pack_cancel,
            pack_remove,
            provider_detect,
            provider_check,
            provider_probe,
            pick_key_file,
            clear_key_file,
            grant_consent,
            network_log,
            #[cfg(feature = "updater")]
            update_check,
            #[cfg(feature = "updater")]
            update_install
        ])
        .build(tauri::generate_context!())
        .expect("the Tauri application starts");
    // `run_return`, so the code `AppHandle::exit` was given is the process's: a smoke conversion
    // that failed must not look like one that passed.
    let code = app.run_return(|app, event| {
        if let tauri::RunEvent::Exit = event {
            // Nothing the app started outlives it: every engine's tree, then the model server
            // and its key.
            tree::end_all();
            let llm = Arc::clone(&app.state::<Llm>().0);
            let stopped = match llm.try_lock() {
                Ok(mut guard) => {
                    if let Some(host) = guard.as_mut() {
                        host.shutdown();
                    }
                    true
                }
                Err(_) => false,
            };
            if !stopped {
                // A model is still loading for a job: end the server the way the panic hook
                // would. Its key file goes with the run directory at the next start.
                oc_core::sidecar::supervise::kill_all();
            }
        }
    });
    std::process::exit(code);
}
