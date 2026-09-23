//! Whether a book's text may go to an endpoint (D10, SECURITY §8, PHASE 11 detail 4).
//!
//! A loopback endpoint — Ollama on `localhost:11434`, the sidecar on `127.0.0.1` — never leaves the
//! machine, and needs nothing. Any other host needs **consent that names it**: the user's
//! statement that this host, and no other, may read the text of the books they convert. The check
//! is made here, in the one crate that opens sockets, so that no provider can forget it:
//! [`crate::transport::HttpTransport`] cannot be built to a host [`authorize`] refuses, and the
//! refusal happens before any socket exists.
//!
//! **The URL is parsed narrowly, on purpose.** Anything a URL parser could read two ways — user-info
//! (`http://127.0.0.1@evil.example` is a request to `evil.example`), percent-escapes, a backslash,
//! a query, whitespace — is refused rather than interpreted, because "which host is this, really?"
//! is exactly the question a generous parser gets wrong. An unparseable URL is never loopback.

use std::net::{Ipv4Addr, Ipv6Addr};

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::NetError;

/// How long a consent lasts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsentScope {
    /// This run and no other: given on its command line (`--llm-allow-host`) or in its job spec
    /// (`ai.non_loopback_consent`). The engine remembers nothing; the next run is asked again. The
    /// desktop app keeps the user's choice per configuration and writes it into each job it runs.
    Run,
}

impl ConsentScope {
    /// The name the report prints.
    pub fn as_str(self) -> &'static str {
        match self {
            ConsentScope::Run => "run",
        }
    }
}

/// The user's consent to sending a book's text to one host (D10). The conversion report prints it,
/// so a reader of the report can see that text left the machine, where it went, and when.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsentRecord {
    /// Lower-cased, as [`host_of`] returns it.
    pub host: String,
    pub granted_at: OffsetDateTime,
    pub scope: ConsentScope,
}

impl ConsentRecord {
    /// Consent to `host`, granted now.
    pub fn grant(host: &str, scope: ConsentScope) -> Self {
        let now = OffsetDateTime::now_utc();
        Self {
            host: host.to_ascii_lowercase(),
            // To the second: the report is read by a person, and a nanosecond says nothing more.
            granted_at: now.replace_nanosecond(0).unwrap_or(now),
            scope,
        }
    }

    /// `granted_at` in RFC 3339, UTC.
    pub fn granted_at_rfc3339(&self) -> String {
        self.granted_at
            .to_offset(time::UtcOffset::UTC)
            .format(&Rfc3339)
            .unwrap_or_default()
    }
}

/// Whether sending to `url` needs consent: true unless its host is this machine. A URL that does
/// not parse is never this machine.
pub fn requires_consent(url: &str) -> bool {
    host_of(url).map_or(true, |host| !is_loopback(&host))
}

/// Whether `url` may be sent a book's text, given the consent there is.
///
/// Loopback: always. Any other host: only with consent that names it, and only over `https` —
/// consent to a host reading the text is not consent to everyone on the way reading it.
pub fn authorize(url: &str, consent: Option<&ConsentRecord>) -> Result<(), NetError> {
    let parsed = parse(url)?;
    if is_loopback(&parsed.host) {
        return Ok(());
    }
    if consent.is_none_or(|consent| consent.host != parsed.host) {
        return Err(NetError::ConsentRequired { host: parsed.host });
    }
    if !parsed.https {
        return Err(NetError::PlaintextRemote { host: parsed.host });
    }
    Ok(())
}

/// The host of an `http://` or `https://` URL, lower-cased, without brackets or port.
pub fn host_of(url: &str) -> Result<String, NetError> {
    parse(url).map(|parsed| parsed.host)
}

/// Whether `host` — as [`host_of`] returns it — is this machine: `localhost`, 127/8, `::1`, or
/// `::1`'s IPv4-mapped spelling of 127/8.
pub fn is_loopback(host: &str) -> bool {
    if host == "localhost" {
        return true;
    }
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return ip.is_loopback();
    }
    host.parse::<Ipv6Addr>().is_ok_and(|ip| {
        ip.is_loopback()
            || ip
                .to_ipv4_mapped()
                .is_some_and(|mapped| mapped.is_loopback())
    })
}

struct Parsed {
    https: bool,
    host: String,
}

fn parse(url: &str) -> Result<Parsed, NetError> {
    let bad = || NetError::BadUrl(url.to_owned());
    if url
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '\\' | '%' | '@' | '?' | '#'))
    {
        return Err(bad());
    }
    let (https, rest) = if let Some(rest) = strip_prefix_ignore_case(url, "https://") {
        (true, rest)
    } else if let Some(rest) = strip_prefix_ignore_case(url, "http://") {
        (false, rest)
    } else {
        return Err(bad());
    };
    let authority = rest.split('/').next().unwrap_or_default();

    let (host, port) = match authority.strip_prefix('[') {
        Some(bracketed) => {
            let (host, after) = bracketed.split_once(']').ok_or_else(bad)?;
            if host.parse::<Ipv6Addr>().is_err() {
                return Err(bad());
            }
            let port = match after {
                "" => None,
                _ => Some(after.strip_prefix(':').ok_or_else(bad)?),
            };
            (host, port)
        }
        None => match authority.split_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        },
    };
    if let Some(port) = port {
        if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(bad());
        }
    }
    if host.is_empty() || host.contains(['[', ']']) {
        return Err(bad());
    }
    Ok(Parsed {
        https,
        host: host.to_ascii_lowercase(),
    })
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}
