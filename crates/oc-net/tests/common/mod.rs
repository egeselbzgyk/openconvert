//! A loopback HTTP fixture for the downloader's tests (IMPLEMENTATION_PLAN PHASE 9, rows 9.3–9.6).
//!
//! The downloader decides what to fetch against real `https://huggingface.co/...` URLs; the fixture
//! answers them on `127.0.0.1` so the real `ureq` client, the real body reader and the real
//! redirect handling run over a real socket without leaving the machine. [`Loopback`] is the only
//! thing that knows the two apart: it maps `https://<host>/<path>` to
//! `http://127.0.0.1:<port>/<host>/<path>` and delegates to [`HttpFetch`].

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oc_net::download::{Fetch, Fetched, HttpFetch};
use oc_net::registry::{ModelEntry, ModelRegistry};
use oc_net::NetError;
use sha2::{Digest, Sha256};

pub const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// How the fixture answers one path.
#[derive(Clone, Debug)]
pub enum Answer {
    Body(Vec<u8>),
    /// `Content-Length` says the whole body, and the connection closes halfway through it.
    Truncated(Vec<u8>),
    Redirect(String),
    Status(u16),
}

pub struct Server {
    pub port: u16,
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

    pub fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }

    pub fn fetch(&self) -> Box<dyn Fetch> {
        Box::new(Loopback {
            port: self.port,
            inner: HttpFetch::new(Duration::from_secs(5)),
        })
    }

    /// [`Server::fetch`], recording its connections under `purpose`.
    #[allow(dead_code)] // Used by the updater tests; each test binary compiles this module.
    pub fn fetch_for(&self, purpose: oc_net::audit::Purpose) -> Box<dyn Fetch> {
        Box::new(Loopback {
            port: self.port,
            inner: HttpFetch::new(Duration::from_secs(5)).with_purpose(purpose),
        })
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
    let _ = match answer {
        Some(Answer::Body(body)) => write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .and_then(|()| stream.write_all(&body)),
        Some(Answer::Truncated(body)) => write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .and_then(|()| stream.write_all(&body[..body.len() / 2])),
        Some(Answer::Redirect(location)) => write!(
            stream,
            "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ),
        Some(Answer::Status(status)) => write!(
            stream,
            "HTTP/1.1 {status} Nope\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ),
        None => write!(
            stream,
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ),
    };
    let _ = stream.flush();
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

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A registry entry for `body`, downloadable from `url_template`.
pub fn entry(body: &[u8], url_template: &str) -> ModelEntry {
    let text = format!(
        r#"schema_version = 1
default = "tiny"

[[model]]
id            = "tiny"
tier          = "default"
display_name  = "Tiny"
family        = "qwen3"
arch          = "dense"
license       = "Apache-2.0"
notice_text   = "Tiny (c) nobody, Apache-2.0."
repo          = "org/tiny-GGUF"
revision      = "{COMMIT}"
file          = "tiny.gguf"
url_template  = "{url_template}"
sha256        = "{}"
size_bytes    = {}
context       = 8192
parallel      = 1
min_ram_bytes = 1
prompt_profile = "qwen3-chatml"
cache_reuse   = true
"#,
        sha256_hex(body),
        body.len()
    );
    ModelRegistry::parse(&text)
        .expect("a valid registry")
        .entries()[0]
        .clone()
}

pub const TEMPLATE: &str = "https://huggingface.co/{repo}/resolve/{revision}/{file}";

/// The path the default template resolves to for [`entry`].
pub fn resolve_path() -> String {
    format!("/org/tiny-GGUF/resolve/{COMMIT}/tiny.gguf")
}

/// A body big enough to arrive in more than one read.
pub fn body() -> Vec<u8> {
    (0..300_000u32).map(|i| (i % 251) as u8).collect()
}

pub struct Quiet;
impl oc_net::download::DownloadProgress for Quiet {
    fn bytes(&self, _: u64, _: u64) {}
}

pub fn config() -> oc_net::download::DownloadConfig {
    oc_net::download::DownloadConfig { max_redirects: 5 }
}
