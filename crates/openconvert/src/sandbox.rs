//! What the engine did to restrict itself, and what the report says about it (PHASE 14 details 5
//! and 7).
//!
//! The mechanisms are `oc_core::sandbox`'s; this is the order the engine applies them in and the
//! record it keeps. **None of it ever fails a conversion**: a mechanism that could not be applied
//! is a line in the report (`status: "unsupported"` and why), never an error for the user.

use serde::Serialize;

use oc_core::limits::Limits;
use oc_core::sandbox::rlimit;

/// The report's `sandbox` section.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SandboxReport {
    pub memory: MemoryReport,
}

/// `--max-memory`, as applied and as found in force when the PDF was opened.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct MemoryReport {
    /// `RLIMIT_AS` on Unix.
    pub mechanism: &'static str,
    /// `applied`, `unsupported` or `failed`.
    pub status: &'static str,
    /// What `--max-memory` (or the job spec's `limits.max_memory_bytes`) asked for.
    pub requested_bytes: u64,
    /// The limit read back from the kernel immediately before the PDF was read — not the flag
    /// echoed. `None` when there is none to read.
    pub in_force_at_open: Option<u64>,
    /// The flag a user changes it with, printed beside the number (PHASE 14 failure modes).
    pub flag: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The flag that sets the memory cap.
pub const MAX_MEMORY_FLAG: &str = "--max-memory";

/// Apply the memory cap. Call before the PDF is opened — before PDFium is bound, so its
/// allocations are under it too.
pub fn apply_memory_cap(limits: &Limits) -> MemoryReport {
    let requested = limits.max_memory_bytes;
    let (status, reason) = match rlimit::apply_memory_cap(requested) {
        Ok(rlimit::MemoryCap::Applied { .. }) => ("applied", None),
        Err(error @ oc_core::sandbox::SandboxError::Unsupported { .. }) => {
            ("unsupported", Some(error.to_string()))
        }
        Err(error) => ("failed", Some(error.to_string())),
    };
    MemoryReport {
        mechanism: rlimit::MECHANISM,
        status,
        requested_bytes: requested,
        in_force_at_open: None,
        flag: MAX_MEMORY_FLAG,
        reason,
    }
}

impl MemoryReport {
    /// Read the limit back from the kernel. Call immediately before the PDF's bytes are read.
    pub fn observe_at_open(&mut self) {
        self.in_force_at_open = rlimit::memory_cap_in_force();
    }
}

/// Binary multiples, the only ones `--max-memory` accepts: `4GiB` is unambiguous, `4GB` is not.
const UNITS: [(&str, u32); 5] = [("TiB", 40), ("GiB", 30), ("MiB", 20), ("KiB", 10), ("B", 0)];

/// Parse `--max-memory`'s value: a byte count, optionally with a binary unit (`4GiB`, `512MiB`).
/// `None` for anything else, including a value that overflows.
pub fn parse_bytes(value: &str) -> Option<u64> {
    let value = value.trim();
    let (digits, shift) = UNITS
        .iter()
        .find_map(|(unit, shift)| value.strip_suffix(unit).map(|digits| (digits, *shift)))
        .unwrap_or((value, 0));
    let number: u64 = digits.trim().parse().ok()?;
    number.checked_mul(1_u64.checked_shl(shift)?)
}

#[test]
fn byte_counts_parse_with_binary_units_only() {
    assert_eq!(parse_bytes("4GiB"), Some(4 << 30));
    assert_eq!(parse_bytes("512MiB"), Some(512 << 20));
    assert_eq!(parse_bytes("4294967296"), Some(4_294_967_296));
    assert_eq!(parse_bytes("64 KiB"), Some(64 << 10));
    assert_eq!(
        parse_bytes("4GB"),
        None,
        "decimal units are ambiguous and refused"
    );
    assert_eq!(parse_bytes("-1"), None);
    assert_eq!(
        parse_bytes("99999999999TiB"),
        None,
        "overflow is refused, not wrapped"
    );
}
