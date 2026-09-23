//! The NDJSON event protocol the engine speaks on **stderr** (D13.2, §2.3).
//!
//! One UTF-8 JSON object per line. stdout is the data channel and carries nothing else, so
//! `--progress json` and `--dump-stage -` can be used together without interleaving.
//!
//! Events are a code-and-arguments protocol, never prose: the GUI localises. That is why
//! `warning` and `fatal` carry a `code` and an `args` object rather than a sentence.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

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
/// dropped line. The counter is atomic and the writer is behind a lock because events come from
/// more than one thread: the heartbeat has a thread of its own (§2.3), and progress will be
/// reported from `rayon` workers once per-page parallelism arrives. Numbering and writing happen
/// under the same lock, so the lines appear in `seq` order.
pub struct EventSink<W: Write> {
    writer: Mutex<W>,
    seq: AtomicU64,
    enabled: bool,
}

impl<W: Write> EventSink<W> {
    /// A sink that writes events. `enabled` is `--progress json`: when false every emit is
    /// a no-op, so call sites do not have to ask.
    pub fn new(writer: W, enabled: bool) -> Self {
        Self {
            writer: Mutex::new(writer),
            seq: AtomicU64::new(0),
            enabled,
        }
    }

    /// Whether events are written at all (`--progress json`, or a job spec).
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The `hello` event. Always the first line (§2.3).
    pub fn hello(&self, engine_version: &str, ir_version: u32, pdfium_version: &str) {
        self.hello_with(engine_version, ir_version, pdfium_version, &[]);
    }

