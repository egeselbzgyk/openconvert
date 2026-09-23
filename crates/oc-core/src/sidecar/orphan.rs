//! Children that cannot outlive the engine, however the engine ends (PHASE 14; carried over from
//! PHASE 9 detail 5 and PHASE 13 row 13.19; D13.2, SECURITY §3).
//!
//! `supervise` tears children down on every ending that runs code: `Drop`, the panic hook, the
//! signal handler. An engine killed **outright** — `SIGKILL`, a segfault in PDFium, the OOM
//! killer — runs none of them, and only the kernel can act:
//!
//! - **Linux:** the child asks for `PR_SET_PDEATHSIG` (SIGTERM) before it becomes `tesseract` or
//!   `llama-server`. That must happen between `fork` and `exec`, where `std` offers only the
//!   `unsafe` `pre_exec` hook; this project writes no `unsafe`. So the engine starts its own binary
//!   as a trampoline — `openconvert __oc-exec-child <engine pid> -- <program> <args…>` — which
//!   sets the signal through rustix's safe `set_parent_process_death_signal`, checks the engine is
//!   still its parent (it may have died in the instant before), and `exec`s the program in its own
//!   place: same pid, same environment, same stdio. The signal follows the *thread* that spawned
//!   the child, so children are spawned from threads that outlive them (the caller's, which waits).
//! - **Windows:** the child is put in the engine's `KILL_ON_JOB_CLOSE` job (`sandbox::jobobject`).
//! - **macOS:** there is no `PDEATHSIG`. Children stay in the engine's process group, so the
//!   supervisor's `kill(-pgid)` reaches them, and the app cleans up on `applicationWillTerminate`.
//!
//! **Opt-in.** Only a binary that calls [`init`] first thing in `main` can be a trampoline, so only
//! such a binary gets one; any other caller of `oc-core` (a test, the desktop app) spawns directly,
//! exactly as before.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::OnceLock;

/// The hidden first argument that makes an engine binary a trampoline.
pub const TRAMPOLINE_ARG: &str = "__oc-exec-child";
/// Separates the trampoline's own arguments from the program it becomes.
const SEPARATOR: &str = "--";
/// What a shell reports for a command that could not be executed (POSIX `sh`, "Exit Status").
const EXEC_FAILED: i32 = 126;

/// This binary, when it can act as the trampoline.
static TRAMPOLINE: OnceLock<PathBuf> = OnceLock::new();

/// Call first in `main`. In a trampoline invocation this never returns: it becomes the child.
/// Otherwise it records this binary as the trampoline for the children it will start.
pub fn init() {
    let mut args = std::env::args_os();
    let _binary = args.next();
    if args.next().as_deref() == Some(OsStr::new(TRAMPOLINE_ARG)) {
        become_child(args.collect());
    }
    if let Ok(binary) = std::env::current_exe() {
        let _ = TRAMPOLINE.set(binary);
    }
}

/// Whether children started through [`command`] get the parent-death signal.
pub fn guarded() -> bool {
    cfg!(target_os = "linux") && TRAMPOLINE.get().is_some()
}

/// The command that starts `program` so that it cannot outlive this process. Add arguments,
/// environment and stdio to it as to `Command::new(program)`.
pub fn command(program: &Path) -> Command {
    #[cfg(target_os = "linux")]
    if let Some(trampoline) = TRAMPOLINE.get() {
        let mut command = Command::new(trampoline);
        command
            .arg(TRAMPOLINE_ARG)
            .arg(std::process::id().to_string())
            .arg(SEPARATOR)
            .arg(program);
        return command;
    }
    Command::new(program)
}

/// The program a [`command`] runs, for messages: the trampoline's target, not the trampoline.
pub fn program_of(command: &Command) -> String {
    let mut args = command.get_args();
    if args.next() == Some(OsStr::new(TRAMPOLINE_ARG)) {
        let _parent = args.next();
        let _separator = args.next();
        if let Some(program) = args.next() {
            return program.to_string_lossy().into_owned();
        }
    }
    command.get_program().to_string_lossy().into_owned()
}

/// After a spawn: on Windows, into the engine's job object. Elsewhere nothing is left to do.
/// Never fails the spawn: a child that could not be adopted is still torn down by `supervise` on
/// every ending that runs code.
pub fn adopt(child: &Child) {
    #[cfg(windows)]
    if let Err(error) = crate::sandbox::jobobject::adopt(child) {
        tracing::warn!(%error, "a child is not in the engine's job object");
    }
    #[cfg(not(windows))]
    let _ = child;
}

/// The trampoline: `<engine pid> -- <program> <args…>`.
fn become_child(args: Vec<OsString>) -> ! {
    let mut args = args.into_iter();
    let parent = args
        .next()
        .and_then(|pid| pid.to_str().and_then(|pid| pid.parse::<i32>().ok()));
    let separator = args.next();
    let program = args.next();
    let (Some(parent), Some(program)) = (parent, program) else {
        exit_failed("the exec trampoline was started without a parent and a program");
    };
    if separator.as_deref() != Some(OsStr::new(SEPARATOR)) {
        exit_failed("the exec trampoline's arguments are malformed");
    }
    set_death_signal(parent);
    let error = exec(&program, args);
    exit_failed(&format!(
        "could not execute {}: {error}",
        Path::new(&program).display()
    ));
}

#[cfg(target_os = "linux")]
fn set_death_signal(parent: i32) {
    use rustix::process::{getppid, set_parent_process_death_signal, Signal};

    // Best effort: a kernel that refuses leaves the child exactly as unguarded as before.
    let _ = set_parent_process_death_signal(Some(Signal::TERM));
    // The engine may have died between our `fork` and the call above, in which case the signal
    // will never come: we have already been re-parented, and must not become the child at all.
    if getppid().map(|pid| pid.as_raw_nonzero().get()) != Some(parent) {
        std::process::exit(crate::exit::ExitCode::Cancelled.code());
    }
}

#[cfg(not(target_os = "linux"))]
fn set_death_signal(_parent: i32) {}

#[cfg(unix)]
fn exec(program: &OsStr, args: impl Iterator<Item = OsString>) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    Command::new(program).args(args).exec()
}

#[cfg(not(unix))]
fn exec(program: &OsStr, args: impl Iterator<Item = OsString>) -> std::io::Error {
    // Never reached: `command` only uses the trampoline on Linux. Run and pass the status on.
    match Command::new(program).args(args).status() {
        Ok(status) => std::process::exit(status.code().unwrap_or(EXEC_FAILED)),
        Err(error) => error,
    }
}

fn exit_failed(message: &str) -> ! {
    eprintln!("openconvert: {message}");
    std::process::exit(EXEC_FAILED);
}

#[test]
fn an_unguarded_command_is_the_program_itself() {
    // This test binary never called `init`, so it is not a trampoline.
    let command = command(Path::new("/usr/bin/tesseract"));
    assert_eq!(command.get_program(), "/usr/bin/tesseract");
    assert_eq!(program_of(&command), "/usr/bin/tesseract");
    assert!(!guarded());
}

#[test]
fn a_trampoline_command_names_its_target() {
    let mut command = Command::new("/opt/openconvert");
    command
        .arg(TRAMPOLINE_ARG)
        .arg("42")
        .arg(SEPARATOR)
        .arg("/usr/bin/tesseract")
        .arg("in.png");
    assert_eq!(program_of(&command), "/usr/bin/tesseract");
}
