//! The app-owned `llama-server` (`llm.rs`, PHASE 9 detail 3): on loopback, with its key in a
//! private file and never on a command line, shared by every job, stopped when idle and when the
//! app ends.
//!
//! The server is `oc-stub-llama-server` from `oc-testkit`, which takes the real server's command
//! line, reads its key from `LLAMA_API_KEY` and answers `/health` and `/v1/chat/completions` the
//! way the real one does, without a model. Gated behind `engine-integration` because it needs a
//! binary built beside this test: `cargo build -p openconvert -p oc-testkit --bins`. Linux only: the
//! tests read `/proc` to see a process gone and its command line. macOS and Windows are unverified
//! here.
#![cfg(all(feature = "engine-integration", target_os = "linux"))]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use oc_ai::transport::{Transport, TransportError};
use oc_core::sidecar::llama::ServerSpec;
use oc_core::thresholds::T;
use oc_net::transport::HttpTransport;
use openconvert_desktop::llm::LlmHost;
use secrecy::SecretString;

/// A9.4's bound, which the app's server is held to as well.
const TEARDOWN: Duration = Duration::from_secs(2);
const LLAMA_SERVER_DEFAULT_PORT: u16 = 8080;

/// `target/<profile>/oc-stub-llama-server`, beside the directory this test binary is in.
fn stub() -> PathBuf {
    let exe = std::env::current_exe().expect("the test knows where it is");
    let profile_dir = exe
        .parent()
        .and_then(|deps| deps.parent())
        .expect("target/<profile>/deps/<test>");
    let stub = profile_dir.join(format!(
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

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oc-desktop-llm-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("made");
    dir
}

fn spec(model: &str) -> ServerSpec {
    ServerSpec {
        model: PathBuf::from(model),
        context: 8192,
        threads: 1,
        cache_reuse: true,
        context_checkpoints: None,
    }
}

/// Whether a process with this pid still exists. A zombie counts as gone: it holds no memory.
fn alive(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => !stat
            .rsplit_once(") ")
            .is_some_and(|(_, rest)| rest.starts_with('Z')),
        Err(_) => false,
    }
}

fn gone_within(pid: u32, bound: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < bound {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    !alive(pid)
}

fn chat(endpoint: &str, key: Option<SecretString>) -> Result<String, TransportError> {
    HttpTransport::new(endpoint, key)
        .expect("a base")
        .post_json("/v1/chat/completions", "{}", Duration::from_secs(2))
}

fn read_key(path: &Path) -> SecretString {
    SecretString::from(std::fs::read_to_string(path).expect("the key file"))
}

/// The server listens on loopback on a port of its own, answers only with its key, and the key
/// reaches an engine through a file only this user can read — never through a command line.
#[test]
fn the_app_server_listens_on_loopback_and_hands_its_key_over_a_private_file() {
    let run = scratch("key");
    let mut host = LlmHost::new(stub(), run.clone());
    let lease = host
        .acquire(&spec("a.gguf"), Instant::now())
        .expect("started");

    let port: u16 = lease
        .endpoint
        .strip_prefix("http://127.0.0.1:")
        .expect("loopback, by address")
        .parse()
        .expect("a port");
    assert_ne!(port, LLAMA_SERVER_DEFAULT_PORT);

    assert!(lease.api_key_file.starts_with(&run));
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&lease.api_key_file)
            .expect("the key file")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "owner read/write only: {mode:o}");
    }
    assert!(matches!(
        chat(&lease.endpoint, None),
        Err(TransportError::Status { status: 401 })
    ));
    chat(&lease.endpoint, Some(read_key(&lease.api_key_file))).expect("answered with the key");

    let pid = host.pid().expect("running");
    let key = std::fs::read_to_string(&lease.api_key_file).expect("the key file");
    let argv = std::fs::read(format!("/proc/{pid}/cmdline")).expect("its command line");
    assert!(
        !String::from_utf8_lossy(&argv).contains(key.trim()),
        "the key is not on the command line"
    );
}

/// One server for every job: a second job reuses the first one's, and the server stops only after
/// `llm.idle_kill_secs` with no job holding it — taking its key file with it.
#[test]
fn the_app_server_is_shared_by_jobs_and_stopped_when_idle() {
    let run = scratch("idle");
    let mut host = LlmHost::new(stub(), run);
    let idle = Duration::from_secs(u64::try_from(T.llm.idle_kill_secs).expect("positive"));

    let start = Instant::now();
    let first = host.acquire(&spec("a.gguf"), start).expect("started");
    let pid = host.pid().expect("running");
    host.release(start);
    let second = host.acquire(&spec("a.gguf"), start).expect("reused");
    assert_eq!(host.pid(), Some(pid), "the same server, not a second one");
    assert_eq!(first, second);

    // Held by a job: never idle, however long the job takes.
    assert!(!host.tick(start + idle + idle));
    let done = start + idle;
    host.release(done);
    assert!(!host.tick(done + idle - Duration::from_secs(1)));
    assert!(alive(pid));

    assert!(host.tick(done + idle), "idle for llm.idle_kill_secs");
    assert!(gone_within(pid, TEARDOWN));
    assert_eq!(host.pid(), None);
    assert!(!second.api_key_file.exists(), "the key went with it");

    // The next job starts a new server with a new key.
    let third = host
        .acquire(&spec("a.gguf"), done + idle)
        .expect("restarted");
    assert_ne!(host.pid(), Some(pid));
    assert!(third.api_key_file.exists());
}

/// Another model means another server, but never under a job that is still using the first.
#[test]
fn another_model_restarts_the_server_only_when_no_job_holds_it() {
    let mut host = LlmHost::new(stub(), scratch("model"));
    let now = Instant::now();
    host.acquire(&spec("a.gguf"), now).expect("started");
    let first = host.pid().expect("running");
    assert!(host.acquire(&spec("b.gguf"), now).is_err(), "busy");
    assert_eq!(host.pid(), Some(first));

    host.release(now);
    host.acquire(&spec("b.gguf"), now).expect("restarted");
    assert!(gone_within(first, TEARDOWN), "the first server was stopped");
    assert_ne!(host.pid(), Some(first));
}

/// The app quitting — `shutdown`, or the host simply dropped — leaves no server behind and no key
/// on disk (A9.4, for the app's own server).
#[test]
fn the_app_server_does_not_outlive_the_app() {
    let run = scratch("exit");
    let mut host = LlmHost::new(stub(), run.clone());
    let lease = host
        .acquire(&spec("a.gguf"), Instant::now())
        .expect("started");
    let pid = host.pid().expect("running");
    host.shutdown();
    assert!(gone_within(pid, TEARDOWN));
    assert!(!lease.api_key_file.exists());

    let mut dropped = LlmHost::new(stub(), run);
    dropped
        .acquire(&spec("a.gguf"), Instant::now())
        .expect("started");
    let pid = dropped.pid().expect("running");
    drop(dropped);
    assert!(gone_within(pid, TEARDOWN));
}

/// A server that cannot start is an error, not a hang, and leaves no key behind.
#[test]
fn a_server_that_cannot_start_is_an_error() {
    let run = scratch("missing");
    let mut host = LlmHost::new(run.join("no-such-llama-server"), run.clone());
    assert!(host.acquire(&spec("a.gguf"), Instant::now()).is_err());
    assert_eq!(host.pid(), None);
    assert!(!run.join("llm.key").exists());
}
