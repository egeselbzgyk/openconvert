//! The desktop app's form of the command line: one argument, a job spec (D13.2, RT B15).
//!
//! These run the built binary, because what is under test is what a supervisor sees: the exit
//! code, the NDJSON on stderr, and the files on disk.

use std::path::{Path, PathBuf};
use std::process::Command;

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_openconvert"))
}

fn fixture(name: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(format!("{name}.pdf"));
    assert!(
        path.is_file(),
        "missing fixture {}; run `cargo run -p xtask -- fixtures`",
        path.display()
    );
    path
}

/// A fresh scratch directory per test.
fn scratch(test: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("openconvert-jobspec-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the scratch directory is made");
    directory
}

/// Run the engine with exactly one argument and parse every stderr line as an event.
fn run_spec(spec: &Path) -> (Option<i32>, Vec<serde_json::Value>) {
    let output = Command::new(binary())
        .arg(spec)
        .output()
        .expect("the binary runs");
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    let events = stderr
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("stderr carries only events; {line:?}: {e}"))
        })
        .collect();
    (output.status.code(), events)
}

fn write_spec(directory: &Path, spec: &serde_json::Value) -> PathBuf {
    let path = directory.join("job.json");
    std::fs::write(&path, serde_json::to_vec_pretty(spec).expect("serialises"))
        .expect("the spec is written");
    path
}

#[test]
fn a_job_spec_path_is_the_only_argument_the_engine_needs() {
    let directory = scratch("ok");
    let output = directory.join("book.epub");
    let spec = write_spec(
        &directory,
        &serde_json::json!({
            "schema": "openconvert.job/1",
            "job_id": "job-1",
            "input": {"path": fixture("f01_prose_single_column")},
            "output": {"path": output},
            "preset": "novel",
            "locale": "de"
        }),
    );

    let (code, events) = run_spec(&spec);
    assert_eq!(code, Some(0), "events: {events:#?}");

    assert_eq!(events[0]["t"], "hello", "hello is always the first line");
    let job = events
        .iter()
        .find(|event| event["t"] == "job")
        .expect("a job event");
    assert_eq!(job["job_id"], "job-1");
    assert_eq!(job["phase"], "started");
    assert!(job["pages"].as_u64().is_some_and(|pages| pages > 0));

    let done = events.last().expect("events");
    assert_eq!(done["t"], "done");
    assert_eq!(done["status"], "ok");
    let report = PathBuf::from(done["report_path"].as_str().expect("done names the report"));
    assert!(
        report.is_file(),
        "the report is where done says it is: {report:?}"
    );
    assert_eq!(done["output_path"], output.to_string_lossy().as_ref());
    assert!(output.is_file());
}

#[test]
fn an_invalid_job_spec_is_exit_2_with_e_jobspec() {
    let directory = scratch("invalid");
    let output = directory.join("book.epub");
    // `--model-path` smuggled in as a field is exactly what the schema's closed top level is for.
    let spec = write_spec(
        &directory,
        &serde_json::json!({
            "schema": "openconvert.job/1",
            "input": {"path": fixture("f01_prose_single_column")},
            "output": {"path": output},
            "model_path": "/tmp/evil.gguf"
        }),
    );

    let (code, events) = run_spec(&spec);
    assert_eq!(code, Some(2));
    assert_eq!(
        events[0]["t"], "hello",
        "the supervisor can still check versions"
    );
    let fatal = events.last().expect("events");
    assert_eq!(fatal["t"], "fatal");
    assert_eq!(fatal["code"], "E_JOBSPEC");
    assert!(
        !output.exists(),
        "nothing was attempted for an invalid spec"
    );
}

#[test]
fn a_job_spec_never_replaces_an_existing_output_unless_it_says_so() {
    let directory = scratch("exists");
    let output = directory.join("book.epub");
    std::fs::write(&output, b"somebody's earlier book").expect("written");
    let spec = write_spec(
        &directory,
        &serde_json::json!({
            "schema": "openconvert.job/1",
            "input": {"path": fixture("f01_prose_single_column")},
            "output": {"path": output}
        }),
    );

    let (code, events) = run_spec(&spec);
    assert_eq!(code, Some(2));
    assert_eq!(events.last().expect("events")["code"], "E_OUTPUT_EXISTS");
    assert_eq!(
        std::fs::read(&output).expect("still there"),
        b"somebody's earlier book"
    );
}

