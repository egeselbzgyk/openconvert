//! Who kills the sidecar, whatever happens to the engine (PHASE 9 detail 5, RT C2/A5).
//!
//! Every server the engine starts is registered here, and torn down by whichever of these runs
//! first:
//!
//! - `OwnedServer`'s `Drop`, on a normal exit;
//! - the **panic hook**, which runs before unwinding and before a `panic = "abort"` abort — a `Drop`
//!   guard alone is not enough, because an aborting panic skips destructors;
//! - the **signal handler** — SIGINT/SIGTERM on Unix, the console-ctrl handler on Windows — which
//!   tears the children down and exits with `ExitCode::Cancelled`;
//! - and, when the desktop app supervises the engine, `kill(-pgid)` or the job object: the server is
//!   spawned into the engine's own process group, so the supervisor's group kill reaches it too.
//!
//! A supervisor whose own processes are not `Child`ren registered here — the desktop app's engine
//! process groups — adds its teardown with [`on_teardown`], and the same three paths run it.
//!
//! What this cannot cover without `unsafe` code of our own is an engine killed outright
//! (`SIGKILL`, a segfault in PDFium): Linux's `PR_SET_PDEATHSIG` must be set between `fork` and
//! `exec`, and a Windows nested job object is FFI. Both are Phase 14's hardening; see
//! `docs/DECISIONS_LOG.md`.

use std::collections::BTreeMap;
use std::process::Child;
use std::sync::{Mutex, MutexGuard, Once, PoisonError};

use crate::exit::ExitCode;

static CHILDREN: Mutex<BTreeMap<u32, Child>> = Mutex::new(BTreeMap::new());
static HOOKS: Mutex<Vec<fn()>> = Mutex::new(Vec::new());
static INSTALL: Once = Once::new();

/// The registry. A panic elsewhere never poisons it for good: nothing is done while it is held that
/// can panic, and teardown must work however the process is ending.
fn children() -> MutexGuard<'static, BTreeMap<u32, Child>> {
    CHILDREN.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Take ownership of `child` until it is killed. Installs the hooks first, so there is no moment
/// at which a child is registered and nothing would tear it down.
pub fn register(child: Child) -> u32 {
    install();
    let pid = child.id();
    children().insert(pid, child);
    pid
}

/// Whether the child `pid` is still running. A child that has exited is reaped and forgotten.
pub fn is_running(pid: u32) -> bool {
    let mut children = children();
    let running = children
        .get_mut(&pid)
        .is_some_and(|child| matches!(child.try_wait(), Ok(None)));
    if !running {
        children.remove(&pid);
    }
    running
}

/// Kill and reap the child `pid`, if it is still registered.
pub fn kill(pid: u32) {
    let child = children().remove(&pid);
    if let Some(child) = child {
        end(child);
    }
}

/// Take the child `pid` back from the registry and wait for it to exit, returning how it ended.
///
/// For a child that is expected to finish on its own — `tesseract` has closed its output and is
/// exiting. The child is removed *before* the wait so that the registry lock is never held across
/// a blocking call: the signal handler and the panic hook take the same lock, and a teardown that
/// waited behind a `wait` would not be a teardown. `None` when the pid is not registered.
pub fn wait(pid: u32) -> Option<std::process::ExitStatus> {
    let child = children().remove(&pid);
    child.and_then(|mut child| child.wait().ok())
}

/// Also run `hook` whenever everything is torn down — by [`kill_all`], the panic hook or the
/// signal handler. Installs the hooks first. A hook must not panic and must not block: it runs
/// inside a panic hook and a signal handler's thread.
pub fn on_teardown(hook: fn()) {
    install();
    HOOKS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(hook);
}

/// Kill and reap every registered child, then run every [`on_teardown`] hook.
pub fn kill_all() {
    let drained = std::mem::take(&mut *children());
    for child in drained.into_values() {
        end(child);
    }
    let hooks = HOOKS.lock().unwrap_or_else(PoisonError::into_inner).clone();
    for hook in hooks {
        hook();
    }
}

/// How many children are registered.
pub fn live_children() -> usize {
    children().len()
}

/// `SIGKILL` on Unix, `TerminateProcess` on Windows, then reap: `llama-server` holds nothing that
/// needs flushing, and its memory is back the moment it is gone (D8).
fn end(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Install the panic hook and the signal handler, once per process.
pub fn install() {
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            kill_all();
            previous(info);
        }));
        // SIGINT, SIGTERM and SIGHUP on Unix; Ctrl-C, Ctrl-Break and console close on Windows.
        // A signal to stop is a cancellation (D13.2).
        if let Err(error) = ctrlc::set_handler(|| {
            kill_all();
            std::process::exit(ExitCode::Cancelled.code());
        }) {
            tracing::warn!(%error, "no signal handler: a signalled engine may orphan its sidecar");
        }
    });
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    static TORN_DOWN: AtomicUsize = AtomicUsize::new(0);

    fn count() {
        TORN_DOWN.fetch_add(1, Ordering::SeqCst);
    }

    /// The desktop app's engine groups are not `Child`ren of this registry; the teardown that
    /// ends the registered children ends them too.
    #[test]
    fn teardown_hooks_run_with_the_registered_children() {
        on_teardown(count);
        let before = TORN_DOWN.load(Ordering::SeqCst);
        kill_all();
        assert_eq!(TORN_DOWN.load(Ordering::SeqCst), before + 1);
    }
}
