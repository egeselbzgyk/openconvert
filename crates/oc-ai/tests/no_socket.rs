//! Test 8.15 and acceptance criterion A8.4: `oc-ai` cannot open a socket (D13.9, ARCHITECTURE
//! §3.1 — "`oc-ai` has no network crate").
//!
//! Two ways a crate reaches the network, and both are closed here:
//!
//! 1. **A dependency.** `Cargo.lock` is walked from `oc-ai` through every edge it records — normal,
//!    build and dev alike, and every optional dependency any feature could turn on — so the set
//!    walked is a superset of what `cargo tree -p oc-ai` shows, and a socket crate absent from it
//!    is absent from every build of this crate. The socket crates are named here and held equal
//!    to the ones `deny.toml` confines to `oc-net`, so the two lists cannot drift apart.
//! 2. **The standard library.** `std::net` needs no dependency at all, which is the hole a
//!    dependency ban cannot see. `oc-ai`'s own sources — and its tests, which are where a careless
//!    "just call the real server" would appear — are scanned for it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Crates that open sockets or wrap something that does. Every one `deny.toml` confines to
/// `oc-net` must be here; the rest are the ones a contributor would plausibly reach for.
const SOCKET_CRATES: &[&str] = &[
    "async-std",
    "attohttpc",
    "curl",
    "curl-sys",
    "h2",
    "hyper",
    "hyper-util",
    "isahc",
    "minreq",
    "mio",
    "reqwest",
    "socket2",
    "tokio",
    "tungstenite",
    "ureq",
];

/// What in a source file would open a socket without any dependency.
const STD_SOCKETS: &[&str] = &["std::net", "TcpStream", "TcpListener", "UdpSocket"];

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every package `Cargo.lock` records, by name, with the names of what it depends on.
fn lock_graph() -> BTreeMap<String, BTreeSet<String>> {
    let text = std::fs::read_to_string(workspace().join("Cargo.lock")).expect("Cargo.lock");
    let lock: toml::Table = toml::from_str(&text).expect("Cargo.lock is TOML");
    let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for package in lock
        .get("package")
        .and_then(toml::Value::as_array)
        .expect("a package list")
    {
        let name = package["name"].as_str().expect("a name").to_owned();
        let dependencies = graph.entry(name).or_default();
        for dependency in package
            .get("dependencies")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
        {
            // `name`, `name version` or `name version (source)`: the name is the first word.
            let spec = dependency.as_str().expect("a dependency spec");
            let name = spec.split_whitespace().next().expect("a name");
            dependencies.insert(name.to_owned());
        }
    }
    graph
}

/// Everything reachable from `root`, `root` included.
fn reachable(graph: &BTreeMap<String, BTreeSet<String>>, root: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![root.to_owned()];
    while let Some(name) = stack.pop() {
        if seen.insert(name.clone()) {
            stack.extend(graph.get(&name).into_iter().flatten().cloned());
        }
    }
    seen
}

/// The crates `deny.toml` bans everywhere except inside `oc-net` — the privacy ban (D13.9).
fn confined_to_oc_net() -> BTreeSet<String> {
    let text = std::fs::read_to_string(workspace().join("deny.toml")).expect("deny.toml");
    let deny: toml::Table = toml::from_str(&text).expect("deny.toml is TOML");
    deny["bans"]["deny"]
        .as_array()
        .expect("a ban list")
        .iter()
        .filter(|ban| {
            ban.get("wrappers")
                .and_then(toml::Value::as_array)
                .is_some_and(|wrappers| {
                    wrappers
                        .iter()
                        .any(|wrapper| wrapper.as_str() == Some("oc-net"))
                })
        })
        .map(|ban| ban["name"].as_str().expect("a name").to_owned())
        .collect()
}

fn rust_files(directory: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("a directory") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

/// Test 8.15.
#[test]
fn oc_ai_has_no_socket_dependency() {
    let graph = lock_graph();
    assert!(graph.contains_key("oc-ai"), "Cargo.lock records oc-ai");
    let reached = reachable(&graph, "oc-ai");
    assert!(
        reached.contains("serde_json") && reached.contains("oc-model"),
        "the walk follows edges: {reached:?}"
    );
    assert!(!reached.contains("oc-net"), "oc-ai must not reach oc-net");

    let sockets: Vec<&&str> = SOCKET_CRATES
        .iter()
        .filter(|name| reached.contains(**name))
        .collect();
    assert!(
        sockets.is_empty(),
        "oc-ai reaches socket-capable crates through Cargo.lock: {sockets:?}"
    );

    let confined = confined_to_oc_net();
    assert!(
        confined.contains("ureq"),
        "deny.toml still confines ureq to oc-net"
    );
    for name in &confined {
        assert!(
            SOCKET_CRATES.contains(&name.as_str()),
            "deny.toml confines {name} to oc-net and this test does not check it"
        );
    }

    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&crate_root.join("src"), &mut files);
    rust_files(&crate_root.join("tests"), &mut files);
    let this_file = crate_root.join("tests").join("no_socket.rs");
    for file in files.iter().filter(|file| **file != this_file) {
        let text = std::fs::read_to_string(file).expect("a source file");
        for needle in STD_SOCKETS {
            assert!(
                !text.contains(needle),
                "{} names {needle}: oc-ai opens no sockets, even without a dependency",
                file.display()
            );
        }
    }
}