#[test]
fn a_job_spec_whose_input_changed_is_refused() {
    let directory = scratch("changed");
    let spec = write_spec(
        &directory,
        &serde_json::json!({
            "schema": "openconvert.job/1",
            "input": {"path": fixture("f01_prose_single_column"), "sha256": "0".repeat(64)},
            "output": {"path": directory.join("book.epub")}
        }),
    );

    let (code, events) = run_spec(&spec);
    assert_eq!(code, Some(2));
    assert_eq!(events.last().expect("events")["code"], "E_INPUT_CHANGED");
}

#[test]
fn a_mistyped_subcommand_is_still_a_usage_error() {
    let output = Command::new(binary())
        .arg("convrt")
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown subcommand"));
}

/// RT A5.7: `--version`, one argument, is the desktop app's startup handshake. With stderr piped
/// the engine answers with `hello`; stdout still carries the line a person reads.
#[test]
fn version_answers_the_handshake_with_hello() {
    let output = Command::new(binary())
        .arg("--version")
        .output()
        .expect("the binary runs");
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("openconvert "));
    let first = String::from_utf8_lossy(&output.stderr)
        .lines()
        .next()
        .map(str::to_owned)
        .expect("a hello line");
    let hello: serde_json::Value = serde_json::from_str(&first).expect("an event");
    assert_eq!(hello["t"], "hello");
    assert_eq!(hello["engine_version"], env!("CARGO_PKG_VERSION"));
}

/// D13.11: a job spec never carries a password. It comes from a file the spec names or from
/// `OC_PDF_PASSWORD`, which is how the desktop app passes the one a user typed for one job.
#[test]
fn a_locked_pdf_opens_with_the_password_from_the_environment() {
    let encrypted = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/fixtures/mutations/h01__encrypted_password.pdf");
    assert!(encrypted.is_file(), "missing {}", encrypted.display());
    let directory = scratch("password");
    let output = directory.join("locked.epub");
    let spec = write_spec(
        &directory,
        &serde_json::json!({
            "schema": "openconvert.job/1",
            "input": {"path": encrypted},
            "output": {"path": output}
        }),
    );

    let refused = Command::new(binary())
        .arg(&spec)
        .env_remove("OC_PDF_PASSWORD")
        .output()
        .expect("runs");
    assert_eq!(refused.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&refused.stderr).contains("E_PASSWORD_REQUIRED"));

    let opened = Command::new(binary())
        .arg(&spec)
        .env("OC_PDF_PASSWORD", "secret")
        .output()
        .expect("runs");
    assert_eq!(
        opened.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&opened.stderr)
    );
    assert!(output.is_file());
}

