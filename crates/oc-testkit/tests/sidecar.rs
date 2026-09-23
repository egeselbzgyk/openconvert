//! Rows 9.8, 9.9, 9.10 and 9.13, and acceptance criterion A9.4: an engine-owned `llama-server`
//! listens only on loopback, only answers with its key, and does not outlive the engine however the
//! engine ends.
//!
//! The server is `oc-stub-llama-server`, which takes the real server's command line and answers its
//! two endpoints; the engine is `oc-sidecar-engine`, which drives `oc_core::sidecar` exactly as the
//! real engine will. Both are real processes, because "the child is gone after the parent died" is
//! not a thing one process can observe about itself.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use oc_ai::transport::{Transport, TransportError};
use oc_core::sidecar::llama::ServerSpec;
use oc_core::sidecar::server::{Health, OwnedServer, SidecarError};
use oc_core::thresholds::T;
use oc_net::transport::HttpTransport;
use secrecy::ExposeSecret;

/// A9.4: "no orphaned `llama-server` within 2 s".
const TEARDOWN: Duration = Duration::from_secs(2);
const LLAMA_SERVER_DEFAULT_PORT: u16 = 8080;

fn stub() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_oc-stub-llama-server"))
}

fn engine() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_oc-sidecar-engine"))
}

fn spec() -> ServerSpec {
    ServerSpec {
        model: PathBuf::from("no-model.gguf"),
        context: 8192,
        threads: 1,
        cache_reuse: true,
        context_checkpoints: None,
    }
}

fn probe(server: &OwnedServer) -> Health {
    match HttpTransport::new(&server.base_url(), None)
        .expect("a base")
        .get("/health", Duration::from_secs(1))
    {
        Ok(_) => Health::Ready,
        Err(_) => Health::NotYet,
    }
}

fn start() -> OwnedServer {
    let port = oc_net::loopback::free_port().expect("a free port");
    let server = OwnedServer::spawn(&stub(), &spec(), port).expect("spawn");
    server
        .wait_healthy(&probe, Duration::from_secs(10))
        .expect("healthy");
    server
}

/// Whether a process with this pid still exists. A zombie counts as gone: it holds no memory.
fn alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => !stat
                .rsplit_once(") ")
                .is_some_and(|(_, rest)| rest.starts_with('Z')),
            Err(_) => false,
        }
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }
    #[cfg(windows)]
    {
        Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains(&pid.to_string()))
    }
}

