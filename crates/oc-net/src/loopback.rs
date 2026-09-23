//! A free port on `127.0.0.1` for the engine-owned sidecar (PHASE 9 detail 3).
//!
//! The plan files this under `oc-core/src/sidecar/portpick.rs`. It lives here instead because
//! asking the kernel for a free port *is* opening a socket, and only `oc-net` opens sockets (D13.9,
//! test 9.7). The port is handed to `oc_core::sidecar` by the caller.

use std::net::{Ipv4Addr, TcpListener};

/// A port the kernel has just reported free on the loopback interface. The listener is closed
/// before the port is returned, so another process could take it in between; the caller spawns
/// the server at once and treats a server that never becomes healthy as a failed start.
pub fn free_port() -> std::io::Result<u16> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    Ok(listener.local_addr()?.port())
}
