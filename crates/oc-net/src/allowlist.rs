//! The hosts a download may touch (D13.9, SECURITY §8).
//!
//! Checked on the URL the registry resolves to and again on every redirect, before the fetch that
//! would open a socket to it. The parse is deliberately narrower than RFC 3986: an `https` scheme, a
//! bare host, no user-info and no port. Anything cleverer than that is refused rather than
//! interpreted, because a URL parser that is generous about `user@host` or `host:port` is exactly
//! where "which host is this, really?" goes wrong.

use crate::NetError;

/// Every host the model manager may connect to, including the CDN a download redirects to.
pub const HOST_ALLOWLIST: &[&str] = &[
    "huggingface.co",
    "cdn-lfs.huggingface.co",
    "cdn-lfs-us-1.huggingface.co",
];

const HTTPS: &str = "https://";

/// The host of `url`, if `url` is an `https://` URL on an allowlisted host.
pub fn check(url: &str) -> Result<String, NetError> {
    let host = host_of(url)?;
    if HOST_ALLOWLIST.contains(&host.as_str()) {
        Ok(host)
    } else {
        Err(NetError::HostNotAllowed { host })
    }
}

/// The host of an `https://` URL, lowercased. User-info and ports are refused outright.
pub fn host_of(url: &str) -> Result<String, NetError> {
    let bad = || NetError::BadUrl(url.to_owned());
    let scheme_len = HTTPS.len();
    if url.len() < scheme_len || !url[..scheme_len].eq_ignore_ascii_case(HTTPS) {
        return Err(bad());
    }
    let rest = &url[scheme_len..];
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || authority.contains(['@', ':', '\\', '%']) {
        return Err(bad());
    }
    Ok(authority.to_ascii_lowercase())
}

/// Where a redirect's `Location` points, made absolute against the URL that sent it.
pub fn resolve_location(from: &str, location: &str) -> Result<String, NetError> {
    if location.starts_with('/') && !location.starts_with("//") {
        let host = host_of(from)?;
        Ok(format!("{HTTPS}{host}{location}"))
    } else {
        Ok(location.to_owned())
    }
}
