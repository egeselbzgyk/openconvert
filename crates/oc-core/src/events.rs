//! The NDJSON event protocol the engine speaks on **stderr** (D13.2, §2.3).
//!
//! One UTF-8 JSON object per line. stdout is the data channel and carries nothing else, so
//! `--progress json` and `--dump-stage -` can be used together without interleaving.
//!
//! Events are a code-and-arguments protocol, never prose: the GUI localises. That is why
//! `warning` and `fatal` carry a `code` and an `args` object rather than a sentence.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;

/// The protocol version. A consumer that does not know it must refuse the stream rather
/// than guess, so it is in every line as `v`.
pub const PROTOCOL_VERSION: u32 = 1;

/// Hard cap on one serialised line (§2.3). Beyond it the engine truncates rather than emit
/// a line a line-oriented reader might not survive.
pub const MAX_EVENT_BYTES: usize = 8 * 1024;

/// Written in place of the payload of an event too large to send.
const TRUNCATION_NOTICE: &str = "event truncated";

/// Emits events to a writer, numbering them as it goes.
///
/// `seq` is strictly monotone from zero across the whole run, so a consumer can detect a
/// dropped line. The counter is atomic because progress will be reported from `rayon`
/// workers once per-page parallelism arrives.
pub struct EventSink<W: Write> {
    writer: W,
    seq: AtomicU64,
    enabled: bool,
}

impl<W: Write> EventSink<W> {
    /// A sink that writes events. `enabled` is `--progress json`: when false every emit is
    /// a no-op, so call sites do not have to ask.
    pub fn new(writer: W, enabled: bool) -> Self {
        Self {
            writer,
            seq: AtomicU64::new(0),
            enabled,
        }
    }

    /// The `hello` event. Always the first line (§2.3).
    pub fn hello(&mut self, engine_version: &str, ir_version: u32, pdfium_version: &str) {
        self.emit(
            "hello",
            serde_json::json!({
                "engine_version": engine_version,
                "ir_version": ir_version,
                "protocol": PROTOCOL_VERSION,
                "pdfium_version": pdfium_version,
                "capabilities": ["inspect"],
            }),
        );
    }

    /// The `done` event. Always the last line of a run that reaches an end.
    pub fn done(&mut self, status: &str) {
        self.emit("done", serde_json::json!({ "status": status }));
    }

    /// The `done` event of a run that produced a file (D13.2).
    pub fn done_with(&mut self, status: &str, output_path: Option<&str>) {
        self.emit(
            "done",
            serde_json::json!({ "status": status, "output_path": output_path }),
        );
    }

    /// A `stage` event. `phase` is `begin` or `end`.
    pub fn stage(&mut self, name: &str, phase: &str) {
        self.emit("stage", serde_json::json!({ "name": name, "phase": phase }));
    }

    /// A `warning` event: a code and its arguments, never a sentence — the GUI localises.
    pub fn warning(&mut self, code: &str, severity: &str, args: serde_json::Value) {
        self.emit(
            "warning",
            serde_json::json!({ "code": code, "severity": severity, "args": args }),
        );
    }

    /// The `fatal` event: the run stops here.
    pub fn fatal(&mut self, code: &str, message: &str) {
        self.emit(
            "fatal",
            serde_json::json!({ "code": code, "message": message }),
        );
    }

    /// Emit one event with the common envelope.
    pub fn emit(&mut self, kind: &str, payload: serde_json::Value) {
        if !self.enabled {
            return;
        }
        let seq = self.seq.fetch_add(1, Ordering::SeqCst);
        let mut line = serde_json::json!({
            "v": PROTOCOL_VERSION,
            "t": kind,
            "seq": seq,
            "ts_ms": now_ms(),
        });
        if let (Some(object), Some(extra)) = (line.as_object_mut(), payload.as_object()) {
            for (key, value) in extra {
                object.insert(key.clone(), value.clone());
            }
        }

        let mut text = serde_json::to_string(&line).unwrap_or_default();
        if text.len() > MAX_EVENT_BYTES {
            // Truncating the payload keeps the envelope parseable, which is what a
            // line-oriented consumer needs most when something has gone wrong.
            text = serde_json::to_string(&serde_json::json!({
                "v": PROTOCOL_VERSION,
                "t": kind,
                "seq": seq,
                "ts_ms": now_ms(),
                "truncated": true,
                "message": TRUNCATION_NOTICE,
            }))
            .unwrap_or_default();
        }
        // A consumer that has gone away is not this process's problem to report: the run
        // continues and the exit code still says what happened.
        let _ = writeln!(self.writer, "{text}");
        let _ = self.writer.flush();
    }
}

/// Milliseconds since the Unix epoch, or zero if the clock is before it.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

/// Types serialised into an event payload must round-trip through `serde_json`.
pub fn payload<T: Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
}