/// A `llama-server` on loopback, as the desktop app's own server looks to the engine: `/props`
/// (what the capability probe recognises it by) and chat completions only with the key, `/health`
/// without. It records every request line and the key it came with.
struct LlamaDouble {
    url: String,
    seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl LlamaDouble {
    fn start(key: &'static str) -> Self {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let url = format!("http://{}", listener.local_addr().expect("bound"));
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = std::sync::Arc::clone(&seen);
        std::thread::spawn(move || {
            for stream in listener.incoming().map_while(Result::ok) {
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut request = String::new();
                if reader.read_line(&mut request).is_err() {
                    continue;
                }
                let (mut authorised, mut length) = (false, 0usize);
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).map_or(true, |n| n == 0) || line == "\r\n" {
                        break;
                    }
                    let lower = line.to_ascii_lowercase();
                    if let Some(value) = lower.strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                    if lower.starts_with("authorization:") && line.trim_end().ends_with(key) {
                        authorised = true;
                    }
                }
                let mut body = vec![0u8; length];
                let _ = reader.read_exact(&mut body);
                let request = request.trim().to_owned();
                log.lock().expect("the log").push(format!(
                    "{request} {}",
                    if authorised { "key" } else { "nokey" }
                ));
                let (status, reply) = if request.starts_with("GET /health") {
                    (200, r#"{"status":"ok"}"#)
                } else if !authorised {
                    (401, r#"{"error":{"code":401,"message":"Invalid API Key"}}"#)
                } else if request.starts_with("GET /props") {
                    (200, r#"{"default_generation_settings":{}}"#)
                } else if request.starts_with("POST /v1/chat/completions") {
                    (
                        200,
                        r#"{"choices":[{"index":0,"message":{"role":"assistant","content":"{}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#,
                    )
                } else {
                    (404, r#"{"error":{"code":404}}"#)
                };
                let mut stream = stream;
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
            }
        });
        Self { url, seen }
    }

    fn seen(&self) -> Vec<String> {
        self.seen.lock().expect("the log").clone()
    }
}

/// PHASE 12 part B2: the one argument carries AI too. `ai.enabled`, `endpoint`, `api_key_file` and
/// `model_id` reach the engine exactly as `--ai --llm-endpoint … --llm-api-key-file …
/// --llm-model …` would: it asks that endpoint what it is, with that key, and the report says which
/// adapter answered and for which model. Nothing needed consent — the endpoint is this computer.
#[test]
fn a_job_spec_with_ai_on_asks_the_endpoint_it_names() {
    let directory = scratch("ai-on");
    let server = LlamaDouble::start("app-run-key");
    let key_file = directory.join("llm.key");
    std::fs::write(&key_file, "app-run-key").expect("the key is written");
    let output = directory.join("book.epub");
    let spec = write_spec(
        &directory,
        &serde_json::json!({
            "schema": "openconvert.job/1",
            "job_id": "job-ai",
            "input": {"path": fixture("f07_verse_and_quote")},
            "output": {"path": output},
            "ai": {
                "enabled": true,
                "endpoint": server.url,
                "api_key_file": key_file,
                "model_id": "qwen3-1.7b-instruct"
            }
        }),
    );

    let (code, events) = run_spec(&spec);
    assert_eq!(code, Some(0), "events: {events:#?}");
    let done = events.last().expect("events");
    assert_eq!(done["status"], "ok");
    let report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(done["report_path"].as_str().expect("a report")).expect("read"),
    )
    .expect("JSON");
    assert_eq!(
        report["ai"]["provider"], "local_sidecar",
        "{:#}",
        report["ai"]
    );
    assert_eq!(report["ai"]["model_id"], "qwen3-1.7b-instruct");
    assert!(report["consent"].is_null(), "loopback needs no consent");
    assert!(
        server
            .seen()
            .iter()
            .any(|line| line.starts_with("GET /props") && line.ends_with(" key")),
        "the capability probe went to the spec's endpoint with the spec's key: {:?}",
        server.seen()
    );
    assert!(
        !events
            .iter()
            .any(|event| event["t"] == "warning" && event["code"] == "W_LLM_UNAVAILABLE"),
        "the model was reachable"
    );
}

/// With AI on and nothing answering, the book converts without a model and says so — the job
/// spec's form of RT D20's fail-open, which the app shows as its banner.
#[test]
fn a_job_spec_with_ai_on_and_no_model_converts_without_one() {
    let directory = scratch("ai-down");
    // A port that was free a moment ago: nothing listens on it.
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("a loopback port");
    let output = directory.join("book.epub");
    let spec = write_spec(
        &directory,
        &serde_json::json!({
            "schema": "openconvert.job/1",
            "input": {"path": fixture("f01_prose_single_column")},
            "output": {"path": output},
            "ai": {"enabled": true, "endpoint": format!("http://{closed}")}
        }),
    );

    let (code, events) = run_spec(&spec);
    assert_eq!(code, Some(0), "events: {events:#?}");
    assert!(
        events
            .iter()
            .any(|event| event["t"] == "warning" && event["code"] == "W_LLM_UNAVAILABLE"),
        "the banner's warning: {events:#?}"
    );
    assert!(output.is_file());
}
