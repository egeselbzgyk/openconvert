//! The supervisor against the real engine (Phase 12 rows 12.6 and 12.17).
//!
//! Gated behind `engine-integration` (IMPLEMENTATION_PLAN §0.2): it needs the engine built beside
//! this test (`cargo build -p openconvert`) and the vendored PDFium. The `desktop` CI job builds
//! both and turns the feature on.
#![cfg(feature = "engine-integration")]

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use oc_core::jobspec::JobSpec;
use oc_core::thresholds::T;
use openconvert_desktop::engine::{handshake, Engine, ProcessLauncher, UiError};
use openconvert_desktop::fs_scope::AppDirs;

/// `target/<profile>/openconvert`, beside the directory this test binary is in.
fn engine_binary() -> PathBuf {
    let exe = std::env::current_exe().expect("the test knows where it is");
    let profile_dir = exe
        .parent()
        .and_then(|deps| deps.parent())
        .expect("target/<profile>/deps/<test>");
    let engine = profile_dir.join(format!("openconvert{}", std::env::consts::EXE_SUFFIX));
    assert!(
        engine.is_file(),
        "no engine at {}; run `cargo build -p openconvert` first",
        engine.display()
    );
    engine
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oc-desktop-it-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("made");
    dir
}

fn seconds(value: i64) -> Duration {
    Duration::from_secs(u64::try_from(value).expect("a positive threshold"))
}

/// 12.6 / A12.2 — Cancel reaches `done{cancelled}` within `ipc.cancel_deadline_secs`, the engine
/// exits 3, and the destination holds no output, no report and no `.oc-tmp-*`.
#[test]
fn cancel_reaches_done_cancelled_and_cleans_temp() {
    let root = scratch("cancel");
    let dirs = AppDirs::under(&root.join("app")).expect("made");
    let books = root.join("books");
    std::fs::create_dir_all(&books).expect("made");

    let pages = usize::try_from(T.perf.bench_reference_pages).expect("positive");
    let input = books.join("long.pdf");
    std::fs::write(&input, oc_testkit::handmade::reference_book(pages)).expect("written");
    let spec = JobSpec::new(input.clone(), books.join("long.epub"));

    let engine = Engine::new(
        dirs.jobs.clone(),
        ProcessLauncher::new(engine_binary(), dirs.jobs.clone()),
    );
    let (lines, received) = mpsc::channel::<String>();
    let mut running = engine
        .start(
            "job-cancel",
            &spec,
            None,
            Box::new(move |line| {
                let _ = lines.send(line);
            }),
        )
        .expect("the engine starts");

    let event = |line: String| -> serde_json::Value {
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("{line:?}: {e}"))
    };

    // Cancel once pages are demonstrably being read.
    loop {
        let next = event(
            received
                .recv_timeout(Duration::from_secs(60))
                .expect("progress arrives"),
        );
        assert_ne!(
            next["t"], "done",
            "it finished before it could be cancelled"
        );
        if next["t"] == "progress" {
            break;
        }
    }
    running.cancel().expect("the cancel is written");
    let asked = Instant::now();

    let deadline = seconds(T.ipc.cancel_deadline_secs);
    let done = loop {
        let next = event(
            received
                .recv_timeout(seconds(T.ipc.kill_after_secs))
                .expect("the run ends"),
        );
        if next["t"] == "done" || next["t"] == "fatal" {
            break next;
        }
    };
    let took = asked.elapsed();
    assert_eq!(done["status"], "cancelled", "{done}");
    assert!(took <= deadline, "done{{cancelled}} after {took:?}");

    let exit = loop {
        if let Some(code) = running.try_wait().expect("waits") {
            break code;
        }
        assert!(
            asked.elapsed() <= seconds(T.ipc.kill_after_secs),
            "still running at the kill deadline"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(exit, Some(3), "cancelled is exit 3");

    let mut left: Vec<String> = std::fs::read_dir(&books)
        .expect("reads")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, ["long.pdf"], "the destination is clean");
}

