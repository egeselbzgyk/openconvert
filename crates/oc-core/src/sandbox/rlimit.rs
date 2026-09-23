//! `--max-memory` as a limit on the engine's address space (PHASE 14 detail 5, D13.2).
//!
//! Unix: `setrlimit(RLIMIT_AS)` at startup, before the PDF is opened, so an over-allocation ends
//! this process and not the user's session. The soft limit is set and the hard limit left alone:
//! the engine is protecting the machine from a file, not from itself.
//!
//! `RLIMIT_AS` caps *address space*, not resident memory, so it can fire earlier than a user
//! expects on an allocator-heavy workload; the default is deliberately generous (4 GiB), and the
//! report says which cap was in force and which flag sets it. Children inherit it — a `tesseract`
//! or `llama-server` the engine starts is bounded by the same number.
//!
//! Windows: the job object that would carry `JOB_OBJECT_LIMIT_PROCESS_MEMORY` is reached through
//! `win32job`, which does not expose that limit, and setting it ourselves is an `unsafe` FFI call
//! this project does not make (see `jobobject`). The desktop app's job object is the Windows cap
//! until that is decided; standalone, the engine reports the cap as unsupported.

use super::SandboxError;

/// The name the report and the error use.
pub const MECHANISM: &str = "RLIMIT_AS";

/// What `--max-memory` became.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryCap {
    /// The limit is in force: `bytes` of address space.
    Applied { bytes: u64 },
}

/// Limit the engine's address space to `bytes`. Idempotent; a later call replaces the limit.
///
/// Never raises the soft limit past the hard one: asking for more than the hard limit sets the
/// hard limit, which is the most this process can have anyway.
#[cfg(unix)]
pub fn apply_memory_cap(bytes: u64) -> Result<MemoryCap, SandboxError> {
    use rustix::process::{getrlimit, setrlimit, Resource, Rlimit};

    let hard = getrlimit(Resource::As).maximum;
    let soft = hard.map_or(bytes, |hard| bytes.min(hard));
    setrlimit(
        Resource::As,
        Rlimit {
            current: Some(soft),
            maximum: hard,
        },
    )
    .map_err(|error| SandboxError::Failed {
        mechanism: MECHANISM,
        message: error.to_string(),
    })?;
    Ok(MemoryCap::Applied { bytes: soft })
}

#[cfg(not(unix))]
pub fn apply_memory_cap(_bytes: u64) -> Result<MemoryCap, SandboxError> {
    Err(SandboxError::Unsupported {
        mechanism: MECHANISM,
    })
}

/// The address-space limit in force right now, read back from the kernel — what the report
/// records at the moment the PDF is opened. `None` when unlimited or unknowable.
#[cfg(unix)]
pub fn memory_cap_in_force() -> Option<u64> {
    rustix::process::getrlimit(rustix::process::Resource::As).current
}

#[cfg(not(unix))]
pub fn memory_cap_in_force() -> Option<u64> {
    None
}

// ---------------------------------------------------------------------------
// Tests. Each nextest test is its own process, so lowering this process's limit is contained;
// the integration test that the *engine* applies it before opening the PDF is row 14.7.
// ---------------------------------------------------------------------------

/// The cap is in force after it is applied, and it is a real limit: an allocation past it fails
/// (as an error here, because `try_reserve` asks; an ordinary allocation would end the process).
#[cfg(unix)]
#[test]
fn the_memory_cap_limits_the_address_space() {
    const GIB: u64 = 1 << 30;
    assert_eq!(
        apply_memory_cap(2 * GIB),
        Ok(MemoryCap::Applied { bytes: 2 * GIB })
    );
    assert_eq!(memory_cap_in_force(), Some(2 * GIB));

    let mut buffer: Vec<u8> = Vec::new();
    assert!(
        buffer
            .try_reserve_exact(usize::try_from(3 * GIB).unwrap_or(usize::MAX))
            .is_err(),
        "3 GiB was reserved under a 2 GiB address-space cap"
    );
    assert!(
        buffer
            .try_reserve_exact(usize::try_from(GIB / 4).unwrap_or(0))
            .is_ok(),
        "a quarter of a gibibyte fits"
    );
}
