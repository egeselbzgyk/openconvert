//! `HttpTransport`: the `oc_ai::Transport` a real endpoint is reached through.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use oc_ai::transport::{Transport, TransportError};
use oc_net::transport::HttpTransport;

/// Answers one request with `status` and `body`, and sends back what it was asked.
fn one_shot(status: u16, body: &'static str) -> (u16, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut head = String::new();
        let mut length = 0usize;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("line");
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().expect("length");
            }
            head.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }
        let mut request_body = vec![0u8; length];
        reader.read_exact(&mut request_body).expect("body");
        head.push_str(&String::from_utf8_lossy(&request_body));
        tx.send(head).expect("send");
        let mut stream = stream;
        write!(
            stream,
            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("reply");
    });
    (port, rx)
}

#[test]
fn http_transport_posts_json_with_the_bearer_key() {
    let (port, asked) = one_shot(200, r#"{"ok":true}"#);
    let transport = HttpTransport::new(
        &format!("http://127.0.0.1:{port}"),
        Some("per-run-key".to_owned().into()),
    )
    .expect("a loopback base");
    let reply = transport
        .post_json("/v1/chat/completions", r#"{"q":1}"#, Duration::from_secs(5))
        .expect("a reply");
    assert_eq!(reply, r#"{"ok":true}"#);
    let request = asked.recv().expect("the request");
    assert!(
        request.starts_with("POST /v1/chat/completions HTTP/1.1"),
        "{request}"
    );
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer per-run-key"),
        "{request}"
    );
    assert!(request
        .to_ascii_lowercase()
        .contains("content-type: application/json"));
    assert!(request.ends_with(r#"{"q":1}"#), "{request}");
}

#[test]
fn http_transport_reports_a_status_and_an_unreachable_endpoint() {
    let (port, _asked) = one_shot(401, r#"{"error":"no key"}"#);
    let transport = HttpTransport::new(&format!("http://127.0.0.1:{port}"), None).expect("a base");
    assert_eq!(
        transport.post_json("/health", "{}", Duration::from_secs(5)),
        Err(TransportError::Status { status: 401 })
    );

    // Nothing listens on a port that was just released.
    let port = TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port();
    let transport = HttpTransport::new(&format!("http://127.0.0.1:{port}"), None).expect("a base");
    assert!(matches!(
        transport.post_json("/health", "{}", Duration::from_secs(5)),
        Err(TransportError::Unreachable(_))
    ));
}

#[test]
fn the_api_key_is_not_in_the_transports_debug_output() {
    let transport =
        HttpTransport::new("http://127.0.0.1:1", Some("s3cret".to_owned().into())).expect("a base");
    assert!(!format!("{transport:?}").contains("s3cret"));
}
