#![forbid(unsafe_code)]
//! The Landlock probe (PHASE 14 rows 14.10 and 14.12).
//!
//! `oc-sandbox-probe [--read P]… [--rw P]… [--connect-port N]… -- <action>…`, where an action is
//! `write:<path>`, `read:<path>` or `connect:<host:port>`. It restricts itself to the scope set
//! through `oc_core::sandbox::landlock` — the engine's own call, from `main` before any thread
//! starts, as the engine does it — and then performs each action, printing one JSON line per step:
//! first the outcome, then `{"action", "target", "ok", "errno"}` for each action.

use std::io::{Read, Write};
use std::path::PathBuf;

use oc_core::sandbox::landlock::{landlock_self_restrict, LandlockOutcome};
use oc_core::sandbox::ScopeSet;

fn main() {
    let mut scope = ScopeSet::default();
    let mut args = std::env::args().skip(1);
    let mut actions = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--read" => scope.read.push(PathBuf::from(args.next().expect("a path"))),
            "--rw" => scope
                .read_write
                .push(PathBuf::from(args.next().expect("a path"))),
            "--connect-port" => scope
                .connect_ports
                .push(args.next().expect("a port").parse().expect("a port number")),
            "--" => actions.extend(args.by_ref()),
            other => panic!("unknown argument {other}"),
        }
    }

    let outcome = match landlock_self_restrict(&scope) {
        LandlockOutcome::Applied {
            abi,
            net_restricted,
        } => serde_json::json!({"status": "applied", "abi": abi, "net_restricted": net_restricted}),
        LandlockOutcome::Unsupported { reason } => {
            serde_json::json!({"status": "unsupported", "reason": reason})
        }
    };
    println!("{}", serde_json::json!({ "landlock": outcome }));

    for action in actions {
        let (kind, target) = action.split_once(':').expect("kind:target");
        let result: std::io::Result<()> = match kind {
            "write" => std::fs::File::create(target).and_then(|mut file| file.write_all(b"probe")),
            "read" => std::fs::File::open(target).and_then(|mut file| {
                let mut buffer = Vec::new();
                file.read_to_end(&mut buffer).map(|_| ())
            }),
            "connect" => std::net::TcpStream::connect(target).map(|_| ()),
            other => panic!("unknown action {other}"),
        };
        println!(
            "{}",
            serde_json::json!({
                "action": kind,
                "target": target,
                "ok": result.is_ok(),
                "errno": result.as_ref().err().and_then(std::io::Error::raw_os_error),
            })
        );
    }
}
