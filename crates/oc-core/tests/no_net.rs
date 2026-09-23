//! Row 9.7 and acceptance criterion A9.3: the conversion path cannot reach the network (D13.9).
//!
//! "Downloads never happen during a conversion" is only a property if the orchestrator cannot open a
//! socket at all. Two ways it could, both closed here — the same two `oc-ai`'s test 8.15 closes:
//!
//! 1. **A dependency.** `Cargo.lock` is walked from `oc-core` through every edge it records, so the
//!    set walked is a superset of `cargo tree -p oc-core`; neither `oc-net` nor any socket crate may
//!    be in it. This is `cargo tree -p oc-core -i ureq` being empty, stated as a test that runs on
//!    every platform instead of a shell step that runs on one.
//! 2. **The standard library.** `std::net` needs no dependency. `oc-core`'s own sources are scanned
//!    for it. The sidecar supervisor lives here and reaches its server only through a probe its
//!    caller supplies — the caller is the thing that links `oc-net`.
//!
//! The other half of A9.3 is CI's `no-network` job, which runs the conversion suite under
//! `unshare -n`, where no socket can open whatever the code does.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Crates that open sockets or wrap something that does, plus the one workspace crate allowed to.
const FORBIDDEN: &[&str] = &[
    "oc-net",
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
    "rustls",
    "socket2",
    "tokio",
    "tungstenite",
    "ureq",
];

const STD_SOCKETS: &[&str] = &["std::net", "TcpStream", "TcpListener", "UdpSocket"];

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn lock_graph() -> BTreeMap<String, BTreeSet<String>> {
    let text = std::fs::read_to_string(workspace().join("Cargo.lock")).expect("Cargo.lock");
    let lock: toml::Table = toml::from_str(&text).expect("Cargo.lock is TOML");
    let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for package in lock["package"].as_array().expect("a package list") {
        let name = package["name"].as_str().expect("a name").to_owned();
        let dependencies = graph.entry(name).or_default();
        for dependency in package
            .get("dependencies")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
        {
            let spec = dependency.as_str().expect("a spec");
            dependencies.insert(spec.split_whitespace().next().expect("a name").to_owned());
        }
    }
    graph
}

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

/// Row 9.7.
#[test]
fn oc_core_has_no_net_dependency() {
    let graph = lock_graph();
    assert!(graph.contains_key("oc-core"), "Cargo.lock records oc-core");
    // The walk is only a proof if the crates it looks for are ones the lock file could name: the
    // downloader's own graph has them, so a misspelt list would fail here rather than pass.
    let net = reachable(&graph, "oc-net");
    for name in ["ureq", "rustls"] {
        assert!(net.contains(name), "oc-net reaches {name}");
    }

    let core = reachable(&graph, "oc-core");
    let found: Vec<&str> = FORBIDDEN
        .iter()
        .copied()
        .filter(|name| core.contains(*name))
        .collect();
    assert!(
        found.is_empty(),
        "oc-core reaches {found:?}: the conversion path must not be able to open a socket (D13.9)"
    );

    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(!files.is_empty());
    for file in files {
        let text = std::fs::read_to_string(&file).expect("a source file");
        for needle in STD_SOCKETS {
            assert!(
                !text.contains(needle),
                "{} mentions `{needle}`: only oc-net opens sockets (D13.9)",
                file.display()
            );
        }
    }
}
