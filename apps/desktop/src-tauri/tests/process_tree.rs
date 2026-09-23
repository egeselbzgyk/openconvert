//! An engine is a process tree, and the app ends the whole tree (D13.2, ARCHITECTURE §8.2, PHASE 9
//! detail 5; A9.4 for an engine the app supervises): a kill takes what the engine started with it,
//! an engine that crashes leaves nothing running, and quitting the app ends every engine.
//!
//! The "engine" is a shell script given the launcher's one argument, the spec path. It starts a
//! long `sleep` — standing in for a `tesseract` or a `llama-server` of its own — writes both pids
//! beside the spec, and then waits, or kills itself with `SIGKILL` the way a crash in PDFium would
//! end it: without running any teardown of its own. Linux only: the tests read `/proc`. The same
//! code runs on macOS; there, and for Windows' job objects, this is unverified here.
#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use openconvert_desktop::engine::{Launch, ProcessLauncher, Running};
use openconvert_desktop::fs_scope::AppDirs;
use openconvert_desktop::tree;

/// A9.4's bound.
const TEARDOWN: Duration = Duration::from_secs(2);

const ENGINE: &str = r#"#!/bin/sh
dir=$(dirname "$1")
name=$(basename "$1" .json)
sleep 30 &
echo $! > "$dir/$name.child"
echo $$ > "$dir/$name.engine"
case "$(cat "$1")" in
  crash) kill -9 $$ ;;
  *) wait ;;
esac
"#;

struct Fixture {
    dirs: AppDirs,
    launcher: ProcessLauncher,
}

fn fixture(name: &str) -> Fixture {
    let root = std::env::temp_dir().join(format!("oc-desktop-tree-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dirs = AppDirs::under(&root).expect("made");
    let engine = root.join("engine.sh");
    std::fs::write(&engine, ENGINE).expect("written");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&engine, std::fs::Permissions::from_mode(0o755))
            .expect("executable");
    }
    let launcher = ProcessLauncher::new(engine, dirs.jobs.clone());
    Fixture { dirs, launcher }
}

/// Start job `name` doing `mode`; returns it and the pids of the engine and its child.
fn start(fixture: &Fixture, name: &str, mode: &str) -> (Box<dyn Running>, u32, u32) {
    let spec = fixture.dirs.jobs.join(format!("{name}.json"));
    std::fs::write(&spec, mode).expect("written");
    let running = fixture
        .launcher
        .launch(&spec, None, Box::new(|_| {}))
        .expect("launched");
    let engine = read_pid(&fixture.dirs.jobs.join(format!("{name}.engine")));
    let child = read_pid(&fixture.dirs.jobs.join(format!("{name}.child")));
    (running, engine, child)
}

fn read_pid(path: &Path) -> u32 {
    let start = Instant::now();
    loop {
        if let Some(pid) = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| text.trim().parse().ok())
        {
            return pid;
        }
        assert!(start.elapsed() < TEARDOWN, "no pid at {}", path.display());
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Whether a process with this pid still exists. A zombie counts as gone: it holds no memory.
fn alive(pid: u32) -> bool {
    match std::fs::read_to_string(PathBuf::from(format!("/proc/{pid}/stat"))) {
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

/// Poll the supervisor's way until the engine has been reaped. A reap only counts inside `bound`:
/// `try_wait` also waits for the engine's stderr to close, which a child left running would hold
/// open for as long as it lives.
fn reaped_within(running: &mut Box<dyn Running>, bound: Duration) -> bool {
    let start = Instant::now();
    loop {
        let reaped = running.try_wait().expect("waits").is_some();
        if start.elapsed() > bound {
            return false;
        }
        if reaped {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The kill deadline (`ipc.kill_after_secs`) ends the engine and everything it started.
#[test]
fn a_killed_engine_takes_its_whole_process_tree_with_it() {
    let fixture = fixture("kill");
    let (mut running, engine, child) = start(&fixture, "job-1", "wait");
    assert!(alive(engine) && alive(child));

    running.kill().expect("killed");
    assert!(reaped_within(&mut running, TEARDOWN));
    assert!(gone_within(engine, TEARDOWN), "the engine");
    assert!(gone_within(child, TEARDOWN), "what the engine started");
}

/// An engine that dies without its own teardown — `SIGKILL`, a segfault in PDFium — leaves
/// nothing running: the app sweeps its group before reaping it.
#[test]
fn an_engine_that_crashes_leaves_nothing_behind() {
    let fixture = fixture("crash");
    let (mut running, engine, child) = start(&fixture, "job-1", "crash");

    assert!(reaped_within(&mut running, TEARDOWN), "the engine ended");
    assert!(!alive(engine));
    assert!(gone_within(child, TEARDOWN), "its child did not outlive it");
}

/// Quitting the app — its exit, or `supervise`'s panic hook and signal handler — ends every
/// engine that is still running, and what each started.
#[test]
fn quitting_the_app_ends_every_running_engine() {
    let fixture = fixture("quit");
    let (mut first, engine_1, child_1) = start(&fixture, "job-1", "wait");
    let (mut second, engine_2, child_2) = start(&fixture, "job-2", "wait");

    tree::end_all();
    for pid in [engine_1, child_1, engine_2, child_2] {
        assert!(gone_within(pid, TEARDOWN), "pid {pid}");
    }
    assert!(reaped_within(&mut first, TEARDOWN));
    assert!(reaped_within(&mut second, TEARDOWN));
}

/// Dropping a running engine's handle — a job removed mid-run — ends its tree too.
#[test]
fn a_dropped_engine_handle_ends_its_tree() {
    let fixture = fixture("drop");
    let (running, engine, child) = start(&fixture, "job-1", "wait");
    drop(running);
    assert!(gone_within(engine, TEARDOWN));
    assert!(gone_within(child, TEARDOWN));
}
