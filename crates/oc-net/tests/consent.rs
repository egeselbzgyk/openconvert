//! PHASE 11: whether a book's text may go to an endpoint (D10, SECURITY §8).
//!
//! A loopback endpoint never leaves the machine and needs nothing. Anything else needs consent
//! that names the host — and the check is here, in the one crate that opens sockets, so no
//! provider can forget it: `HttpTransport` cannot be built to a host the user did not name.

use oc_net::consent::{authorize, host_of, requires_consent, ConsentRecord, ConsentScope};
use oc_net::transport::HttpTransport;
use oc_net::NetError;

fn consent(host: &str) -> ConsentRecord {
    ConsentRecord::grant(host, ConsentScope::Run)
}

/// Row 11.6. `127.0.0.1`, `localhost` and `::1` — and the rest of 127/8, and `::1` spelled as an
/// IPv4-mapped address — pass without a toggle, in every URL shape a user writes.
#[test]
fn loopback_never_requires_consent() {
    for url in [
        "http://127.0.0.1:8080",
        "http://127.0.0.1:8080/v1",
        "https://127.0.0.1/v1/",
        "http://localhost:11434",
        "HTTP://LOCALHOST:11434/",
        "http://[::1]:9000",
        "http://[::1]/v1",
        "http://127.12.0.3:1234",
        "http://[::ffff:127.0.0.1]:8080",
    ] {
        assert!(!requires_consent(url), "{url} is this machine");
        assert_eq!(authorize(url, None), Ok(()), "{url}");
        assert!(
            HttpTransport::new(url, None).is_ok(),
            "{url}: a transport needs no consent to reach this machine"
        );
    }
}

/// Every other host needs consent, and consent for one host is not consent for another.
#[test]
fn a_host_off_this_machine_needs_consent_that_names_it() {
    for (url, host) in [
        ("https://example.com/v1", "example.com"),
        ("https://api.example.com:8443/v1", "api.example.com"),
        ("https://10.0.0.2:8080", "10.0.0.2"),
        ("https://127.0.0.1.example.com/v1", "127.0.0.1.example.com"),
        ("https://localhost.example.com", "localhost.example.com"),
        ("https://[2001:db8::1]:8080", "2001:db8::1"),
        ("http://0.0.0.0:8080", "0.0.0.0"),
    ] {
        assert!(requires_consent(url), "{url}");
        assert_eq!(host_of(url).as_deref(), Ok(host), "{url}");
        assert_eq!(
            authorize(url, None),
            Err(NetError::ConsentRequired {
                host: host.to_owned()
            }),
            "{url}"
        );
        assert_eq!(
            authorize(url, Some(&consent("elsewhere.example.org"))),
            Err(NetError::ConsentRequired {
                host: host.to_owned()
            }),
            "{url}: consent for another host"
        );
        assert!(
            matches!(
                HttpTransport::new(url, None),
                Err(NetError::ConsentRequired { .. })
            ),
            "{url}: no transport without consent"
        );
    }

    let granted = consent("EXAMPLE.com");
    assert_eq!(granted.host, "example.com", "hosts compare lower-cased");
    assert_eq!(authorize("https://example.com/v1", Some(&granted)), Ok(()));
    assert!(HttpTransport::with_consent("https://example.com/v1", None, &granted).is_ok());
}

/// A book's text is not sent across a network in the clear: consent to a host is consent to that
/// host reading it, not to everyone on the way (PROVISIONAL, DECISIONS_LOG 2026-09-23).
#[test]
fn plain_http_off_this_machine_is_refused_even_with_consent() {
    let granted = consent("10.0.0.2");
    assert_eq!(
        authorize("http://10.0.0.2:11434", Some(&granted)),
        Err(NetError::PlaintextRemote {
            host: "10.0.0.2".to_owned()
        })
    );
    assert!(authorize("https://10.0.0.2:11434", Some(&granted)).is_ok());
}

/// Anything a URL parser could read two ways is refused rather than interpreted: user-info (the
/// classic `http://127.0.0.1@evil.example`), stray ports, escapes, queries and whitespace.
#[test]
fn an_ambiguous_url_is_refused_and_needs_consent() {
    for url in [
        "http://127.0.0.1@evil.example/v1",
        "http://user:pass@localhost:8080",
        "http://localhost:80:80",
        "http://local%68ost:8080",
        "http://localhost\\@evil.example",
        "http://localhost:8080/v1?x=1",
        "http://localhost:8080/#frag",
        "http://localhost:80a",
        "http://[::1:8080",
        "http:// localhost",
        "ftp://127.0.0.1",
        "127.0.0.1:8080",
        "http://",
    ] {
        assert!(host_of(url).is_err(), "{url} parsed");
        assert!(
            requires_consent(url),
            "{url}: unparseable is never loopback"
        );
        assert!(
            matches!(authorize(url, None), Err(NetError::BadUrl(_))),
            "{url}"
        );
    }
}

/// The record says when, and for how long: this run and no other.
#[test]
fn a_consent_record_carries_its_time_in_rfc_3339() {
    let granted = consent("example.com");
    let stamp = granted.granted_at_rfc3339();
    assert!(stamp.ends_with('Z'), "{stamp}");
    assert_eq!(stamp.len(), "2026-09-23T10:00:00Z".len(), "{stamp}");
    assert_eq!(granted.scope.as_str(), "run");
}
