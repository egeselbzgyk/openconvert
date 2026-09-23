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
        self.hello_with(engine_version, ir_version, pdfium_version, &[]);
    }

    /// The `hello` event, with capabilities this run discovered beyond the ones every engine has —
    /// `ocr:tesseract-5.3.4` when a usable Tesseract was found, and nothing when it was not
    /// (PHASE 13 detail 1). A supervisor reads OCR support here rather than by trying it.
    pub fn hello_with(
        &mut self,
        engine_version: &str,
        ir_version: u32,
        pdfium_version: &str,
        extra: &[String],
    ) {
        let mut capabilities = vec!["inspect".to_owned()];
        capabilities.extend(extra.iter().cloned());
        self.emit(
            "hello",
            serde_json::json!({
                "engine_version": engine_version,
                "ir_version": ir_version,
                "protocol": PROTOCOL_VERSION,
                "pdfium_version": pdfium_version,
                "capabilities": capabilities,
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
    ///
    /// **Always reaches the user, whatever `--progress` says.** Every other event on this
    /// sink is telemetry a caller may decline; a fatal is the program's answer to what it
    /// was asked to do, and a process that exits non-zero having printed nothing at all has
    /// not answered. Nine corpus documents failed in under a second with a precise message
    /// — "I-1: stage text does not balance: 14 characters left and 14 appeared" — and every
    /// one of them looked, from outside, like a hang. The class they were filed under did
    /// not exist (PHASE 7.5).
    ///
    /// When the NDJSON channel is off the same code and message go out as one human line,
    /// on the same stream, because stderr is not the NDJSON channel then and is free to
    /// carry prose. That is the rule `--locale` already follows for warnings.
    pub fn fatal(&mut self, code: &str, message: &str) {
        if self.enabled {
            self.emit(
                "fatal",
                serde_json::json!({ "code": code, "message": message }),
            );
            return;
        }
        // Deliberately not through `emit`: this is not an event, and it must not be
        // numbered, truncated at `MAX_EVENT_BYTES`, or silenced by the `enabled` flag that
        // is the whole reason it was invisible.
        let _ = writeln!(self.writer, "error [{code}]: {message}");
        let _ = self.writer.flush();
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

#[cfg(test)]
mod tests {
    use super::*;

    /// PHASE 7.5. A process that exits non-zero having printed nothing has not answered the
    /// question it was asked.
    ///
    /// Nine corpus documents were filed as a "timeout class" on exactly this evidence: an
    /// empty output file and a non-zero exit. Every one of them had failed in under a second
    /// with a precise message, and the message was discarded because `--progress json` was
    /// not passed. The class did not exist; the silence invented it.
    #[test]
    fn a_fatal_reaches_the_user_without_progress_json() {
        let mut out = Vec::new();
        EventSink::new(&mut out, false).fatal("E_PDF", "stage text does not balance");

        let text = String::from_utf8(out).expect("the sink writes UTF-8");
        assert!(text.contains("E_PDF"), "the code is named: {text:?}");
        assert!(
            text.contains("stage text does not balance"),
            "the message survives: {text:?}"
        );
    }

    /// The other half: with the NDJSON channel on, a fatal is an event and not prose, because
    /// the GUI localises and cannot parse a sentence.
    #[test]
    fn a_fatal_is_an_ndjson_event_when_the_channel_is_on() {
        let mut out = Vec::new();
        EventSink::new(&mut out, true).fatal("E_PDF", "stage text does not balance");

        let text = String::from_utf8(out).expect("the sink writes UTF-8");
        let event: serde_json::Value =
            serde_json::from_str(text.trim()).expect("one JSON object per line");
        assert_eq!(event["t"], "fatal");
        assert_eq!(event["code"], "E_PDF");
        assert_eq!(event["v"], PROTOCOL_VERSION);
    }

    /// Telemetry stays optional. A `stage` event with the channel off writes nothing — the
    /// exemption is for the fatal alone, not a licence for every event to print.
    #[test]
    fn an_ordinary_event_stays_silent_when_the_channel_is_off() {
        let mut out = Vec::new();
        let mut sink = EventSink::new(&mut out, false);
        sink.stage("text", "begin");
        sink.done("ok");

        assert!(out.is_empty(), "no telemetry without --progress json");
    }
}
