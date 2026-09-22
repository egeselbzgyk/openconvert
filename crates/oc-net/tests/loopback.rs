//! The port an engine-owned sidecar is given (PHASE 9 detail 3, row 9.8's "port ≠ 8080").

/// llama-server's own default port, which a second copy of the app — or anything else — may hold.
const LLAMA_SERVER_DEFAULT_PORT: u16 = 8080;

#[test]
fn free_port_is_an_ephemeral_loopback_port() {
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..16 {
        let port = oc_net::loopback::free_port().expect("a free port");
        assert_ne!(port, LLAMA_SERVER_DEFAULT_PORT);
        assert_ne!(port, 0);
        // Free means bindable, on loopback, right now.
        std::net::TcpListener::bind(("127.0.0.1", port)).expect("the port is free");
        seen.insert(port);
    }
    assert!(
        seen.len() > 1,
        "the kernel hands out different ports: {seen:?}"
    );
}
