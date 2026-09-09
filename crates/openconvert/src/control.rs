//! The control channel: NDJSON on **stdin** (§2.3).
//!
//! stdout is data and stderr is events, so the one channel left for a supervisor to *say*
//! something is stdin. Two messages: `{"t":"cancel"}` sets the cancellation flag, and
//! `{"t":"ping"}` exists so a supervisor can tell "still working" from "hung" without
//! guessing from progress events.
//!
//! Read on its own thread, because the engine must keep working while it listens — a cancel
//! that is only noticed once the work finishes is not a cancel. The thread is detached and
//! never joined: it is blocked on a read that only ends when the supervisor closes stdin or
//! the process exits, and waiting for either would defeat the purpose.

use std::io::BufRead;

use oc_core::cancel::Cancel;

/// A control message from the supervisor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Cancel,
    Ping,
}

/// Parse one line of the control channel.
///
/// Unknown messages and malformed lines are `None` and are *ignored* rather than fatal. A
/// supervisor from a newer version of the UI may send a message this engine does not know, and
/// refusing to work because of it would turn a forward-compatible protocol into a brittle one.
pub fn parse(line: &str) -> Option<Control> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    match value.get("t")?.as_str()? {
        "cancel" => Some(Control::Cancel),
        "ping" => Some(Control::Ping),
        _ => None,
    }
}

/// Start listening on stdin, setting `cancel` when asked to.
///
/// Returns immediately. Does nothing useful if stdin is not connected to anything, which is
/// the normal case for a human at a terminal.
pub fn listen(cancel: Cancel) {
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        drain(stdin.lock(), &cancel);
    });
}

/// Read control messages until the channel ends or a cancel arrives.
///
/// Separated from [`listen`] so it can be tested against a byte slice. The alternative — a
/// test that spawns the real binary, writes to its stdin and hopes the flag is set before the
/// work finishes — is a race by construction: it would pass or fail on how quickly a machine
/// starts a process, which is not the property under test.
fn drain<R: BufRead>(reader: R, cancel: &Cancel) {
    for line in reader.lines() {
        let Ok(line) = line else {
            // The channel closed or went bad; there is nothing further to hear.
            return;
        };
        if parse(&line) == Some(Control::Cancel) {
            cancel.cancel();
            // Nothing after a cancel can change the answer, and the loop that matters is in
            // another thread waiting to see the flag.
            return;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2).
// ---------------------------------------------------------------------------

/// The two messages §2.3 defines, and the rule that anything else is ignored.
#[test]
fn control_messages_parse_and_unknown_ones_are_ignored() {
    assert_eq!(parse(r#"{"t":"cancel"}"#), Some(Control::Cancel));
    assert_eq!(parse(r#"{"t":"ping"}"#), Some(Control::Ping));

    // Extra fields are fine: a newer supervisor may send more than we read.
    assert_eq!(
        parse(r#"{"t":"cancel","job":"abc"}"#),
        Some(Control::Cancel)
    );

    // Unknown, malformed and empty are all "nothing to do", never an error. An engine that
    // died on an unrecognised control message would make the protocol impossible to extend.
    assert_eq!(parse(r#"{"t":"explode"}"#), None);
    assert_eq!(parse(r#"{"no-t":1}"#), None);
    assert_eq!(parse("not json at all"), None);
    assert_eq!(parse(""), None);
    // `t` has to be a string; a number is not a message name.
    assert_eq!(parse(r#"{"t":7}"#), None);
}

/// A cancel anywhere in the stream sets the flag, and nothing else does.
///
/// This is the half of test 1.19 that joins the control channel to the flag;
/// `cmd_dump_stage::cancel_is_observed_inside_page_loop` is the half that joins the flag to
/// the page loop. Both are deterministic, which a single end-to-end test through the process
/// could not be.
#[test]
fn a_cancel_on_the_control_channel_sets_the_flag() {
    let cancel = Cancel::new();
    drain(
        &br#"{"t":"ping"}
{"t":"cancel"}
"#[..],
        &cancel,
    );
    assert!(cancel.is_cancelled());
}

#[test]
fn other_control_traffic_does_not_cancel() {
    let cancel = Cancel::new();
    // A ping, an unknown message, a malformed line and a blank one: all survivable, none of
    // them a reason to stop working.
    drain(
        &br#"{"t":"ping"}
{"t":"from-a-newer-ui"}
not json

"#[..],
        &cancel,
    );
    assert!(!cancel.is_cancelled());
}

/// An empty channel — stdin closed immediately, which is what happens when the engine is run
/// from a script — is not a cancellation.
#[test]
fn a_closed_control_channel_does_not_cancel() {
    let cancel = Cancel::new();
    drain(&b""[..], &cancel);
    assert!(!cancel.is_cancelled());
}
