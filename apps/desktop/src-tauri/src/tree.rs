//! An engine's whole process tree, owned and ended as one (D13.2, ARCHITECTURE §8.2, PHASE 9
//! detail 5).
//!
//! An engine can start processes of its own — `tesseract` for OCR, and a `llama-server` when it
//! owns one (the CLI's case; the app hands its engines the app's own server). Ending a job has to
//! end all of them, however the engine itself ends:
//!
//! - **Unix.** The engine is started as the leader of a new process group
//!   (`CommandExt::process_group(0)` in [`crate::engine::ProcessLauncher`]), and what it spawns stays
//!   in that group. Ending the tree is `kill(-pgid, SIGKILL)`. When the engine exits by itself —
//!   including a crash that ran none of its own teardown — the group is **swept before the engine
//!   is reaped**: until it is reaped, the engine's zombie holds the group id, so a sweep can never
//!   reach another group that has reused the number. `waitid(…, WNOWAIT)` is what looks at the
//!   engine without reaping it. macOS takes this path too; **unverified here**.
//! - **Windows.** Each engine is assigned to a job object of its own with `KILL_ON_JOB_CLOSE`.
//!   Closing the job ends everything in it, and Windows closes it when the app dies, however it
//!   dies. The engine is assigned just after it is spawned; the engine reads its job spec before
//!   it starts anything, so nothing it starts escapes the job. **Unverified here** (no Windows).
//!
//! Every live Unix tree is also listed process-wide, so the app's exit and the panic hook and
//! signal handler of `oc_core::sidecar::supervise` end them all ([`end_all`]). The `unsafe` this
//! needs lives in `rustix` and `win32job`, whose APIs are safe; this crate stays
//! `forbid(unsafe_code)`.

use std::process::Child;

pub use imp::{end_all, Tree};

#[cfg(unix)]
mod imp {
    use std::collections::BTreeSet;
    use std::process::{Child, ExitStatus};
    use std::sync::{Mutex, MutexGuard, PoisonError};

    use rustix::process::{kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions};

    /// The group ids of every engine not yet reaped. Nothing that can panic runs while it is held,
    /// so the panic hook can always take it.
    static LIVE: Mutex<BTreeSet<i32>> = Mutex::new(BTreeSet::new());

    fn live() -> MutexGuard<'static, BTreeSet<i32>> {
        LIVE.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// One engine's process group.
    #[derive(Debug)]
    pub struct Tree {
        /// The engine, which leads the group. `None` once it has been reaped.
        leader: Option<Pid>,
    }

    impl Tree {
        /// Take charge of `child`'s group. `child` must have been spawned as a group leader.
        pub fn adopt(child: &Child) -> std::io::Result<Self> {
            let leader = Pid::from_child(child);
            live().insert(leader.as_raw_nonzero().get());
            Ok(Self {
                leader: Some(leader),
            })
        }

        /// End every process in the tree. Safe to repeat.
        pub fn end(&mut self) {
            if let Some(leader) = self.leader {
                let _ = kill_process_group(leader, Signal::KILL);
            }
        }

        /// `child.try_wait()`, with the group swept first if the engine has exited: whatever it
        /// left running goes before the engine is reaped and its group id is free for reuse.
        pub fn try_wait(&mut self, child: &mut Child) -> std::io::Result<Option<ExitStatus>> {
            match self.leader_exited() {
                Some(false) => return Ok(None),
                Some(true) => {
                    self.end();
                    self.release();
                }
                // The system could not say: reap as usual, and never signal the group again once
                // the engine is reaped.
                None => {}
            }
            let status = child.try_wait()?;
            if status.is_some() {
                self.release();
            }
            Ok(status)
        }

        /// Whether the engine has exited, asked without reaping it: `Some(true)` exited,
        /// `Some(false)` still running, `None` when the system could not say.
        fn leader_exited(&self) -> Option<bool> {
            let leader = self.leader?;
            let options = WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT;
            match waitid(WaitId::Pid(leader), options) {
                Ok(status) => Some(status.is_some()),
                Err(_) => None,
            }
        }

        /// The engine is about to be reaped: from here on its group id may be reused, so the tree
        /// never signals it again.
        pub fn release(&mut self) {
            if let Some(leader) = self.leader.take() {
                live().remove(&leader.as_raw_nonzero().get());
            }
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            self.release();
        }
    }

    /// End every engine tree that is still live: the app is ending.
    pub fn end_all() {
        let groups: Vec<i32> = live().iter().copied().collect();
        for group in groups {
            if let Some(leader) = Pid::from_raw(group) {
                let _ = kill_process_group(leader, Signal::KILL);
            }
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::os::windows::io::AsRawHandle;
    use std::process::{Child, ExitStatus};

    use win32job::{ExtendedLimitInfo, Job};

    /// One engine's job object.
    #[derive(Debug)]
    pub struct Tree {
        job: Option<Job>,
    }

    impl Tree {
        /// A job with `KILL_ON_JOB_CLOSE`, and `child` in it.
        pub fn adopt(child: &Child) -> std::io::Result<Self> {
            let mut limits = ExtendedLimitInfo::new();
            limits.limit_kill_on_job_close();
            let job = Job::create_with_limit_info(&limits).map_err(std::io::Error::other)?;
            job.assign_process(child.as_raw_handle() as isize)
                .map_err(std::io::Error::other)?;
            Ok(Self { job: Some(job) })
        }

        /// End every process in the tree: closing the job's last handle does it.
        pub fn end(&mut self) {
            self.job = None;
        }

        /// `child.try_wait()`; once the engine has exited, the job is closed, which ends whatever
        /// it left running. A job cannot be reused by another tree, so no look before the reap is
        /// needed.
        pub fn try_wait(&mut self, child: &mut Child) -> std::io::Result<Option<ExitStatus>> {
            let status = child.try_wait()?;
            if status.is_some() {
                self.end();
            }
            Ok(status)
        }

        /// Nothing to forget: the job ends with its handle.
        pub fn release(&mut self) {}
    }

    /// Windows ends every job when the app's handles close, however the app ends.
    pub fn end_all() {}
}

/// Put `child`, just spawned as a group leader, in charge of a [`Tree`]. On a system that cannot
/// make one — no job object could be created — the engine still runs, as a single process the app
/// kills directly.
pub fn adopt(child: &Child) -> Option<Tree> {
    Tree::adopt(child).ok()
}
