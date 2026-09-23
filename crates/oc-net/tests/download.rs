//! Rows 9.3–9.6 and acceptance criterion A9.2: the downloader fetches only what the registry pins,
//! only from hosts on the allowlist, and never leaves a file at the final path that it has not
//! verified.

mod common;

use std::panic::{catch_unwind, AssertUnwindSafe};

use common::{Answer, Quiet, Server, COMMIT, TEMPLATE};
use oc_net::download::{DownloadProgress, Downloader};
use oc_net::store::ModelStore;
use oc_net::NetError;

fn scratch() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "oc-net-{}-{}",
        std::process::id(),
        std::thread::current()
            .name()
            .unwrap_or("t")
            .replace("::", "_")
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn files_under(dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    for entry in walk(dir) {
        out.push(
            entry
                .strip_prefix(dir)
                .expect("under")
                .to_string_lossy()
                .replace('\\', "/"),
        );
    }
    out.sort();
    out
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

/// Row 9.3, A9.2.
#[test]
fn download_refuses_host_off_allowlist() {
    let server = Server::start();
    let dir = scratch();
    let downloader = Downloader::new(ModelStore::new(&dir), server.fetch(), common::config());
    let body = common::body();

    for template in [
        "https://example.com/{repo}/resolve/{revision}/{file}",
        "https://huggingface.co.example.com/{repo}/resolve/{revision}/{file}",
        "https://user@example.com/{repo}/resolve/{revision}/{file}",
        "https://huggingface.co@example.com/{repo}/resolve/{revision}/{file}",
        "https://huggingface.co:8443/{repo}/resolve/{revision}/{file}",
        "http://huggingface.co/{repo}/resolve/{revision}/{file}",
    ] {
        let error = downloader
            .pull(&common::entry(&body, template), &Quiet)
            .expect_err(template);
        assert!(
            matches!(error, NetError::HostNotAllowed { .. } | NetError::BadUrl(_)),
            "{template}: {error:?}"
        );
    }
    assert_eq!(server.requests(), 0, "zero requests before a refusal");

    // A hop off the list is refused too, and the host it names is never asked.
    server.route(
        "huggingface.co",
        &common::resolve_path(),
        Answer::Redirect("https://example.com/weights.gguf".to_owned()),
    );
    let error = downloader
        .pull(&common::entry(&body, TEMPLATE), &Quiet)
        .expect_err("an off-list redirect");
    assert_eq!(
        error,
        NetError::HostNotAllowed {
            host: "example.com".to_owned()
        }
    );
    assert_eq!(server.requests(), 1, "only the allowlisted hop was fetched");
    assert!(files_under(&dir).is_empty(), "{:?}", files_under(&dir));
}

/// Row 9.4.
#[test]
fn download_verifies_sha256_while_streaming() {
    let server = Server::start();
    let dir = scratch();
    let body = common::body();
    let mut flipped = body.clone();
    flipped[body.len() / 3] ^= 0b0000_0100;
    server.route(
        "huggingface.co",
        &common::resolve_path(),
        Answer::Redirect(format!("https://cdn-lfs.huggingface.co/blob/{COMMIT}")),
    );
    server.route(
        "cdn-lfs.huggingface.co",
        &format!("/blob/{COMMIT}"),
        Answer::Body(flipped),
    );
    let downloader = Downloader::new(ModelStore::new(&dir), server.fetch(), common::config());

    let error = downloader
        .pull(&common::entry(&body, TEMPLATE), &Quiet)
        .expect_err("a flipped bit");
    assert!(matches!(error, NetError::ShaMismatch { .. }), "{error:?}");
    assert!(
        files_under(&dir).is_empty(),
        "no .part and no final file: {:?}",
        files_under(&dir)
    );

    // A body longer than the registry says stops at the registry's size, not at the server's.
    let mut longer = body.clone();
    longer.extend_from_slice(b"more");
    server.route(
        "cdn-lfs.huggingface.co",
        &format!("/blob/{COMMIT}"),
        Answer::Body(longer),
    );
    let error = downloader
        .pull(&common::entry(&body, TEMPLATE), &Quiet)
        .expect_err("too long");
    assert!(matches!(error, NetError::TooLarge { .. }), "{error:?}");
    assert!(files_under(&dir).is_empty(), "{:?}", files_under(&dir));
}

/// Row 9.5, and the positive half of A9.1.
#[test]
fn download_writes_license_and_notice() {
    let server = Server::start();
    let dir = scratch();
    let body = common::body();
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
    let downloader = Downloader::new(ModelStore::new(&dir), server.fetch(), common::config());
    let entry = common::entry(&body, TEMPLATE);

    let path = downloader
        .pull(&entry, &Quiet)
        .expect("a verified download");
    assert_eq!(path, dir.join("tiny").join("tiny.gguf"));
    assert_eq!(std::fs::read(&path).expect("the model"), body);
    assert_eq!(
        files_under(&dir),
        ["tiny/LICENSE", "tiny/NOTICE", "tiny/tiny.gguf"]
    );
    let license = std::fs::read_to_string(dir.join("tiny/LICENSE")).expect("LICENSE");
    assert!(license.contains("Apache License"), "{license}");
    assert!(license.contains("Version 2.0, January 2004"));
    let notice = std::fs::read_to_string(dir.join("tiny/NOTICE")).expect("NOTICE");
    assert!(notice.contains("Tiny (c) nobody, Apache-2.0."), "{notice}");
    assert!(notice.contains(&common::sha256_hex(&body)), "{notice}");
    assert!(notice.contains(COMMIT), "{notice}");
}

/// Stops the transfer the way a killed process does: no cleanup code runs after it.
struct Kill;
impl DownloadProgress for Kill {
    fn bytes(&self, done: u64, _: u64) {
        if done > 0 {
            panic!("killed mid-download");
        }
    }
}

/// Row 9.6.
#[test]
fn download_is_atomic_on_interrupt() {
    let server = Server::start();
    let dir = scratch();
    let body = common::body();
    let entry = common::entry(&body, TEMPLATE);
    let downloader = Downloader::new(ModelStore::new(&dir), server.fetch(), common::config());

    // The connection drops halfway: an error, and nothing at the final path.
    server.route(
        "huggingface.co",
        &common::resolve_path(),
        Answer::Truncated(body.clone()),
    );
    let error = downloader
        .pull(&entry, &Quiet)
        .expect_err("a dropped connection");
    assert!(matches!(error, NetError::Transport(_)), "{error:?}");
    assert!(!dir.join("tiny/tiny.gguf").exists());

    // The process dies halfway: whatever is left is the .part, never a truncated GGUF.
    server.route(
        "huggingface.co",
        &common::resolve_path(),
        Answer::Body(body.clone()),
    );
    let killed = catch_unwind(AssertUnwindSafe(|| downloader.pull(&entry, &Kill)));
    assert!(killed.is_err(), "the download was killed");
    assert!(!dir.join("tiny/tiny.gguf").exists(), "no final file");
    assert_eq!(files_under(&dir), ["tiny/tiny.gguf.part"]);

    // And the next pull starts over and lands the verified file.
    let path = downloader.pull(&entry, &Quiet).expect("a clean retry");
    assert_eq!(std::fs::read(path).expect("the model"), body);
    assert!(!dir.join("tiny/tiny.gguf.part").exists());
}
