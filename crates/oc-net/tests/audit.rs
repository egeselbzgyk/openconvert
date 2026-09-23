//! PHASE 14 row 14.21: the network audit log records the connections `oc-net` opens — one line for
//! a download — and nothing for what opens none. The conversion half (a conversion appends zero
//! lines) is `openconvert`'s `a_conversion_appends_nothing_to_the_network_audit_log`, because only
//! that crate can convert.

mod common;

use common::{Answer, Quiet, Server, COMMIT, TEMPLATE};
use oc_net::audit::{install, installed, AuditLog};
use oc_net::download::Downloader;
use oc_net::store::ModelStore;
use oc_net::NetError;

fn lines() -> Vec<serde_json::Value> {
    let log = installed().expect("installed");
    std::fs::read_to_string(log.path())
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("a JSON line"))
        .collect()
}

/// Row 14.21, the download half. One pull is one line, with the host, the purpose, the size and the
/// outcome; a pull the allowlist refuses opens no socket and writes no line; so does reading the
/// store. A redirected pull is one line per connection, which is what "every outbound connection"
/// means.
#[test]
fn net_audit_log_records_downloads_and_nothing_else() {
    let dir = std::env::temp_dir().join(format!("oc-audit-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    install(AuditLog::in_data_dir(&dir.join("data"), 1 << 20));
    assert!(lines().is_empty(), "a fresh log is empty");

    let server = Server::start();
    let body = common::body();
    server.route(
        "huggingface.co",
        &common::resolve_path(),
        Answer::Body(body.clone()),
    );
    let store = ModelStore::new(dir.join("models"));
    let downloader = Downloader::new(store, server.fetch(), common::config());
    downloader
        .pull(&common::entry(&body, TEMPLATE), &Quiet)
        .expect("a verified download");

    let recorded = lines();
    assert_eq!(recorded.len(), 1, "one download, one line: {recorded:?}");
    let line = &recorded[0];
    assert_eq!(line["purpose"], "download");
    assert_eq!(line["outcome"], "ok");
    assert_eq!(line["bytes"], body.len() as u64);
    assert!(
        line["ts"].as_str().is_some_and(|ts| ts.ends_with('Z')),
        "{line}"
    );
    assert!(
        line["host"].as_str().is_some_and(|host| !host.is_empty()),
        "{line}"
    );

    // Refused before any socket: nothing to record.
    let off_list = common::entry(&body, "https://example.org/{repo}/{revision}/{file}");
    let requests = server.requests();
    assert!(matches!(
        downloader.pull(&off_list, &Quiet),
        Err(NetError::HostNotAllowed { .. })
    ));
    assert_eq!(server.requests(), requests);
    let _ = downloader.store().list();
    assert_eq!(lines().len(), 1, "no connection, no line");

    // Redirected: two connections, two lines.
    server.route(
        "huggingface.co",
        &common::resolve_path(),
        Answer::Redirect(format!("https://cdn-lfs-us-1.huggingface.co/blob/{COMMIT}")),
    );
    server.route(
        "cdn-lfs-us-1.huggingface.co",
        &format!("/blob/{COMMIT}"),
        Answer::Body(body.clone()),
    );
    let _ = std::fs::remove_dir_all(dir.join("models"));
    downloader
        .pull(&common::entry(&body, TEMPLATE), &Quiet)
        .expect("a redirected download");
    let recorded = lines();
    assert_eq!(recorded.len(), 3, "{recorded:?}");
    assert_eq!(recorded[1]["outcome"], "HTTP 302");
    assert_eq!(recorded[2]["outcome"], "ok");
    let _ = std::fs::remove_dir_all(&dir);
}
