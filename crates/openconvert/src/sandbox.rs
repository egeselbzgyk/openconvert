//! What the engine did to restrict itself, and what the report says about it (PHASE 14 details 5
//! and 7).
//!
//! The mechanisms are `oc_core::sandbox`'s; this is the order the engine applies them in and the
//! record it keeps. **None of it ever fails a conversion**: a mechanism that could not be applied
//! is a line in the report (`status: "unsupported"` and why), never an error for the user.

use std::path::{Path, PathBuf};

use serde::Serialize;

use oc_core::limits::Limits;
use oc_core::sandbox::landlock::{landlock_self_restrict, LandlockOutcome};
use oc_core::sandbox::{rlimit, ScopeSet};

/// The report's `sandbox` section.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SandboxReport {
    pub memory: MemoryReport,
    /// Absent until the restriction has been attempted (a run that failed before it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub landlock: Option<LandlockReport>,
}

impl SandboxReport {
    /// Apply the memory cap and start the record. Call before the PDF is opened.
    pub fn start(limits: &Limits) -> Self {
        Self {
            memory: apply_memory_cap(limits),
            landlock: None,
        }
    }
}

/// Landlock, as applied or as skipped (PHASE 14 detail 7: "the outcome is recorded in the report").
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct LandlockReport {
    /// `applied` or `unsupported`.
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abi: Option<i32>,
    /// Whether TCP `connect` was restricted too (ABI ≥ 4).
    pub net_restricted: bool,
    /// Why it was skipped: `sandbox: landlock unsupported (<reason>)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Setting this environment variable to `off` skips Landlock and records why. An operator's switch
/// for diagnosing a scope that is too narrow — and how the recorded skip is exercised on a kernel
/// that has Landlock (row 14.11).
pub const LANDLOCK_VAR: &str = "OC_LANDLOCK";
const LANDLOCK_OFF: &str = "off";

/// What a job touches, from which [`scope_for`] builds its [`ScopeSet`].
#[derive(Clone, Debug, Default)]
pub struct ScopeInputs {
    pub input: PathBuf,
    pub output: PathBuf,
    pub report: PathBuf,
    /// Files the run reads after the restriction (the overrides file).
    pub extra_reads: Vec<PathBuf>,
    /// Directories the run writes after it (the LLM answer cache, the rebuild cache).
    pub extra_writes: Vec<PathBuf>,
    /// Programs the run will start after it (`tesseract`), which need themselves, their
    /// libraries and their data readable and executable — and this binary, the exec trampoline.
    pub programs: Vec<PathBuf>,
    /// The LLM endpoint's port, when `--ai` has one.
    pub connect_ports: Vec<u16>,
}

/// Where a Linux system keeps what a program it starts needs: shared libraries, the loader's
/// cache, and the interpreters a wrapper script names on its `#!` line.
const SYSTEM_READ: [&str; 8] = [
    "/bin",
    "/usr/bin",
    "/usr/lib",
    "/usr/lib64",
    "/lib",
    "/lib64",
    "/usr/local/lib",
    "/etc/ld.so.cache",
];
/// Where PDFium's font mapper looks for system fonts on Linux, when a PDF names a font it does not
/// embed. Readable so a restricted run maps fonts exactly as an unrestricted one does: the output
/// must not depend on whether the kernel has Landlock (D13.8).
const FONT_DIRS: [&str; 4] = [
    "/usr/share/fonts",
    "/usr/share/X11/fonts/Type1",
    "/usr/share/X11/fonts/TTF",
    "/usr/local/share/fonts",
];
/// Opened by every child's `Stdio::null()` in this process.
const DEV_NULL: &str = "/dev/null";

/// The scope set for one job (PHASE 14 detail 7): read the input; read and write where the output,
/// the report and the caches go; read and execute what a child needs.
pub fn scope_for(inputs: &ScopeInputs) -> ScopeSet {
    let parent = |path: &Path| {
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    };
    let mut read: Vec<PathBuf> = vec![inputs.input.clone()];
    read.extend(inputs.extra_reads.iter().cloned());
    read.extend(FONT_DIRS.iter().map(PathBuf::from));
    if !inputs.programs.is_empty() {
        read.extend(SYSTEM_READ.iter().map(PathBuf::from));
        if let Some(trampoline) = oc_core::sidecar::orphan::trampoline() {
            read.push(trampoline);
        }
        for program in &inputs.programs {
            read.push(program.clone());
            // `<prefix>/bin/tesseract` keeps its libraries and data under `<prefix>`.
            if let Some(prefix) = program.parent().and_then(Path::parent) {
                read.push(prefix.to_path_buf());
            }
        }
    }
    let mut read_write = vec![
        parent(&inputs.output),
        parent(&inputs.report),
        PathBuf::from(DEV_NULL),
    ];
    read_write.extend(inputs.extra_writes.iter().cloned());
    read.sort();
    read.dedup();
    read_write.sort();
    read_write.dedup();
    ScopeSet {
        read,
        read_write,
        connect_ports: inputs.connect_ports.clone(),
    }
}

/// Restrict this process to `scope` with Landlock, or record why not. Never fails.
pub fn restrict(scope: &ScopeSet) -> LandlockReport {
    if std::env::var(LANDLOCK_VAR).is_ok_and(|value| value == LANDLOCK_OFF) {
        return LandlockReport {
            status: "unsupported",
            abi: None,
            net_restricted: false,
            reason: Some(format!("disabled by {LANDLOCK_VAR}={LANDLOCK_OFF}")),
        };
    }
    match landlock_self_restrict(scope) {
        LandlockOutcome::Applied {
            abi,
            net_restricted,
        } => LandlockReport {
            status: "applied",
            abi: Some(abi),
            net_restricted,
            reason: None,
        },
        LandlockOutcome::Unsupported { reason } => LandlockReport {
            status: "unsupported",
            abi: None,
            net_restricted: false,
            reason: Some(reason),
        },
    }
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

#[test]
fn a_scope_reads_the_input_and_writes_beside_the_output() {
    let scope = scope_for(&ScopeInputs {
        input: PathBuf::from("/in/book.pdf"),
        output: PathBuf::from("/out/book.epub"),
        report: PathBuf::from("/reports/book.json"),
        ..ScopeInputs::default()
    });
    assert!(scope.read.contains(&PathBuf::from("/in/book.pdf")));
    assert!(
        !scope.read.contains(&PathBuf::from("/in")),
        "the input, not its directory"
    );
    assert!(scope.read_write.contains(&PathBuf::from("/out")));
    assert!(scope.read_write.contains(&PathBuf::from("/reports")));
    assert!(
        !scope.read.contains(&PathBuf::from("/usr/lib")),
        "no child, no libraries"
    );
    assert!(scope.connect_ports.is_empty(), "no --ai, no socket");

    let with_ocr = scope_for(&ScopeInputs {
        input: PathBuf::from("/in/book.pdf"),
        output: PathBuf::from("book.epub"),
        report: PathBuf::from("book.epub.report.json"),
        programs: vec![PathBuf::from("/opt/tess/bin/tesseract")],
        ..ScopeInputs::default()
    });
    assert!(with_ocr.read.contains(&PathBuf::from("/opt/tess")));
    assert!(with_ocr.read.contains(&PathBuf::from("/usr/lib")));
    assert!(
        with_ocr.read_write.contains(&PathBuf::from(".")),
        "a bare file name is the cwd"
    );
}