fn gone_within(pid: u32, deadline: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < deadline {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    !alive(pid)
}

/// Run the engine in `mode`, and return it with the pid of the server it started.
fn run_engine(mode: &str) -> (std::process::Child, u32) {
    let mut child = Command::new(engine())
        .args([mode, &stub().to_string_lossy()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the engine starts");
    let mut line = String::new();
    BufReader::new(child.stdout.take().expect("stdout"))
        .read_line(&mut line)
        .expect("the engine reports its server");
    let pid = line
        .trim()
        .strip_prefix("pid ")
        .and_then(|pid| pid.parse().ok())
        .unwrap_or_else(|| panic!("the engine said {line:?}"));
    assert!(alive(pid), "the server is running before the engine ends");
    let mut stdin = child.stdin.take().expect("stdin");
    stdin.write_all(b"go\n").expect("go");
    (child, pid)
}

/// Row 9.8.
#[test]
fn owned_server_binds_loopback_ephemeral_with_key() {
    let server = start();
    assert_ne!(server.port(), LLAMA_SERVER_DEFAULT_PORT);
    assert!(server.base_url().starts_with("http://127.0.0.1:"));
    let key = server.api_key().expose_secret().to_owned();
    assert!(
        key.len() >= 32,
        "a CSPRNG key, not a constant: {} chars",
        key.len()
    );
    assert_ne!(
        key,
        start().api_key().expose_secret(),
        "every run gets its own key"
    );

    // Without the key: refused. With it: answered.
    let anonymous = HttpTransport::new(&server.base_url(), None).expect("a base");
    assert_eq!(
        anonymous.post_json("/v1/chat/completions", "{}", Duration::from_secs(5)),
        Err(TransportError::Status { status: 401 })
    );
    let keyed =
        HttpTransport::new(&server.base_url(), Some(server.api_key().clone())).expect("a base");
    assert!(keyed
        .post_json("/v1/chat/completions", "{}", Duration::from_secs(5))
        .is_ok());

    // What the kernel says is listening on that port: 127.0.0.1, not 0.0.0.0.
    #[cfg(target_os = "linux")]
    {
        let tcp = std::fs::read_to_string("/proc/net/tcp").expect("/proc/net/tcp");
        let port = format!("{:04X}", server.port());
        let listening: Vec<&str> = tcp
            .lines()
            .skip(1)
            .filter_map(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                (fields.get(3) == Some(&"0A")).then(|| fields[1])
            })
            .filter(|local| local.ends_with(&format!(":{port}")))
            .collect();
        assert_eq!(
            listening,
            [format!("0100007F:{port}")],
            "bound to loopback only"
        );
        // And the key is not on the server's command line.
        let cmdline =
            std::fs::read(format!("/proc/{}/cmdline", server.pid())).expect("the cmdline");
        assert!(!String::from_utf8_lossy(&cmdline).contains(&key));
    }
}

/// Row 9.9, A9.4 "exit".
#[test]
fn owned_server_is_killed_on_engine_exit() {
    let (mut engine, pid) = run_engine("exit");
    let status = engine.wait().expect("the engine exits");
    assert!(status.success(), "{status:?}");
    assert!(
        gone_within(pid, TEARDOWN),
        "the server outlived the engine by more than {TEARDOWN:?}"
    );
}

/// Row 9.10, A9.4 "crash": the engine forgets its server, so `Drop` never runs, and panics.
#[test]
fn owned_server_is_killed_on_engine_panic() {
    let (mut engine, pid) = run_engine("panic");
    let status = engine.wait().expect("the engine exits");
    assert_eq!(status.code(), Some(101), "a panic is exit code 101 (D13.2)");
    assert!(
        gone_within(pid, TEARDOWN),
        "the panic hook did not tear the server down"
    );
}

/// A9.4 "cancel": a SIGTERM to the engine takes its server with it, and the engine reports a
/// cancellation (exit code 3).
#[cfg(unix)]
#[test]
fn owned_server_is_killed_on_engine_sigterm() {
    let (mut engine, pid) = run_engine("wait");
    let sent = Command::new("kill")
        .args(["-TERM", &engine.id().to_string()])
        .status()
        .expect("kill runs");
    assert!(sent.success());
    let status = engine.wait().expect("the engine exits");
    assert_eq!(status.code(), Some(3), "{status:?}");
    assert!(gone_within(pid, TEARDOWN));
}

/// Row 9.13, clock-injected: `kill_if_idle` is handed its "now".
/// Carried over from Phase 9 (A9.4) into Phase 14: an engine killed **outright** — `SIGKILL` to
/// its pid alone, the way a segfault in PDFium or the OOM killer ends it — runs no hook, no handler
/// and no destructor, so only the kernel can end the server. On Linux it does, through
/// `PR_SET_PDEATHSIG` set by the exec trampoline (`oc_core::sidecar::orphan`).
#[cfg(target_os = "linux")]
#[test]
fn owned_server_does_not_outlive_a_sigkilled_engine() {
    let (mut engine, server) = run_engine("wait");
    engine.kill().expect("SIGKILL to the engine alone");
    let _ = engine.wait();
    assert!(
        gone_within(server, TEARDOWN),
        "llama-server {server} outlived an engine killed with SIGKILL"
    );
}

#[test]
fn idle_kill_after_120s() {
    let idle = Duration::from_secs(u64::try_from(T.llm.idle_kill_secs).expect("positive"));
    let mut server = start();
    let pid = server.pid();
    let t0 = Instant::now();
    server.call_started(t0);
    server.call_finished(t0);

    assert!(!server.kill_if_idle(t0 + idle - Duration::from_secs(1)));
    assert!(server.is_running());

    // A call in flight is never idle, however long it takes.
    let t1 = t0 + Duration::from_secs(30);
    server.call_started(t1);
    assert!(!server.kill_if_idle(t1 + idle * 10));
    assert!(server.is_running());
    let t2 = t1 + Duration::from_secs(5);
    server.call_finished(t2);
    assert!(!server.kill_if_idle(t2 + idle - Duration::from_secs(1)));

    assert!(
        server.kill_if_idle(t2 + idle),
        "idle for llm.idle_kill_secs"
    );
    assert!(!server.is_running());
    assert!(gone_within(pid, TEARDOWN));
    assert!(!server.kill_if_idle(t2 + idle * 2), "killed once");
}

/// A server that never answers `/health` is a failed start, not a hang, and is not left running.
#[test]
fn a_server_that_never_becomes_healthy_is_killed() {
    let port = oc_net::loopback::free_port().expect("a free port");
    let server = OwnedServer::spawn(&stub(), &spec(), port).expect("spawn");
    let pid = server.pid();
    let never = |_: &OwnedServer| Health::NotYet;
    let error = server
        .wait_healthy(&never, Duration::from_millis(300))
        .expect_err("never healthy");
    assert!(matches!(error, SidecarError::Unhealthy(_)), "{error:?}");
    drop(server);
    assert!(gone_within(pid, TEARDOWN));
}

/// A program that is not there is a spawn error naming it.
#[test]
fn a_missing_server_binary_is_a_spawn_error() {
    let error = OwnedServer::spawn(Path::new("/nonexistent/llama-server"), &spec(), 1)
        .expect_err("no such program");
    assert!(matches!(error, SidecarError::Spawn { .. }), "{error:?}");
}
