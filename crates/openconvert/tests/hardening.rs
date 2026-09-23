//! PHASE 14: the engine's hardening, observed from outside the process — the audit log a
//! conversion leaves alone, the caps a hostile file meets, the sandbox a conversion runs in.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_openconvert"))
}

fn fixture(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(format!("{name}.pdf"));
    assert!(
        path.is_file(),
        "missing fixture {}; run `cargo run -p xtask -- fixtures`",
        path.display()
    );
    path
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oc-hardening-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

/// Run the engine with its data directory (and so its audit log) inside `home`.
fn engine(home: &Path, args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .env("HOME", home)
        .env("XDG_DATA_HOME", home.join("data"))
        .env("LOCALAPPDATA", home.join("data"))
        .output()
        .expect("the engine runs")
}

/// Row 14.21, the conversion half: a conversion opens no connection, so the audit log is exactly as
/// it was — not appended to, not created, not rotated. The download half is `oc-net`'s
/// `net_audit_log_records_downloads_and_nothing_else`.
#[test]
fn a_conversion_appends_nothing_to_the_network_audit_log() {
    let home = scratch("audit");
    let log = home.join("data/openconvert/network-audit.log");
    std::fs::create_dir_all(log.parent().expect("a parent")).expect("data dir");
    let before = b"{\"ts\":\"2026-09-23T00:00:00Z\",\"host\":\"huggingface.co\",\"purpose\":\"download\",\"bytes\":1,\"outcome\":\"ok\",\"loopback\":false}\n";
    std::fs::write(&log, before).expect("a log with one line");

    let output = home.join("book.epub");
    let converted = engine(
        &home,
        &[
            "convert",
            &fixture("f01_prose_single_column").display().to_string(),
            "-o",
            &output.display().to_string(),
            "--ocr",
            "never",
        ],
    );
    assert_eq!(
        converted.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&converted.stderr)
    );
    assert!(output.is_file());
    assert_eq!(
        std::fs::read(&log).expect("the log"),
        before,
        "a conversion wrote to the network audit log"
    );
    let _ = std::fs::remove_dir_all(&home);
}
