//! PHASE 15 rows 15.9 and 15.10 (A15.4): an update is accepted when its `latest.json` entry is
//! signed with the release key, and refused — before anything is written — when a single byte of
//! the payload differs from what was signed.
//!
//! The keypair is made here, per run, the way `tauri signer generate` makes one, and signs the way
//! `tauri signer sign` does (the `minisign` crate, base64-wrapped). No private key exists anywhere
//! but in this process's memory.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Mutex;

use base64::Engine as _;
use oc_net::download::{Fetch, Fetched};
use oc_net::update::{
    fetch_update, install_appimage, platform_keys, UpdateConfig, UpdateError, UPDATE_HOST_ALLOWLIST,
};
use oc_net::NetError;

mod common;

const ENDPOINT: &str =
    "https://github.com/egeselbzgyk/openconvert/releases/latest/download/latest.json";
const ASSET: &str = "https://github.com/egeselbzgyk/openconvert/releases/download/v9.9.9/\
                     OpenConvert_9.9.9_amd64.AppImage";
const CDN: &str = "https://release-assets.githubusercontent.com/github-production-release-asset/1/\
                   OpenConvert_9.9.9_amd64.AppImage";

/// The numbers a caller reads from `thresholds.toml`, small enough to exercise.
const CONFIG: UpdateConfig = UpdateConfig {
    max_redirects: 5,
    manifest_max_bytes: 64 * 1024,
    payload_max_bytes: 1024 * 1024,
};

/// Serves fixed answers and records what was asked for.
struct FakeFetch {
    answers: BTreeMap<String, Answer>,
    asked: Mutex<Vec<String>>,
}

#[derive(Clone)]
enum Answer {
    Body(Vec<u8>),
    Redirect(String),
}

impl FakeFetch {
    fn new(answers: &[(&str, Answer)]) -> Self {
        Self {
            answers: answers
                .iter()
                .map(|(url, answer)| ((*url).to_owned(), answer.clone()))
                .collect(),
            asked: Mutex::new(Vec::new()),
        }
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().expect("lock").clone()
    }
}

impl Fetch for FakeFetch {
    fn get(&self, url: &str) -> Result<Fetched, NetError> {
        self.asked.lock().expect("lock").push(url.to_owned());
        match self.answers.get(url) {
            Some(Answer::Body(bytes)) => Ok(Fetched::Body(Box::new(Cursor::new(bytes.clone())))),
            Some(Answer::Redirect(to)) => Ok(Fetched::Redirect(to.clone())),
            None => Err(NetError::Status(404)),
        }
    }
}

/// A release key, as `tauri signer generate` makes one: the public half base64-wrapped the way
/// `tauri.conf.json` carries it.
struct ReleaseKey {
    public: minisign::PublicKey,
    secret: minisign::SecretKey,
}

impl ReleaseKey {
    fn generate() -> Self {
        let pair = minisign::KeyPair::generate_unencrypted_keypair().expect("a keypair");
        Self {
            public: pair.pk,
            secret: pair.sk,
        }
    }

    fn pubkey_b64(&self) -> String {
        let text = self.public.to_box().expect("a key box").to_string();
        base64::engine::general_purpose::STANDARD.encode(text)
    }

    /// A `.sig` file's content, as `tauri signer sign` writes it.
    fn sign_b64(&self, payload: &[u8]) -> String {
        let signature = minisign::sign(
            Some(&self.public),
            &self.secret,
            Cursor::new(payload),
            Some("timestamp:1\tfile:OpenConvert_9.9.9_amd64.AppImage"),
            Some("signature from tauri secret key"),
        )
        .expect("signed")
        .to_string();
        base64::engine::general_purpose::STANDARD.encode(signature)
    }
}

fn latest_json(version: &str, signature: &str) -> Vec<u8> {
    let mut platforms = serde_json::Map::new();
    for key in platform_keys(None) {
        platforms.insert(
            key,
            serde_json::json!({ "signature": signature, "url": ASSET }),
        );
    }
    serde_json::to_vec(&serde_json::json!({
        "version": version,
        "notes": "A test release.",
        "pub_date": "2026-09-23T00:00:00Z",
        "platforms": platforms,
    }))
    .expect("json")
}

fn payload() -> Vec<u8> {
    (0..4096u32).map(|i| (i % 251) as u8).collect()
}

