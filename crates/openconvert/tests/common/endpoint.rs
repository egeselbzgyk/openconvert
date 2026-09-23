//! A model server on `127.0.0.1` for the binary's provider tests (PHASE 11): answers a table of
//! `(method, path)` routes and records every request it was sent — method, path, headers and body —
//! so a test can see what left the engine, and where.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

/// One request as the server received it.
#[derive(Clone, Debug)]
pub struct Received {
    pub method: String,
    pub path: String,
    /// Every header line, as sent.
    pub headers: Vec<String>,
    pub body: String,
}

type Routes = BTreeMap<(String, String), (u16, String)>;

pub struct Endpoint {
    pub port: u16,
    received: Arc<Mutex<Vec<Received>>>,
}

/// `llama-server`'s `/props`, enough for the capability probe to recognise it.
pub const LLAMA_PROPS: &str =
    r#"{"default_generation_settings":{"n_ctx":8192},"total_slots":1,"build_info":"stub"}"#;

impl Endpoint {
    pub fn start(routes: &[(&str, &str, u16, &str)]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let routes: Routes = routes
            .iter()
            .map(|(method, path, status, body)| {
                (
                    ((*method).to_owned(), (*path).to_owned()),
                    (*status, (*body).to_owned()),
                )
            })
            .collect();
        let routes = Arc::new(routes);
        let received: Arc<Mutex<Vec<Received>>> = Arc::default();
        let log = received.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let routes = routes.clone();
                let log = log.clone();
                std::thread::spawn(move || serve(stream, &routes, &log));
            }
        });
        Self { port, received }
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn received(&self) -> Vec<Received> {
        self.received.lock().expect("not poisoned").clone()
    }
}

fn serve(stream: TcpStream, routes: &Routes, log: &Mutex<Vec<Received>>) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let mut headers = Vec::new();
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).map_or(true, |n| n == 0) || line == "\r\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
        headers.push(line.trim_end().to_owned());
    }
    let mut body = vec![0u8; length];
    let _ = reader.read_exact(&mut body);
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let path = parts.next().unwrap_or("/").to_owned();
    log.lock().expect("not poisoned").push(Received {
        method: method.clone(),
        path: path.clone(),
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    });
    let (status, reply) = routes
        .get(&(method, path))
        .cloned()
        .unwrap_or((404, r#"{"error":"not found"}"#.to_owned()));
    let mut stream = stream;
    let _ = write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
        reply.len()
    );
    let _ = stream.flush();
}
