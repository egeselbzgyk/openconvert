//! The webview's privacy, by construction (D13.9, SECURITY §8; Phase 12 row 12.13, A12.6).
//!
//! Read straight from the shipped configuration: the capability files the window runs under and
//! the Content-Security-Policy in `tauri.conf.json`. No window is opened — what is asserted is what
//! ships, and a change that widened either would have to change this test in the same commit.

use std::collections::BTreeMap;
use std::path::PathBuf;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn config() -> serde_json::Value {
    let text = std::fs::read_to_string(crate_dir().join("tauri.conf.json")).expect("readable");
    serde_json::from_str(&text).expect("tauri.conf.json is JSON")
}

/// The CSP as directive → sources.
fn csp() -> BTreeMap<String, Vec<String>> {
    let config = config();
    let policy = config["app"]["security"]["csp"]
        .as_str()
        .expect("a CSP is set as one string");
    policy
        .split(';')
        .map(str::trim)
        .filter(|directive| !directive.is_empty())
        .map(|directive| {
            let mut parts = directive.split_whitespace().map(str::to_owned);
            let name = parts.next().expect("a directive name");
            (name, parts.collect())
        })
        .collect()
}

/// Every permission identifier the app's capability files grant.
fn granted() -> Vec<(String, String)> {
    let dir = crate_dir().join("capabilities");
    let mut grants = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("capabilities/ exists") {
        let path = entry.expect("an entry").path();
        let text = std::fs::read_to_string(&path).expect("readable");
        let capability: serde_json::Value = serde_json::from_str(&text).expect("JSON");
        let identifier = capability["identifier"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        for permission in capability["permissions"].as_array().expect("permissions") {
            // A permission is a string, or an object with an `identifier` and a scope.
            let name = permission
                .as_str()
                .or_else(|| permission["identifier"].as_str())
                .expect("a permission identifier");
            grants.push((identifier.clone(), name.to_owned()));
        }
    }
    grants
}

/// 12.13 — the capability files grant no `http:` permission and the CSP has `connect-src 'none'`.
///
/// Stated as an allow-list rather than a deny-list: the webview may listen for the app's own
/// events, and anything else it is granted fails this test until someone argues for it here.
#[test]
fn webview_has_no_network_permission() {
    const ALLOWED: [&str; 2] = ["core:event:allow-listen", "core:event:allow-unlisten"];
    let grants = granted();
    assert!(!grants.is_empty(), "the window runs under a capability");
    for (capability, permission) in &grants {
        assert!(
            !permission.starts_with("http:"),
            "{capability} grants {permission}: the webview gets no HTTP (D13.9)"
        );
        assert!(
            ALLOWED.contains(&permission.as_str()),
            "{capability} grants {permission}, which is not on the webview's allow-list"
        );
    }

    let config = config();
    assert_eq!(
        config["app"]["security"]["capabilities"],
        serde_json::json!(["default"]),
        "the window runs under exactly the capability checked here"
    );
    assert_ne!(config["app"]["withGlobalTauri"], true);

    let csp = csp();
    assert_eq!(csp.get("connect-src"), Some(&vec!["'none'".to_owned()]));
    assert_eq!(csp.get("default-src"), Some(&vec!["'self'".to_owned()]));
    // The one non-'self' source anywhere: the app's own preview protocol, which WebView2 spells
    // `http://ocpreview.localhost`. It is served by this app from the finished EPUB, not fetched.
    const PREVIEW: [&str; 2] = ["ocpreview:", "http://ocpreview.localhost"];
    assert_eq!(
        csp.get("frame-src"),
        Some(&PREVIEW.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
    );
    for (directive, sources) in &csp {
        for source in sources {
            if directive == "frame-src" && PREVIEW.contains(&source.as_str()) {
                continue;
            }
            let remote = source.contains("://")
                || ["http:", "https:", "ws:", "wss:"].contains(&source.as_str())
                || source == "*";
            assert!(
                !remote,
                "{directive} admits {source}: no remote origin anywhere"
            );
            assert_ne!(source, "'unsafe-eval'", "{directive} admits eval");
        }
    }
    // Dynamic values go through the CSSOM (`style:`), so neither scripts nor styles need inline.
    for directive in ["script-src", "style-src"] {
        let sources = csp.get(directive).expect("set explicitly");
        assert!(
            !sources.iter().any(|source| source == "'unsafe-inline'"),
            "{directive} admits inline"
        );
    }

    // And no HTTP plugin is linked at all.
    let manifest = std::fs::read_to_string(crate_dir().join("Cargo.toml")).expect("readable");
    assert!(!manifest.contains("tauri-plugin-http"));
}