/// A GitHub release as the updater sees it: the manifest, the asset URL redirecting to the CDN,
/// the CDN serving `served`.
fn release(manifest: Vec<u8>, served: Vec<u8>) -> FakeFetch {
    FakeFetch::new(&[
        (ENDPOINT, Answer::Body(manifest)),
        (ASSET, Answer::Redirect(CDN.to_owned())),
        (CDN, Answer::Body(served)),
    ])
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("oc-updater-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    dir
}

/// Row 15.9: `latest.json` signed with the release key is accepted, the payload is exactly the
/// bytes that were signed, and installing it replaces the AppImage.
#[test]
fn updater_manifest_signature_verifies() {
    let key = ReleaseKey::generate();
    let signed = payload();
    let fetch = release(latest_json("9.9.9", &key.sign_b64(&signed)), signed.clone());

    let update = fetch_update(
        &fetch,
        ENDPOINT,
        "1.0.0",
        &platform_keys(Some("appimage")),
        &key.pubkey_b64(),
        &CONFIG,
    )
    .expect("the update verifies")
    .expect("9.9.9 is newer than 1.0.0");
    assert_eq!(update.version(), "9.9.9");
    assert_eq!(update.file_name(), "OpenConvert_9.9.9_amd64.AppImage");
    assert_eq!(update.bytes(), signed.as_slice());
    assert_eq!(fetch.asked(), [ENDPOINT, ASSET, CDN]);
    for url in fetch.asked() {
        let host = oc_net::allowlist::host_of(&url).expect("a host");
        assert!(UPDATE_HOST_ALLOWLIST.contains(&host.as_str()), "{host}");
    }

    let dir = scratch("install");
    let appimage = dir.join("OpenConvert.AppImage");
    std::fs::write(&appimage, b"the old version").expect("write");
    install_appimage(&update, &appimage).expect("installed");
    assert_eq!(std::fs::read(&appimage).expect("read"), signed);
    assert_eq!(
        std::fs::read_dir(&dir).expect("dir").count(),
        1,
        "nothing staged is left behind"
    );
    let _ = std::fs::remove_dir_all(dir);

    // Up to date: the manifest is read, the payload is not.
    let fetch = release(latest_json("9.9.9", &key.sign_b64(&signed)), signed);
    let none = fetch_update(
        &fetch,
        ENDPOINT,
        "9.9.9",
        &platform_keys(None),
        &key.pubkey_b64(),
        &CONFIG,
    )
    .expect("a check");
    assert!(none.is_none());
    assert_eq!(fetch.asked(), [ENDPOINT]);
}

/// Row 15.10: one flipped byte in the downloaded payload, a signature by another key, or a
/// signature over other bytes — each refused before anything is written, so there is nothing to
/// install. And a build still carrying the placeholder key asks nothing at all.
#[test]
fn updater_rejects_tampered_payload() {
    let key = ReleaseKey::generate();
    let signed = payload();
    let mut tampered = signed.clone();
    tampered[1234] ^= 0x01;
    let check = |fetch: &FakeFetch, pubkey: &str| {
        fetch_update(
            fetch,
            ENDPOINT,
            "1.0.0",
            &platform_keys(None),
            pubkey,
            &CONFIG,
        )
    };

    let fetch = release(latest_json("9.9.9", &key.sign_b64(&signed)), tampered);
    assert!(matches!(
        check(&fetch, &key.pubkey_b64()),
        Err(UpdateError::BadSignature(_))
    ));

    let stranger = ReleaseKey::generate();
    let fetch = release(
        latest_json("9.9.9", &stranger.sign_b64(&signed)),
        signed.clone(),
    );
    assert!(matches!(
        check(&fetch, &key.pubkey_b64()),
        Err(UpdateError::BadSignature(_))
    ));

    let fetch = release(latest_json("9.9.9", "bm90IGEgc2lnbmF0dXJl"), signed.clone());
    assert!(matches!(
        check(&fetch, &key.pubkey_b64()),
        Err(UpdateError::BadSignature(_))
    ));

    let fetch = release(latest_json("9.9.9", &key.sign_b64(&signed)), signed);
    assert_eq!(
        check(&fetch, "TODO_UPDATER_PUBKEY").err(),
        Some(UpdateError::NoKey)
    );
    assert!(fetch.asked().is_empty(), "no key, no request");
}

/// The update allowlist is checked on every hop, and a payload larger than any installer is
/// refused without being read to the end.
#[test]
fn an_update_off_the_release_hosts_or_over_the_budget_is_refused() {
    let key = ReleaseKey::generate();
    let signed = payload();

    let elsewhere = "https://example.com/OpenConvert.AppImage";
    let fetch = FakeFetch::new(&[
        (
            ENDPOINT,
            Answer::Body(latest_json("9.9.9", &key.sign_b64(&signed))),
        ),
        (ASSET, Answer::Redirect(elsewhere.to_owned())),
        (elsewhere, Answer::Body(signed.clone())),
    ]);
    let refused = fetch_update(
        &fetch,
        ENDPOINT,
        "1.0.0",
        &platform_keys(None),
        &key.pubkey_b64(),
        &CONFIG,
    );
    assert!(matches!(
        refused,
        Err(UpdateError::Net(NetError::HostNotAllowed { .. }))
    ));
    assert!(
        !fetch.asked().contains(&elsewhere.to_owned()),
        "never fetched"
    );

    // A model host is not a release host.
    let model_host = "https://huggingface.co/x/latest.json";
    let fetch = FakeFetch::new(&[]);
    assert!(matches!(
        fetch_update(
            &fetch,
            model_host,
            "1.0.0",
            &platform_keys(None),
            &key.pubkey_b64(),
            &CONFIG
        ),
        Err(UpdateError::Net(NetError::HostNotAllowed { .. }))
    ));

    let small = UpdateConfig {
        payload_max_bytes: 100,
        ..CONFIG
    };
    let fetch = release(latest_json("9.9.9", &key.sign_b64(&signed)), signed);
    assert_eq!(
        fetch_update(
            &fetch,
            ENDPOINT,
            "1.0.0",
            &platform_keys(None),
            &key.pubkey_b64(),
            &small
        )
        .err(),
        Some(UpdateError::TooLarge(100))
    );
}

/// PHASE 14 detail 12 meets PHASE 15 detail 5: an update check is a connection like any other, and
/// the network audit log has a line for every hop of it — the manifest, the release page's redirect,
/// the payload — under its own purpose, `update`, so Settings › Network log can say what it was. Run
/// through the real HTTP client against a loopback server.
#[test]
fn every_update_connection_is_in_the_network_audit_log() {
    let key = ReleaseKey::generate();
    let signed = payload();
    let dir = scratch("audit");
    let path = dir.join("network-audit.log");
    oc_net::audit::install(oc_net::audit::AuditLog::new(path.clone(), 1 << 20));

    let server = common::Server::start();
    let path_of = |url: &str| url.trim_start_matches("https://github.com").to_owned();
    server.route(
        "github.com",
        &path_of(ENDPOINT),
        common::Answer::Body(latest_json("9.9.9", &key.sign_b64(&signed))),
    );
    server.route(
        "github.com",
        &path_of(ASSET),
        common::Answer::Redirect(CDN.to_owned()),
    );
    server.route(
        "release-assets.githubusercontent.com",
        CDN.trim_start_matches("https://release-assets.githubusercontent.com"),
        common::Answer::Body(signed.clone()),
    );
    let fetch = server.fetch_for(oc_net::audit::Purpose::Update);
    let update = fetch_update(
        fetch.as_ref(),
        ENDPOINT,
        "1.0.0",
        &platform_keys(None),
        &key.pubkey_b64(),
        &CONFIG,
    )
    .expect("verifies")
    .expect("newer");
    assert_eq!(update.bytes(), signed.as_slice());

    let lines: Vec<serde_json::Value> = std::fs::read_to_string(&path)
        .expect("the log was written")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a JSON line"))
        .collect();
    assert_eq!(lines.len(), 3, "one line per connection: {lines:?}");
    for line in &lines {
        assert_eq!(line["purpose"], "update", "{line}");
    }
    assert_eq!(lines[0]["outcome"], "ok");
    assert_eq!(lines[1]["outcome"], "HTTP 302");
    assert_eq!(lines[2]["outcome"], "ok");
    assert_eq!(
        lines[2]["bytes"],
        serde_json::json!(signed.len()),
        "the payload's size"
    );
    let _ = std::fs::remove_dir_all(dir);
}
