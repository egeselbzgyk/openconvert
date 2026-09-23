//! PHASE 11 detail 2 and 5: Ollama found on `localhost:11434`, and the capability probe that tells
//! a `llama-server`, an Ollama and any other OpenAI-compatible server apart — by what each answers,
//! never by a version string.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oc_net::detect::{
    api_root, detect_ollama, ollama_transport, probe, Server, OLLAMA_DEFAULT_URL,
};
use oc_net::transport::HttpTransport;

/// A loopback server answering GETs from a table of paths; anything else is a 404.
struct Stub {
    port: u16,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Stub {
    fn start(routes: &[(&str, u16, &str)]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let routes: BTreeMap<String, (u16, String)> = routes
            .iter()
            .map(|(path, status, body)| ((*path).to_owned(), (*status, (*body).to_owned())))
            .collect();
        let routes = Arc::new(routes);
        let asked: Arc<Mutex<Vec<String>>> = Arc::default();
        let log = asked.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let routes = routes.clone();
                let log = log.clone();
                std::thread::spawn(move || serve(stream, &routes, &log));
            }
        });
        Self { port, asked }
    }

    fn base(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().expect("not poisoned").clone()
    }
}

fn serve(stream: TcpStream, routes: &BTreeMap<String, (u16, String)>, log: &Mutex<Vec<String>>) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).map_or(true, |n| n == 0) || header == "\r\n" {
            break;
        }
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or("/").to_owned();
    log.lock()
        .expect("not poisoned")
        .push(format!("{method} {path}"));
    let (status, body) = routes
        .get(&path)
        .cloned()
        .unwrap_or((404, r#"{"error":"not found"}"#.to_owned()));
    let mut stream = stream;
    let _ = write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}

const TAGS: &str = r#"{"models":[{"name":"qwen3:1.7b","model":"qwen3:1.7b","modified_at":"2026-09-01T10:00:00Z","size":1400000000,"digest":"abc","details":{"format":"gguf","family":"qwen3","parameter_size":"1.7B","quantization_level":"Q4_K_M"}},{"name":"llama3.2:3b","model":"llama3.2:3b","modified_at":"2026-08-01T10:00:00Z","size":2000000000,"digest":"def","details":{}}]}"#;

fn timeout() -> Duration {
    Duration::from_secs(5)
}

/// Row 11.1. `detect_ollama` asks `GET /api/tags` and returns the models Ollama lists; the default
/// endpoint it is pointed at is `http://localhost:11434`, on this machine, so it needs no consent.
#[test]
fn ollama_detected_on_default_port() {
    assert_eq!(OLLAMA_DEFAULT_URL, "http://localhost:11434");
    let default = ollama_transport().expect("the default endpoint is loopback: no consent needed");
    assert_eq!(default.base(), OLLAMA_DEFAULT_URL);

    let ollama = Stub::start(&[("/api/tags", 200, TAGS)]);
    let transport = HttpTransport::new(&ollama.base(), None).expect("loopback");
    let info = detect_ollama(&transport, timeout()).expect("Ollama is detected");
    assert_eq!(info.models, ["qwen3:1.7b", "llama3.2:3b"]);
    assert_eq!(ollama.asked(), ["GET /api/tags"]);
}

/// Nothing listening, a server that is not Ollama, and a reply that is not a model list: not
/// detected, and not an error — the provider list simply has no Ollama in it.
#[test]
fn nothing_that_is_not_ollama_is_detected() {
    let closed = HttpTransport::new("http://127.0.0.1:1", None).expect("loopback");
    assert_eq!(detect_ollama(&closed, timeout()), None);

    for body in [
        "<html>hello</html>",
        r#"{"data":[]}"#,
        r#"{"models":"many"}"#,
    ] {
        let other = Stub::start(&[("/api/tags", 200, body)]);
        let transport = HttpTransport::new(&other.base(), None).expect("loopback");
        assert_eq!(detect_ollama(&transport, timeout()), None, "{body}");
    }
    let missing = Stub::start(&[]);
    let transport = HttpTransport::new(&missing.base(), None).expect("loopback");
    assert_eq!(detect_ollama(&transport, timeout()), None);
}

/// The capability probe: `llama-server` answers `/props`, Ollama `/api/tags`, anything else
/// OpenAI-compatible `/v1/models` — asked in that order, once, and what a server answers decides
/// what it is.
#[test]
fn the_probe_tells_the_servers_apart_by_what_they_answer() {
    let llama = Stub::start(&[
        (
            "/props",
            200,
            r#"{"default_generation_settings":{"n_ctx":8192},"total_slots":1,"build_info":"b10456"}"#,
        ),
        ("/api/tags", 200, TAGS),
    ]);
    let transport = HttpTransport::new(&llama.base(), None).expect("loopback");
    assert_eq!(probe(&transport, timeout()), Ok(Server::LlamaServer));
    assert_eq!(llama.asked(), ["GET /props"], "one question is enough");

    let ollama = Stub::start(&[("/api/tags", 200, TAGS)]);
    let transport = HttpTransport::new(&ollama.base(), None).expect("loopback");
    assert_eq!(
        probe(&transport, timeout()),
        Ok(Server::Ollama {
            models: vec!["qwen3:1.7b".to_owned(), "llama3.2:3b".to_owned()]
        })
    );
    assert_eq!(ollama.asked(), ["GET /props", "GET /api/tags"]);

    let studio = Stub::start(&[(
        "/v1/models",
        200,
        r#"{"object":"list","data":[{"id":"qwen3-8b-instruct","object":"model"}]}"#,
    )]);
    let transport = HttpTransport::new(&studio.base(), None).expect("loopback");
    assert_eq!(
        probe(&transport, timeout()),
        Ok(Server::OpenAiCompatible {
            models: vec!["qwen3-8b-instruct".to_owned()]
        })
    );

    let nothing = Stub::start(&[]);
    let transport = HttpTransport::new(&nothing.base(), None).expect("loopback");
    assert!(probe(&transport, timeout()).is_err(), "no server answered");
    let closed = HttpTransport::new("http://127.0.0.1:1", None).expect("loopback");
    assert!(probe(&closed, timeout()).is_err());
}

/// A base URL is written with or without `/v1` and a trailing slash; the probe and the chat path are
/// asked of the server's root either way.
#[test]
fn the_api_root_drops_v1_and_trailing_slashes() {
    for (url, root) in [
        ("http://127.0.0.1:8080", "http://127.0.0.1:8080"),
        ("http://127.0.0.1:8080/", "http://127.0.0.1:8080"),
        ("https://llm.example.org/v1", "https://llm.example.org"),
        ("https://llm.example.org/v1/", "https://llm.example.org"),
        (
            "https://llm.example.org/proxy/v1",
            "https://llm.example.org/proxy",
        ),
        ("http://localhost:11434", "http://localhost:11434"),
    ] {
        assert_eq!(api_root(url), root, "{url}");
    }
}
