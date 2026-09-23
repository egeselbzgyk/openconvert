//! Windows: a job object that takes the engine's children with it (PHASE 14, D13.2, SECURITY §3).
//!
//! The engine creates a job with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and assigns every child it
//! starts — `llama-server`, `tesseract` — to it. The engine holds the only handle, so when the
//! engine ends in any way (exit, panic, `TerminateProcess`, a crash), the handle closes and the
//! children end with it. Nested inside the desktop app's own job on Windows 8 and later.
//!
//! Through `win32job`'s safe API: no `unsafe` of ours. That API does not expose
//! `JOB_OBJECT_LIMIT_PROCESS_MEMORY`, so this job carries no memory limit (see `rlimit`).
//!
//! Compiled and type-checked for `x86_64-pc-windows-msvc`; run on Windows only by CI, which is
//! unverified on the machine this was written on.

/// The name the report uses.
pub const MECHANISM: &str = "job object";

#[cfg(windows)]
mod imp {
    use std::sync::OnceLock;

    use super::super::SandboxError;
    use super::MECHANISM;

    /// The engine's own job, created on first use and never closed before the process ends:
    /// closing it is what ends the children.
    static JOB: OnceLock<Option<win32job::Job>> = OnceLock::new();

    fn job() -> Result<&'static win32job::Job, SandboxError> {
        JOB.get_or_init(|| {
            let mut info = win32job::ExtendedLimitInfo::new();
            info.limit_kill_on_job_close();
            win32job::Job::create_with_limit_info(&info).ok()
        })
        .as_ref()
        .ok_or(SandboxError::Failed {
            mechanism: MECHANISM,
            message: "CreateJobObject failed".to_owned(),
        })
    }

    /// Put `child` in the engine's job, so it cannot outlive the engine.
    pub fn adopt(child: &std::process::Child) -> Result<(), SandboxError> {
        use std::os::windows::io::AsRawHandle;
        let handle = child.as_raw_handle() as isize;
        job()?
            .assign_process(handle)
            .map_err(|error| SandboxError::Failed {
                mechanism: MECHANISM,
                message: error.to_string(),
            })
    }
}

#[cfg(windows)]
pub use imp::adopt;

/// Elsewhere there is no job object; the parent-death signal (Linux) and the process group do
/// this work (`sidecar::orphan`).
#[cfg(not(windows))]
pub fn adopt(_child: &std::process::Child) -> Result<(), super::SandboxError> {
    Err(super::SandboxError::Unsupported {
        mechanism: MECHANISM,
    })
}
