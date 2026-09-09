//! Process exit codes (IMPLEMENTATION_PLAN §2.4).
//!
//! A supervisor decides what happened from the exit code and the `done` event, never by
//! parsing stderr text.

/// The engine's exit codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    /// The output and the report were both written.
    Ok = 0,
    /// The conversion failed. A report was still written.
    Failed = 1,
    /// Usage, job-spec or configuration error. Nothing was attempted.
    Usage = 2,
    /// Cancelled on request. Temporary files were removed.
    Cancelled = 3,
}

impl ExitCode {
    pub fn code(self) -> i32 {
        self as i32
    }
}
