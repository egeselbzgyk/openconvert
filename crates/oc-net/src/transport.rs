//! `HttpTransport`: [`oc_ai::transport::Transport`] over HTTP (ARCHITECTURE §3.1).
//!
//! `oc-ai` builds every byte of a request and reads every byte of the reply; this sends them. The
//! key rides in an `Authorization` header — never in a URL and never on a command line — and is a
//! [`SecretString`], so it is zeroed on drop and `Debug` prints it redacted.
//!
//! No proxy is ever used, whatever the environment says: the engine-owned sidecar is on loopback,
//! and a proxy configured for downloads is not a party that may read a book's text. A host off this
//! machine is reached only with consent that names it (D10): [`HttpTransport::new`] builds a
//! transport to this machine and nowhere else, and [`HttpTransport::with_consent`] to the one host a
//! [`ConsentRecord`] names — both refuse before a socket exists ([`crate::consent::authorize`]).

use std::time::Duration;

use oc_ai::transport::{Transport, TransportError};
use secrecy::{ExposeSecret, SecretString};

use crate::consent::{authorize, ConsentRecord};
use crate::NetError;

pub struct HttpTransport {
    base: String,
    api_key: Option<SecretString>,
    agent: ureq::Agent,
}

impl std::fmt::Debug for HttpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpTransport")
            .field("base", &self.base)
            .field("api_key", &self.api_key.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

impl HttpTransport {
    /// A transport to `base` on this machine, e.g. `http://127.0.0.1:43127`. Paths are appended to
    /// it verbatim. A host that is not this machine is refused: that needs [`Self::with_consent`].
    pub fn new(base: &str, api_key: Option<SecretString>) -> Result<Self, NetError> {
        authorize(base, None)?;
        Ok(Self::build(base, api_key))
    }

    /// A transport to `base` on the host `consent` names — or on this machine, which needs none.
    pub fn with_consent(
        base: &str,
        api_key: Option<SecretString>,
        consent: &ConsentRecord,
    ) -> Result<Self, NetError> {
        authorize(base, Some(consent))?;
        Ok(Self::build(base, api_key))
    }

    fn build(base: &str, api_key: Option<SecretString>) -> Self {
        let agent = ureq::Agent::config_builder()
            .proxy(None)
            .max_redirects(0)
            .http_status_as_error(false)
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                    .unversioned_rustls_crypto_provider(std::sync::Arc::new(
                        rustls::crypto::ring::default_provider(),
                    ))
                    .build(),
            )
            .build()
            .new_agent();
        Self {
            base: base.trim_end_matches('/').to_owned(),
            api_key,
            agent,
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// GET `path`, with the key if there is one. `llama-server`'s `/health` is the caller: it
    /// answers 200 once the model is loaded and 503 while it is loading.
    pub fn get(&self, path: &str, timeout: Duration) -> Result<String, TransportError> {
        let mut request = self
            .agent
            .get(format!("{}{path}", self.base))
            .config()
            .timeout_global(Some(timeout))
            .build();
        if let Some(key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {}", key.expose_secret()));
        }
        read_reply(request.call(), timeout)
    }
}

impl Transport for HttpTransport {
    fn post_json(
        &self,
        path: &str,
        body: &str,
        timeout: Duration,
    ) -> Result<String, TransportError> {
        let mut request = self
            .agent
            .post(format!("{}{path}", self.base))
            .config()
            .timeout_global(Some(timeout))
            .build()
            .header("Content-Type", "application/json");
        if let Some(key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {}", key.expose_secret()));
        }
        read_reply(request.send(body), timeout)
    }
}

/// The body of a 2xx reply; any other status, a timeout or a dead connection as an error.
fn read_reply(
    sent: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    timeout: Duration,
) -> Result<String, TransportError> {
    let as_transport = |error: ureq::Error| match error {
        ureq::Error::Timeout(_) => TransportError::Timeout(timeout),
        other => TransportError::Unreachable(other.to_string()),
    };
    let mut response = sent.map_err(as_transport)?;
    let status = response.status();
    if !status.is_success() {
        return Err(TransportError::Status {
            status: status.as_u16(),
        });
    }
    // `read_to_string` keeps ureq's default cap on the reply's size: a chat completion held to
    // `llm.max_output_tokens_per_call` tokens is a few kilobytes, far below it.
    response.body_mut().read_to_string().map_err(as_transport)
}
