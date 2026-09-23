//! Landlock: the engine restricts its own filesystem — and, on ABI ≥ 4, its TCP — to the job's
//! [`ScopeSet`] (PHASE 14 detail 7, D13.9, SECURITY §11).
//!
//! Linux ≥ 5.13 only, self-applied, **never fatal**: on a kernel without Landlock the outcome is
//! [`LandlockOutcome::Unsupported`] and the conversion proceeds; the report records why. Applied
//! after argument parsing and job-spec validation and **before the first PDF byte is read**, and
//! before the engine starts any thread of its own: below ABI 8 a restriction binds the calling
//! thread and the threads it starts afterwards, not threads that already exist. Never applied in
//! the desktop app's process.
//!
//! Every filesystem right Landlock knows at ABI 5 is *handled* — denied unless a rule grants it —
//! and the rules are the scope set: read (and execute) beneath `read`, everything beneath
//! `read_write`. TCP `bind` and `connect` are handled on ABI ≥ 4, with `connect` granted only to
//! `connect_ports` (an `--ai` endpoint's), which makes "the conversion path opens no socket" a
//! property the kernel enforces rather than one the dependency graph promises (D13.9).
//!
//! The syscalls belong to the `landlock` crate; nothing here is `unsafe`.

use super::ScopeSet;

/// The name the report uses.
pub const MECHANISM: &str = "landlock";

/// What self-restriction achieved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LandlockOutcome {
    /// In force. `abi` is the kernel's (as the `landlock` crate understands it); `net_restricted`
    /// is whether TCP is handled too (ABI ≥ 4).
    Applied { abi: i32, net_restricted: bool },
    /// Not in force, and why. The conversion proceeds regardless.
    Unsupported { reason: String },
}

/// Restrict this process (the calling thread and every thread and child it starts from now on) to
/// `scope`. Paths in the scope that do not exist are skipped rather than refused.
#[cfg(target_os = "linux")]
pub fn landlock_self_restrict(scope: &ScopeSet) -> LandlockOutcome {
    match restrict(scope) {
        Ok(outcome) => outcome,
        Err(error) => LandlockOutcome::Unsupported {
            reason: error.to_string(),
        },
    }
}

#[cfg(not(target_os = "linux"))]
pub fn landlock_self_restrict(_scope: &ScopeSet) -> LandlockOutcome {
    LandlockOutcome::Unsupported {
        reason: "Landlock is a Linux kernel feature".to_owned(),
    }
}

#[cfg(target_os = "linux")]
fn restrict(scope: &ScopeSet) -> Result<LandlockOutcome, landlock::RulesetError> {
    use landlock::{
        path_beneath_rules, Access, AccessFs, AccessNet, LandlockStatus, NetPort, Ruleset,
        RulesetAttr, RulesetCreatedAttr, RulesetStatus, ABI,
    };

    /// The filesystem rights handled: everything ABI 5 defines (`IoctlDev` being the newest).
    const FS_ABI: ABI = ABI::V5;
    /// TCP bind and connect arrived with ABI 4.
    const NET_ABI: ABI = ABI::V4;

    let mut created = Ruleset::default()
        .handle_access(AccessFs::from_all(FS_ABI))?
        .handle_access(AccessNet::from_all(NET_ABI))?
        .create()?
        .add_rules(path_beneath_rules(&scope.read, AccessFs::from_read(FS_ABI)))?
        .add_rules(path_beneath_rules(
            &scope.read_write,
            AccessFs::from_all(FS_ABI),
        ))?;
    for port in &scope.connect_ports {
        created = created.add_rule(NetPort::new(*port, AccessNet::ConnectTcp))?;
    }
    let status = created.restrict_self()?;

    let effective = match status.landlock {
        LandlockStatus::Available { effective_abi, .. } => Some(effective_abi),
        LandlockStatus::NotEnabled | LandlockStatus::NotImplemented => None,
    };
    Ok(match (status.ruleset, effective) {
        (RulesetStatus::NotEnforced, _) | (_, None) => LandlockOutcome::Unsupported {
            reason: match status.landlock {
                LandlockStatus::NotEnabled => "Landlock is built into this kernel but not enabled",
                LandlockStatus::NotImplemented => "this kernel has no Landlock (Linux < 5.13)",
                LandlockStatus::Available { .. } => "the kernel enforced none of the rules",
            }
            .to_owned(),
        },
        (_, Some(abi)) => LandlockOutcome::Applied {
            abi: abi as i32,
            net_restricted: abi >= NET_ABI,
        },
    })
}
