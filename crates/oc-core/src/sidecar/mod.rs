//! The `llama-server` sidecar: how it is started, who owns it, and how it is torn down (D8,
//! IMPLEMENTATION_PLAN PHASE 9 details 2, 3 and 5).
//!
//! Nothing here opens a socket (D13.9, test 9.7). The port a server listens on and the probe that
//! asks it whether it is healthy both come from the caller, which is the thing that links `oc-net`.

pub mod llama;
