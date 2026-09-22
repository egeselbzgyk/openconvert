//! Gate L: the edit is `Conserving` — labels, levels, roles and CSS classes only (D13.4 I-3,
//! D13.5, ARCHITECTURE §6.2).
//!
//! This is the gate that makes the line between the model and the deterministic pipeline a
//! property of the code rather than of anyone's care. The one question that draws the line — *if
//! this is answered wrongly, does the book lose or gain a character?* (`docs/DECISIONS_LOG.md`,
//! 2026-09-20) — is asked here of every edit, after the fact, whatever the edit was.
//!
//! Two checks, the second strictly stronger:
//!
//! 1. **`C` is unchanged** — the multiset of the book's non-whitespace characters after canonical
//!    decomposition, computed by the same function every stage's conservation check uses
//!    (`oc_model::ledger::c_of_parts`). Compared *without* the ledger: an edit that deleted a
//!    running head and wrote a `RunningHeader` entry for it would balance I-1 and is still a
//!    deletion the model had no authority to make (label authority is not deletion authority,
//!    D13.5).
//! 2. **The text reads in the same order** — the book's non-whitespace characters, in the order
//!    `Document::text_pieces` reads them, are the same sequence. A rename cannot reorder text, move
//!    it into another block, or re-encode it in another canonical form; a reading-order change is
//!    a deterministic stage's decision, never a task's.
//!
//! Whitespace is outside both, as it is outside `C`: making a paragraph verse breaks it into lines,
//! and a line break is a name for where one line ends.

use oc_model::document::Document;
use oc_model::ledger::c_of_parts;

use super::GateFailure;

/// Gate L, over the book before and after an edit.
pub fn gate_locality(before: &Document, after: &Document) -> Result<(), GateFailure> {
    let before_pieces = before.text_pieces();
    let after_pieces = after.text_pieces();

    let c_before = c_of_parts(before_pieces.iter().map(String::as_str));
    let c_after = c_of_parts(after_pieces.iter().map(String::as_str));
    if c_before != c_after {
        return Err(GateFailure::CharactersChanged {
            lost: c_before.difference(&c_after).total(),
            gained: c_after.difference(&c_before).total(),
        });
    }

    if !reading(&before_pieces).eq(reading(&after_pieces)) {
        return Err(GateFailure::TextReordered);
    }
    Ok(())
}

/// The characters of the book in reading order, whitespace left out.
fn reading(pieces: &[String]) -> impl Iterator<Item = char> + '_ {
    pieces
        .iter()
        .flat_map(|piece| piece.chars())
        .filter(|ch| !ch.is_whitespace())
}
