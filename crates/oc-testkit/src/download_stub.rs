//! A loopback stand-in for the model host, for download tests outside `oc-net` — the desktop app's
//! model manager (IMPLEMENTATION_PLAN Phase 12 row 12.12).
//!
//! The downloader decides what to fetch against the real `https://huggingface.co/...` URLs the
//! registry pins, and checks every one against the allowlist. [`Server::fetch`] then fetches them
//! from `http://127.0.0.1:<port>/<host>/<path>` with `oc-net`'s real client, so the real body
//! reader, redirect handling and hashing run over a real socket without leaving the machine.
//! [`Answer::Slow`] trickles a body out chunk by chunk, so a test can see progress arrive and
//! cancel a download while it is in flight.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oc_net::download::{Fetch, Fetched, HttpFetch};
use oc_net::NetError;

/// How the stub answers one path.
#[derive(Clone, Debug)]
pub enum Answer {
    Body(Vec<u8>),
    /// The whole body, `chunk` bytes at a time with `pause` between them. `Content-Length` says
    /// the whole length up front, as a real host does.
    Slow {
        body: Vec<u8>,
        chunk: usize,
        pause: Duration,
    },
    Status(u16),
}

pub struct Server {
    port: u16,
    routes: Arc<Mutex<BTreeMap<String, Answer>>>,
    requests: Arc<AtomicUsize>,
}

impl Server {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let routes: Arc<Mutex<BTreeMap<String, Answer>>> = Arc::default();
        let requests: Arc<AtomicUsize> = Arc::default();
        let (r, n) = (routes.clone(), requests.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                n.fetch_add(1, Ordering::SeqCst);
                let routes = r.clone();
                std::thread::spawn(move || serve(stream, &routes));
            }
        });
        Self {
            port,
            routes,
            requests,
        }
    }

    /// Answer `https://<host><path>` with `answer`.
    pub fn route(&self, host: &str, path: &str, answer: Answer) {
        self.routes
            .lock()
            .expect("routes")
            .insert(format!("/{host}{path}"), answer);
    }

    /// How many connections have been made.
    pub fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }

    /// A [`Fetch`] that reaches this stub instead of the internet.
    pub fn fetch(&self) -> Box<dyn Fetch> {
        loopback_fetch(self.port)
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

fn serve(stream: TcpStream, routes: &Mutex<BTreeMap<String, Answer>>) {
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
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_owned();
    let answer = routes.lock().expect("routes").get(&path).cloned();
    let mut stream = stream;
    let head = |status: &str, length: usize| {
        format!("HTTP/1.1 {status}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n")
    };
    let _ = match answer {
        Some(Answer::Body(body)) => stream
            .write_all(head("200 OK", body.len()).as_bytes())
            .and_then(|()| stream.write_all(&body)),
        Some(Answer::Slow { body, chunk, pause }) => stream
            .write_all(head("200 OK", body.len()).as_bytes())
            .and_then(|()| {
                for piece in body.chunks(chunk.max(1)) {
                    stream.write_all(piece)?;
                    stream.flush()?;
                    std::thread::sleep(pause);
                }
                Ok(())
            }),
        Some(Answer::Status(status)) => {
            stream.write_all(head(&format!("{status} Nope"), 0).as_bytes())
        }
        None => stream.write_all(head("404 Not Found", 0).as_bytes()),
    };
    let _ = stream.flush();
}

/// A [`Fetch`] that reaches the stub listening on `port`: for a caller that makes a fetcher per
/// download and so needs to make one without holding the [`Server`].
pub fn loopback_fetch(port: u16) -> Box<dyn Fetch> {
    Box::new(Loopback {
        port,
        inner: HttpFetch::new(Duration::from_secs(5)),
    })
}

/// `https://<host>/<path>` → `http://127.0.0.1:<port>/<host>/<path>`, through the real client.
struct Loopback {
    port: u16,
    inner: HttpFetch,
}

impl Fetch for Loopback {
    fn get(&self, url: &str) -> Result<Fetched, NetError> {
        let rest = url
            .strip_prefix("https://")
            .expect("the downloader only asks for https");
        self.inner
            .get(&format!("http://127.0.0.1:{}/{rest}", self.port))
    }
}

/// Lowercase hex SHA-256 of `bytes`, computed by the same code the downloader checks with.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let (_, hex) = oc_net::verify::copy_hashed(
        &mut &bytes[..],
        &mut std::io::sink(),
        u64::MAX,
        &mut |_| {},
        &|| false,
    )
    .expect("hashes");
    hex
}

/// A body big enough to arrive in many reads.
pub fn body(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}
