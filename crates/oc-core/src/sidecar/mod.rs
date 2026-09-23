//! The engine's child processes: how they are started, who owns them, and how they are torn down.
//!
//! Two of them. `llama-server` (D8, IMPLEMENTATION_PLAN PHASE 9 details 2, 3 and 5) is long-lived and
//! spoken to over loopback; `tesseract` (D4, PHASE 13 detail 2) is short-lived, one per OCR region,
//! and read from its stdout. Both are registered with [`supervise`], and started through [`orphan`], so
//! neither outlives the engine — not even one killed outright.
//!
//! Nothing here opens a socket (D13.9, test 9.7). The port a server listens on and the probe that
//! asks it whether it is healthy both come from the caller, which is the thing that links `oc-net`.

pub mod endpoint;
pub mod llama;
pub mod orphan;
pub mod readiness;
pub mod server;
pub mod supervise;
pub mod tesseract;