    /// The `hello` event, with capabilities this run discovered beyond the ones every engine has —
    /// `ocr:tesseract-5.3.4` when a usable Tesseract was found, and nothing when it was not
    /// (PHASE 13 detail 1). A supervisor reads OCR support here rather than by trying it.
    pub fn hello_with(
        &self,
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
    pub fn done(&self, status: &str) {
        self.emit("done", serde_json::json!({ "status": status }));
    }

    /// The `done` event of a run that produced a file (D13.2).
    pub fn done_with(&self, status: &str, output_path: Option<&str>) {
        self.emit(
            "done",
            serde_json::json!({ "status": status, "output_path": output_path }),
        );
    }

    /// The `done` event of a conversion: its status, where the report is, and the output when
    /// one was written (§2.3). Bulk data never crosses the pipe; its paths do.
    pub fn done_job(&self, status: &str, report_path: &str, output_path: Option<&str>) {
        self.emit(
            "done",
            serde_json::json!({
                "status": status,
                "report_path": report_path,
                "output_path": output_path,
            }),
        );
    }

    /// A `stage` event. `phase` is `begin` or `end`.
    pub fn stage(&self, name: &str, phase: &str) {
        self.emit("stage", serde_json::json!({ "name": name, "phase": phase }));
    }

    /// A `warning` event: a code and its arguments, never a sentence — the GUI localises.
    pub fn warning(&self, code: &str, severity: &str, args: serde_json::Value) {
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
    pub fn fatal(&self, code: &str, message: &str) {
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
        let mut writer = self.lock();
        let _ = writeln!(writer, "error [{code}]: {message}");
        let _ = writer.flush();
    }

    /// A `stage` event that ends a stage, with its wall-clock (§2.3's `elapsed_ms`).
    pub fn stage_end(&self, name: &str, elapsed_ms: u64) {
        self.emit(
            "stage",
            serde_json::json!({ "name": name, "phase": "end", "elapsed_ms": elapsed_ms }),
        );
    }

    /// A `progress` event: `done` of `total` `unit`s of `stage` (§2.3).
    pub fn progress(&self, stage: &str, done: u32, total: u32, unit: &str) {
        self.emit(
            "progress",
            serde_json::json!({ "stage": stage, "done": done, "total": total, "unit": unit }),
        );
    }

    /// A `heartbeat` event: "still alive", and nothing else (§2.3).
    pub fn heartbeat(&self) {
        self.emit("heartbeat", serde_json::json!({}));
    }

    /// The writer, recovering it from a thread that panicked while holding it.
    ///
    /// A poisoned lock means some thread died mid-line; the line may be torn, but refusing to
    /// write the `fatal` or `done` that explains the death would be worse than a torn line.
    fn lock(&self) -> std::sync::MutexGuard<'_, W> {
        self.writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Emit one event with the common envelope.
    pub fn emit(&self, kind: &str, payload: serde_json::Value) {
        if !self.enabled {
            return;
        }
        // Numbered under the writer's lock, so `seq` order is line order.
        let mut writer = self.lock();
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
        let _ = writeln!(writer, "{text}");
        let _ = writer.flush();
    }
}

impl<W: Write + Send> EventSink<W> {
    /// Run `work` while a heartbeat goes out every `interval` (§2.3, RT C2).
    ///
    /// The heartbeat is what lets a supervisor tell a slow stage from a hung process: a stage with
    /// no natural count says nothing for as long as it runs, and silence is only a signal if
    /// something is guaranteed to break it. It runs on its own thread for exactly that reason — a
    /// heartbeat sent from the working thread would stop when the work hung, which is when it is
    /// needed.
    ///
    /// **It stops before this returns**, so the caller's `done` or `fatal` is always the last
    /// line. Stopping does not wait out the interval: the thread sleeps on a channel, and the
    /// channel closing wakes it at once.
    pub fn with_heartbeat<R>(&self, interval: Duration, work: impl FnOnce() -> R) -> R {
        if !self.enabled {
            return work();
        }
        let (stop, stopped) = std::sync::mpsc::channel::<()>();
        std::thread::scope(|scope| {
            scope.spawn(move || {
                while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) =
                    stopped.recv_timeout(interval)
                {
                    self.heartbeat();
                }
            });
            let result = work();
            drop(stop);
            result
        })
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

    /// RT C2: a heartbeat goes out while the work runs, and none after it returns, so `done`
    /// stays the last line.
    #[test]
    fn the_heartbeat_beats_while_work_runs_and_stops_before_it_returns() {
        let sink = EventSink::new(Vec::new(), true);
        let answer = sink.with_heartbeat(Duration::from_millis(10), || {
            std::thread::sleep(Duration::from_millis(80));
            42
        });
        sink.done("ok");
        assert_eq!(answer, 42);

        let written = sink.writer.into_inner().expect("not poisoned");
        let lines: Vec<serde_json::Value> = String::from_utf8(written)
            .expect("UTF-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("JSON"))
            .collect();
        let beats = lines.iter().filter(|l| l["t"] == "heartbeat").count();
        assert!(
            beats >= 2,
            "{beats} heartbeats in 80 ms at a 10 ms interval"
        );
        assert_eq!(lines.last().expect("lines")["t"], "done", "done is last");
        for (expected, line) in lines.iter().enumerate() {
            assert_eq!(
                line["seq"], expected as u64,
                "seq is line order across threads"
            );
        }
    }

    /// No channel, no thread: a silent sink's heartbeat is free.
    #[test]
    fn a_silent_sink_runs_the_work_without_a_heartbeat() {
        let sink = EventSink::new(Vec::new(), false);
        assert_eq!(sink.with_heartbeat(Duration::from_millis(1), || 7), 7);
        assert!(sink.writer.into_inner().expect("not poisoned").is_empty());
    }

    /// Telemetry stays optional. A `stage` event with the channel off writes nothing — the
    /// exemption is for the fatal alone, not a licence for every event to print.
    #[test]
    fn an_ordinary_event_stays_silent_when_the_channel_is_off() {
        let mut out = Vec::new();
        let sink = EventSink::new(&mut out, false);
        sink.stage("text", "begin");
        sink.done("ok");

        assert!(out.is_empty(), "no telemetry without --progress json");
    }
}
