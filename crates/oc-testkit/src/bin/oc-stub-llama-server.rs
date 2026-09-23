#![forbid(unsafe_code)]
//! A stand-in for `llama-server`, for the sidecar lifecycle tests (PHASE 9 rows 9.8–9.13).
//!
//! It takes the real server's command line, binds exactly the `--host` and `--port` it is given,
//! reads its key from `LLAMA_API_KEY` as the real one does, and answers the endpoints the engine
//! uses the way the real one does: `GET /health` without a key; `GET /props` — what the engine's
//! capability probe recognises a `llama-server` by — and `POST /v1/chat/completions` only with
//! `Authorization: Bearer <key>`. It never loads a model, so the lifecycle tests run on every
//! platform in milliseconds; everything that needs a real model is behind `live-llm`.
//!
//! `OC_STUB_LOADING_MS` makes `/health` answer 503 for that long after start, as the real server
//! does while a model loads.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

const COMPLETION: &str = r#"{"choices":[{"index":0,"message":{"role":"assistant","content":"{}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":12,"completion_tokens":2,"total_tokens":14,"prompt_tokens_details":{"cached_tokens":0}},"timings":{"cache_n":0,"prompt_n":12}}"#;
const UNAUTHORISED: &str =
    r#"{"error":{"code":401,"message":"Invalid API Key","type":"authentication_error"}}"#;
/// The one key of `/props` the capability probe looks for (`oc_net::detect`).
const PROPS: &str = r#"{"default_generation_settings":{"n_ctx":8192},"total_slots":1}"#;
const LOADING: &str =
    r#"{"error":{"code":503,"message":"Loading model","type":"unavailable_error"}}"#;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let host = value("--host").expect("--host");
    let port = value("--port").expect("--port");
    let key = std::env::var("LLAMA_API_KEY").ok();
    let loading = Duration::from_millis(
        std::env::var("OC_STUB_LOADING_MS")
            .ok()
            .and_then(|ms| ms.parse().ok())
            .unwrap_or(0),
    );
    let started = Instant::now();

    let listener = TcpListener::bind(format!("{host}:{port}")).expect("bind");
    for stream in listener.incoming().flatten() {
        let key = key.clone();
        std::thread::spawn(move || serve(stream, key.as_deref(), started.elapsed() < loading));
    }
}

fn serve(stream: TcpStream, key: Option<&str>, loading: bool) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let mut authorization = None;
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).map_or(true, |n| n == 0) || line == "\r\n" {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
        if lower.starts_with("authorization:") {
            authorization = line.split_once(':').map(|(_, v)| v.trim().to_owned());
        }
    }
    let mut body = vec![0u8; length];
    let _ = reader.read_exact(&mut body);

    let mut parts = request_line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let authorised =
        key.is_none_or(|key| authorization.as_deref() == Some(&format!("Bearer {key}")));
    let (status, reply) = match (method, path) {
        ("GET", "/health") if loading => (503, LOADING),
        ("GET", "/health") => (200, r#"{"status":"ok"}"#),
        ("GET", "/props") if !authorised => (401, UNAUTHORISED),
        ("GET", "/props") => (200, PROPS),
        ("POST", "/v1/chat/completions") if !authorised => (401, UNAUTHORISED),
        ("POST", "/v1/chat/completions") => (200, COMPLETION),
        _ => (404, r#"{"error":{"code":404,"message":"File Not Found"}}"#),
    };
    let mut stream = stream;
    let _ = write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
        reply.len()
    );
    let _ = stream.flush();
}
