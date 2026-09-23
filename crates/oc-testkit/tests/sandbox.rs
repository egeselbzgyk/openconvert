//! PHASE 14 rows 14.10 and 14.12: Landlock, observed from inside a restricted process.
//!
//! The probe is `oc-sandbox-probe`, which restricts itself through the engine's own
//! `oc_core::sandbox::landlock` call and then tries what it is told. Whether the kernel *should*
//! enforce is decided independently of the code under test — from the kernel's own list of active
//! security modules — so each test has two honest branches: enforced, with the denials asserted,
//! or not, with the recorded skip asserted.
//!
//! Landlock is a Linux security module, so the whole file is Linux-only: on macOS and Windows
//! there is nothing to observe, and helpers compiled without their tests would be dead code.
#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::Command;

/// `EACCES`: what Landlock answers for a denied filesystem access and a denied TCP connect.
const EACCES: i64 = 13;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oc-sandbox-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

/// Landlock arrived in Linux 5.13 (the plan's rows 14.10 and A14.4 key on exactly this).
const LANDLOCK_SINCE: (u32, u32) = (5, 13);

/// Whether this kernel should run Landlock — the kernel's word, not the code under test's: the
/// active security modules when securityfs is mounted, the release number when it is not (a
/// container, like the one this was written in).
fn kernel_runs_landlock() -> bool {
    if let Ok(lsm) = std::fs::read_to_string("/sys/kernel/security/lsm") {
        return lsm.split(',').any(|name| name.trim() == "landlock");
    }
    let release = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    let mut numbers = release
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|part| part.parse::<u32>().ok());
    let version = (numbers.next().unwrap_or(0), numbers.next().unwrap_or(0));
    version >= LANDLOCK_SINCE
}

/// Run the probe; the first line is the outcome, the rest one per action.
fn probe(args: &[&str]) -> (serde_json::Value, Vec<serde_json::Value>) {
    let output = Command::new(env!("CARGO_BIN_EXE_oc-sandbox-probe"))
        .args(args)
        .output()
        .expect("the probe runs");
    assert!(output.status.success(), "{output:?}");
    let mut lines = String::from_utf8(output.stdout)
        .expect("utf-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a JSON line"))
        .collect::<Vec<serde_json::Value>>()
        .into_iter();
    let outcome = lines.next().expect("the outcome line")["landlock"].clone();
    (outcome, lines.collect())
}

fn path(p: &Path) -> String {
    p.display().to_string()
}

/// Row 14.10. Inside the scope a write succeeds; outside it fails with `EACCES` — from inside the
/// restricted process, which is the only place it can be seen. Both halves matter: a scope set that
/// forgot the output directory would turn every conversion into a permission error on exactly the
/// kernels that run Landlock (the plan's failure mode), so the permitted write is asserted too.
#[test]
fn landlock_applies_on_supported_kernel() {
    let dir = scratch("fs");
    let inside = dir.join("output");
    let outside = dir.join("elsewhere");
    std::fs::create_dir_all(&inside).expect("inside");
    std::fs::create_dir_all(&outside).expect("outside");
    let input = dir.join("book.pdf");
    std::fs::write(&input, b"%PDF-1.7").expect("input");
    let secret = outside.join("secret.txt");
    std::fs::write(&secret, b"not for the engine").expect("secret");

    let (outcome, steps) = probe(&[
        "--read",
        &path(&input),
        "--rw",
        &path(&inside),
        "--",
        &format!("write:{}", path(&inside.join("book.epub"))),
        &format!("write:{}", path(&outside.join("escaped.epub"))),
        &format!("read:{}", path(&input)),
        &format!("read:{}", path(&secret)),
    ]);

    if !kernel_runs_landlock() {
        assert_eq!(outcome["status"], "unsupported", "{outcome}");
        assert!(outcome["reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty()));
        return;
    }
    assert_eq!(outcome["status"], "applied", "{outcome}");
    assert!(
        outcome["abi"].as_i64().is_some_and(|abi| abi >= 1),
        "{outcome}"
    );

    let [write_in, write_out, read_in, read_out] = steps.as_slice() else {
        panic!("four steps, got {steps:?}");
    };
    assert_eq!(write_in["ok"], true, "a write inside the scope: {write_in}");
    assert_eq!(
        write_out["ok"], false,
        "a write outside the scope: {write_out}"
    );
    assert_eq!(write_out["errno"], EACCES, "{write_out}");
    assert!(!outside.join("escaped.epub").exists());
    assert_eq!(read_in["ok"], true, "the input is readable: {read_in}");
    assert_eq!(
        read_out["ok"], false,
        "a read outside the scope: {read_out}"
    );
    assert_eq!(read_out["errno"], EACCES, "{read_out}");
}

/// Row 14.12. On ABI ≥ 4 a `connect()` from inside is refused, unless the scope names the port;
/// below ABI 4 the outcome says TCP is not restricted, and that line is what is asserted.
#[test]
fn landlock_blocks_tcp_connect_on_abi4() {
    // Listeners in this (unrestricted) process: one the probe may reach, one it may not.
    let refused = std::net::TcpListener::bind("127.0.0.1:0").expect("a listener");
    let allowed = std::net::TcpListener::bind("127.0.0.1:0").expect("a listener");
    let refused_port = refused.local_addr().expect("addr").port();
    let allowed_port = allowed.local_addr().expect("addr").port();

    let (outcome, steps) = probe(&[
        "--connect-port",
        &allowed_port.to_string(),
        "--",
        &format!("connect:127.0.0.1:{refused_port}"),
        &format!("connect:127.0.0.1:{allowed_port}"),
    ]);

    let [to_refused, to_allowed] = steps.as_slice() else {
        panic!("two steps, got {steps:?}");
    };
    assert_eq!(to_allowed["ok"], true, "the scope's own port: {to_allowed}");
    if outcome["status"] == "applied" && outcome["net_restricted"] == true {
        assert!(
            outcome["abi"].as_i64().is_some_and(|abi| abi >= 4),
            "{outcome}"
        );
        assert_eq!(
            to_refused["ok"], false,
            "a connect outside the scope: {to_refused}"
        );
        assert_eq!(to_refused["errno"], EACCES, "{to_refused}");
    } else {
        // The skip line: either no Landlock at all, or one too old to handle TCP.
        assert!(
            outcome["status"] == "unsupported" || outcome["net_restricted"] == false,
            "{outcome}"
        );
        assert!(
            !kernel_runs_landlock() || outcome["abi"].as_i64().is_some_and(|abi| abi < 4),
            "a Landlock kernel at ABI >= 4 must restrict TCP: {outcome}"
        );
    }
}
