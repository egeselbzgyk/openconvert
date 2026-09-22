//! The seam where a request would leave the process (D13.9, ARCHITECTURE §3.1).
//!
//! `oc-ai` builds every byte of a request and reads every byte of a reply, and never sends or
//! receives one: that is a [`Transport`], implemented by `oc-net` for a real endpoint and by an
//! in-process double for the tests. This trait is why `oc-ai` needs no network crate, which is
//! what makes "only `oc-net` opens sockets" a property the build checks (test 8.15) rather than
//! something a reviewer remembers.

use std::time::Duration;

/// Carries one request to an OpenAI-compatible endpoint and brings back its reply.
pub trait Transport: Send + Sync {
    /// POST `body`, a JSON document, to `path` on the endpoint, and return the reply's body.
    fn post_json(
        &self,
        path: &str,
        body: &str,
        timeout: Duration,
    ) -> Result<String, TransportError>;
}

/// Why no reply came back. None of these is an answer, and none is a gate failure: the question
/// was never answered at all.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    #[error("the endpoint did not reply within {0:?}")]
    Timeout(Duration),
    #[error("the endpoint could not be reached: {0}")]
    Unreachable(String),
    #[error("the endpoint replied with HTTP status {status}")]
    Status { status: u16 },
}
