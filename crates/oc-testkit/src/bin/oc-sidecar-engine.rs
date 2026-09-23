//! A minimal engine for the sidecar teardown tests (PHASE 9 rows 9.9 and 9.10).
//!
//! It does what the real engine does before it asks a model anything — pick a loopback port, start
//! an `OwnedServer` through `oc_core::sidecar`, wait for `/health` through `oc-net` — prints the
//! server's pid, waits for a line on stdin, and then ends the way it is told to:
//!
//! - `exit`: returns from `main`, as a finished conversion does;
//! - `panic`: forgets the server first, so its `Drop` cannot run, and panics — only the panic hook
//!   can tear the server down;
//! - `wait`: sleeps until a signal arrives — only the signal handler can tear the server down.
//!
//! `oc_core::sidecar` is what is under test; this binary is its caller, the same shape as
//! `openconvert` will be once Phase 10 asks a model.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use oc_core::sidecar::llama::ServerSpec;
use oc_core::sidecar::server::{Health, OwnedServer};
use oc_net::transport::HttpTransport;

fn main() {
    // As the real engine does: its children get the parent-death signal (PHASE 14).
    oc_core::sidecar::orphan::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mode, program) = (args[0].as_str(), PathBuf::from(&args[1]));

    let port = oc_net::loopback::free_port().expect("a free port");
    let spec = ServerSpec {
        model: PathBuf::from("no-model.gguf"),
        context: 8192,
        threads: 1,
        cache_reuse: true,
        context_checkpoints: None,
    };
    let server = OwnedServer::spawn(&program, &spec, port).expect("spawn");
    server
        .wait_healthy(&probe, Duration::from_secs(10))
        .expect("healthy");
    println!("pid {}", server.pid());
    std::io::stdout().flush().expect("flush");
    // Hold still until the test has seen the server running: otherwise "exit" could end the
    // server before anyone looked at it, and the test would prove nothing.
    let mut go = String::new();
    std::io::stdin().read_line(&mut go).expect("the go line");

    match mode {
        "exit" => {}
        "panic" => {
            std::mem::forget(server);
            panic!("forced panic with a live sidecar");
        }
        "wait" => loop {
            std::thread::sleep(Duration::from_secs(1));
        },
        other => panic!("unknown mode {other}"),
    }
    oc_core::sidecar::supervise::settle();
}

/// `GET /health` over loopback: 200 is ready, anything else is not yet.
pub fn probe(server: &OwnedServer) -> Health {
    let transport = HttpTransport::new(&server.base_url(), None).expect("a loopback base");
    match transport.get("/health", Duration::from_secs(1)) {
        Ok(_) => Health::Ready,
        Err(_) => Health::NotYet,
    }
}