/// 12.17 — an engine whose `hello.engine_version` is not the app's blocks startup with a message
/// that names both versions and the fix. The real engine, asked by an app of another version.
#[test]
fn stale_sidecar_is_refused_at_startup() {
    let engine = engine_binary();

    let refused = handshake(&engine, "0.0.0-stale").expect_err("a stale pairing is refused");
    assert_eq!(
        refused,
        UiError::StaleEngine {
            engine: env!("CARGO_PKG_VERSION").to_owned(),
            app: "0.0.0-stale".to_owned(),
        }
    );
    let message = refused.to_string();
    assert!(message.contains(env!("CARGO_PKG_VERSION")) && message.contains("0.0.0-stale"));
    assert!(
        message.contains("stage-sidecars"),
        "the message names the fix"
    );

    // The matched pair starts.
    let hello = handshake(&engine, env!("CARGO_PKG_VERSION")).expect("the matched pair starts");
    assert!(!hello.pdfium_version.is_empty());
}

/// A12.1 — forty dropped PDFs are forty jobs, one converting at a time, every one of them
/// completing: the queue driving the real engine, forty times over.
#[test]
fn forty_dropped_books_all_complete_one_at_a_time() {
    use std::sync::Arc;

    use oc_model::document::PresetName;
    use openconvert_desktop::jobqueue::{JobQueue, JobState, JobView, QueueSink};

    struct Quiet;
    impl QueueSink for Quiet {
        fn line(&self, _job: &str, _line: String) {}
        fn changed(&self, _job: &JobView) {}
    }

    let root = scratch("forty");
    let dirs = AppDirs::under(&root.join("app")).expect("made");
    let books = root.join("books");
    std::fs::create_dir_all(&books).expect("made");
    let book = oc_testkit::handmade::reference_book(1);
    let drop: Vec<PathBuf> = (1..=40)
        .map(|n| {
            let path = books.join(format!("book-{n:02}.pdf"));
            std::fs::write(&path, &book).expect("written");
            path
        })
        .collect();

    let engine = Engine::new(
        dirs.jobs.clone(),
        ProcessLauncher::new(engine_binary(), dirs.jobs.clone()).with_cache(dirs.cache.clone()),
    );
    let mut queue = JobQueue::new(engine, Arc::new(Quiet));
    assert_eq!(queue.enqueue(&drop, PresetName::Auto).len(), 40);

    let started = Instant::now();
    loop {
        queue.tick(Instant::now());
        let views = queue.views();
        let running = views
            .iter()
            .filter(|view| matches!(view.state, JobState::Running | JobState::Cancelling))
            .count();
        assert!(running <= 1, "{running} converting at once");
        if views
            .iter()
            .all(|view| matches!(view.state, JobState::Exited { .. }))
        {
            for view in &views {
                assert_eq!(
                    view.state,
                    JobState::Exited { code: Some(0) },
                    "{}",
                    view.id
                );
                assert!(view.output.is_file(), "{} wrote no EPUB", view.id);
            }
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(600),
            "forty one-page books took more than ten minutes"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The models screen renders exactly what `openconvert model list --json` prints (PHASE 9 detail
/// 7): the same `ModelReadiness` rows, read the same way from the same registry and store.
#[test]
fn the_app_and_the_cli_list_the_same_models() {
    let root = scratch("models");
    let store = root.join("store");
    let registry_path = root.join("models.toml");
    let commit = "0123456789abcdef0123456789abcdef01234567";
    let hash = "ab".repeat(32);
    std::fs::write(
        &registry_path,
        format!(
            r#"schema_version = 1
default = "tiny"

[[model]]
id = "tiny"
tier = "default"
display_name = "Tiny"
family = "qwen3"
arch = "dense"
license = "Apache-2.0"
repo = "org/tiny-GGUF"
revision = "{commit}"
file = "tiny.gguf"
sha256 = "{hash}"
size_bytes = 1000
context = 8192
parallel = 1
min_ram_bytes = 3221225472
cpu_expectation = "moderate"
prompt_profile = "qwen3-chatml"
cache_reuse = true

[[model]]
id = "later"
tier = "experimental"
display_name = "Later"
family = "qwen3"
arch = "hybrid"
license = "Apache-2.0"
repo = "org/later-GGUF"
revision = "{commit}"
file = "later.gguf"
sha256 = "{hash}"
size_bytes = 2000
context = 8192
parallel = 1
min_ram_bytes = 1610612736
prompt_profile = "qwen3-chatml"
cache_reuse = false
warn = "Experimental."
"#
        ),
    )
    .expect("written");
    // One model installed, as a download leaves it; the other absent.
    std::fs::create_dir_all(store.join("tiny")).expect("made");
    std::fs::write(store.join("tiny").join("tiny.gguf"), [0u8; 1000]).expect("written");
    std::fs::write(store.join("tiny").join("LICENSE"), "Apache").expect("written");

    let output = std::process::Command::new(engine_binary())
        .args(["model", "list", "--json", "--registry"])
        .arg(&registry_path)
        .arg("--dir")
        .arg(&store)
        .output()
        .expect("the engine ran");
    assert!(output.status.success(), "{output:?}");
    let cli: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");

    let registry = oc_net::registry::ModelRegistry::load(&registry_path).expect("a registry");
    let app = serde_json::to_value(openconvert_desktop::models::readiness(
        &registry,
        &oc_net::store::ModelStore::new(&store),
    ))
    .expect("JSON");
    assert_eq!(app, cli);
    assert_eq!(
        app[0]["installed"], true,
        "the case where the two could differ"
    );
}

/// `target/<profile>/oc-stub-llama-server`, beside the engine: `cargo build -p oc-testkit --bins`.
fn stub_server() -> PathBuf {
    let stub = engine_binary().with_file_name(format!(
        "oc-stub-llama-server{}",
        std::env::consts::EXE_SUFFIX
    ));
    assert!(
        stub.is_file(),
        "no stub server at {}; run `cargo build -p oc-testkit --bins` first",
        stub.display()
    );
    stub
}

/// PHASE 12 part B2 (the AI toggle) on PHASE 9 detail 3's app-owned server: AI assistance on, the
/// built-in provider, the default model installed. The queue starts the app's own server for that
/// model when the job's turn comes, the job's one argument names it — endpoint, the run's key file,
/// the model — and the real engine asks it what it is: the report says the local server answered,
/// for that model. The lease ends with the job, and the idle clock then stops the server.
#[test]
fn a_conversion_with_built_in_ai_is_served_by_the_apps_own_server() {
    use std::sync::{Arc, Mutex};

    use oc_core::sidecar::readiness::ModelReadiness;
    use oc_model::document::PresetName;
    use openconvert_desktop::ai::JobAi;
    use openconvert_desktop::jobqueue::{JobQueue, JobState, JobView, QueueSink};
    use openconvert_desktop::llm::{AppModelServer, LlmHost};
    use openconvert_desktop::models::{self, ModelManager, Row, RowSink};

    struct Quiet;
    impl QueueSink for Quiet {
        fn line(&self, _job: &str, _line: String) {}
        fn changed(&self, _job: &JobView) {}
    }
    impl RowSink<ModelReadiness> for Quiet {
        fn changed(&self, _row: &Row<ModelReadiness>) {}
    }

    let root = scratch("builtin-ai");
    let dirs = AppDirs::under(&root.join("app")).expect("made");
    let store = root.join("store");
    let registry_path = root.join("models.toml");
    let commit = "0123456789abcdef0123456789abcdef01234567";
    let hash = "ab".repeat(32);
    std::fs::write(
        &registry_path,
        format!(
            r#"schema_version = 1
default = "tiny"

[[model]]
id = "tiny"
tier = "default"
display_name = "Tiny"
family = "qwen3"
arch = "dense"
license = "Apache-2.0"
repo = "org/tiny-GGUF"
revision = "{commit}"
file = "tiny.gguf"
sha256 = "{hash}"
size_bytes = 1000
context = 8192
parallel = 1
min_ram_bytes = 3221225472
prompt_profile = "qwen3-chatml"
cache_reuse = true
"#
        ),
    )
    .expect("written");
    std::fs::create_dir_all(store.join("tiny")).expect("made");
    std::fs::write(store.join("tiny").join("tiny.gguf"), [0u8; 1000]).expect("written");

    let manager = ModelManager::new(
        oc_net::registry::ModelRegistry::load(&registry_path).map_err(|error| error.to_string()),
        oc_net::store::ModelStore::new(&store),
        models::http_fetch(),
        None,
        Arc::new(Quiet),
    );
    let host = Arc::new(Mutex::new(Some(LlmHost::new(
        stub_server(),
        dirs.run.clone(),
    ))));
    let server = Arc::new(AppModelServer::new(Arc::clone(&host), manager));

    let input = root.join("book.pdf");
    std::fs::write(&input, oc_testkit::handmade::reference_book(2)).expect("written");
    let engine = Engine::new(
        dirs.jobs.clone(),
        ProcessLauncher::new(engine_binary(), dirs.jobs.clone()),
    );
    let mut queue = JobQueue::new(engine, Arc::new(Quiet)).with_model_server(server);
    queue.enqueue_as(
        std::slice::from_ref(&input),
        PresetName::Auto,
        None,
        &JobAi::Builtin,
    );

    let started = Instant::now();
    let view = loop {
        queue.tick(Instant::now());
        let view = queue.views().remove(0);
        if matches!(
            view.state,
            JobState::Exited { .. } | JobState::FailedToStart { .. }
        ) {
            break view;
        }
        assert!(started.elapsed() < Duration::from_secs(120), "{view:?}");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(view.state, JobState::Exited { code: Some(0) }, "{view:?}");
    assert_eq!(
        view.ai.as_ref().and_then(|ai| ai.unavailable),
        None,
        "the app's server was up"
    );

    let spec: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(dirs.jobs.join(format!("{}.json", view.id))).expect("the spec"),
    )
    .expect("JSON");
    assert_eq!(spec["ai"]["enabled"], true);
    assert_eq!(spec["ai"]["model_id"], "tiny");
    assert!(
        spec["ai"]["endpoint"]
            .as_str()
            .is_some_and(|url| url.starts_with("http://127.0.0.1:")),
        "{spec}"
    );
    assert!(
        spec["ai"]["api_key_file"]
            .as_str()
            .is_some_and(|file| file.starts_with(&*dirs.run.to_string_lossy())),
        "the run's key is named by its file in the app's run directory, never written in the spec"
    );

    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("book.epub.report.json")).expect("the report"),
    )
    .expect("JSON");
    assert_eq!(
        report["ai"]["provider"], "local_sidecar",
        "{:#}",
        report["ai"]
    );
    assert_eq!(report["ai"]["model_id"], "tiny");

    // The job has ended, so the lease has: the idle clock stops the server.
    let idle = seconds(T.llm.idle_kill_secs);
    let mut guard = host.lock().expect("not poisoned");
    let running = guard.as_mut().expect("a host");
    assert!(
        running.pid().is_some(),
        "the server outlives the job until it is idle"
    );
    assert!(running.tick(Instant::now() + idle), "idle: stopped");
    assert!(running.pid().is_none());
}

/// Settings › Provider reads the engine's own answers (PHASE 11's hand-off): whether Ollama runs on
/// this computer, whether an endpoint needs consent — asked without sending anything — and what
/// "Test connection" finds. A host nobody consented to is refused before anything is sent, and the
/// refusal names the host, so the consent dialog can.
#[test]
fn the_providers_screen_reads_the_engines_answers() {
    use openconvert_desktop::providers::ProviderCli;
    use openconvert_desktop::settings::{Provider, Settings};

    let cli = ProviderCli::new(engine_binary());
    let detected = cli.detect().expect("detect answers");
    assert!(
        detected.get("ollama").is_some(),
        "null, or Ollama's models: {detected}"
    );

    let remote = cli.check("https://llm.example.org/v1").expect("an answer");
    assert_eq!(remote["host"], "llm.example.org");
    assert_eq!(remote["requires_consent"], true);
    assert_eq!(remote["usable"], true);
    let plain = cli.check("http://llm.example.org/v1").expect("an answer");
    assert_eq!(
        plain["usable"], false,
        "plain http off this computer: {plain}"
    );
    let local = cli.check("http://127.0.0.1:1234/v1").expect("an answer");
    assert_eq!(local["requires_consent"], false);

    let mut custom = Settings {
        provider: Provider::Custom,
        ..Settings::default()
    };
    custom.custom.endpoint = "https://llm.example.org/v1".to_owned();
    assert_eq!(
        cli.probe(&custom),
        Err(UiError::ConsentRequired {
            host: "llm.example.org".to_owned()
        }),
        "nothing is asked of a host nobody consented to"
    );

    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("a loopback port");
    custom.custom.endpoint = format!("http://{closed}/v1");
    let probed = cli.probe(&custom).expect("an answer");
    assert_eq!(probed["available"], false, "{probed}");
}
