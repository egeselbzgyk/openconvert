//! Building semantic values out of the block view.
//!
//! One place, because two rules need the same thing — a list item and a note body are both
//! "some of a block's lines, as a paragraph" — and because `BlockId` minting has to be done
//! once, with one collision set, or two paragraphs in one book can share an id.

use std::collections::BTreeMap;

use oc_core::thresholds::Thresholds;
use oc_model::doc::{Align, Span, SpanStyle};
use oc_model::geom::Rect;
use oc_model::ids::{BlockId, NoteId};
use oc_model::layout::Para;
use oc_model::text::RunId;

use crate::view::LineView;

/// Which run on which page printed which note marker.
///
/// Keyed on the page as well as the run because `RunId` is page-local: `assemble_runs`
/// numbers runs from zero on every page, so run 7 exists once per page of the book
/// (`docs/DECISIONS_LOG.md`).
pub type NoteRefRuns = BTreeMap<(u32, RunId), NoteId>;

/// Mints block ids, refusing to hand out the same one twice.
///
/// `BlockId::derive` is a pure function of page, box and text, so two paragraphs that are
/// genuinely identical — two blank-looking list items, the same line in a table — derive the
/// same id. The collision suffix exists for exactly that (D13.3), and something has to
/// remember which suffixes are spent.
#[derive(Debug, Default)]
pub struct Minter {
    seen: std::collections::BTreeSet<String>,
}

impl Minter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mint(&mut self, page: u32, bbox: Rect, text: &str) -> BlockId {
        let base = BlockId::derive(page, bbox, text);
        let mut id = base;
        for suffix in 0..32u8 {
            id = base.with_collision_suffix(suffix);
            if self.seen.insert(id.as_str().to_owned()) {
                break;
            }
        }
        id
    }
}

/// Build a paragraph out of some of a block's lines.
///
/// The text is the lines joined by single spaces, which is the same flattening `layout` uses
/// — and it has to stay the same, because the conservation check across `structure` compares
/// the two multisets and a different join would read as a stage that changed the text.
pub fn para_of(
    minter: &mut Minter,
    page: u32,
    source: &[BlockId],
    lines: &[&LineView],
    noterefs: &NoteRefRuns,
    t: &Thresholds,
) -> Para {
    // Every character of every line, joined the way `layout` joins them. Nothing is stripped
    // — not even a list marker — because `structure` is Conserving and `Reason` has no
    // variant for text this stage chose to drop.
    let text = lines
        .iter()
        .map(|line| line.text.trim())
        .collect::<Vec<_>>()
        .join(" ");
    let spans = spans_of(page, lines, noterefs, t);
    debug_assert_eq!(
        oc_model::doc::spans_text(&spans),
        text,
        "the spans are the paragraph's text split, never a different text"
    );

    let bbox = lines
        .iter()
        .map(|line| line.bbox())
        .reduce(|a, b| Rect {
            x0: a.x0.min(b.x0),
            y0: a.y0.min(b.y0),
            x1: a.x1.max(b.x1),
            y1: a.y1.max(b.y1),
        })
        .unwrap_or(Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        });

    Para {
        id: minter.mint(page, bbox, &text),
        blocks: source.to_vec(),
        lines: lines.iter().map(|line| line.line.clone()).collect(),
        spans,
        text,
        first_line_indent: lines.first().is_some_and(|line| line.indent_pt() > 0.0),
        pages: (page, page),
        drop_cap: false,
        lang: None,
        align: Align::Left,
        confidence: None,
    }
}

/// Split a paragraph's text at every style change and at every note marker.
///
/// The one hard requirement is that `spans_text(&spans) == text`, because `Para` carries both
/// and the conservation check reads one of them: a span list that said anything other than the
/// paragraph's own text would make I-3 pass on a document that does not exist. Everything
/// below is therefore written to reproduce exactly the flattening `para_of` does — each line's
/// runs concatenated then trimmed, the lines joined by a single space — and to carry the style
/// and the note reference along with the characters rather than instead of them.
///
/// A note marker becomes a span of its own even when its style matches its neighbours', which
/// is what lets `epub` turn exactly that run into `<a epub:type="noteref">` and is why
/// `NoteRef` carries a `RunId` at all.
///
/// The key is `(page, RunId)` and not `RunId`. IR_SKETCH calls `RunId` a "per-document run
/// index", but `oc_text::words::assemble_runs` numbers runs from zero on every page, so run 7
/// exists once per page of the book. Keyed on the id alone, one page's marker silently
/// overwrites another's and a footnote loses its reference (`docs/DECISIONS_LOG.md`).
fn spans_of(page: u32, lines: &[&LineView], noterefs: &NoteRefRuns, t: &Thresholds) -> Vec<Span> {
    let mut pieces: Vec<Span> = Vec::new();

    for line in lines {
        let mut line_pieces: Vec<Span> = line
            .runs
            .iter()
            .map(|run| Span {
                text: run.text.clone(),
                style: SpanStyle {
                    bold: i64::from(run.weight) >= t.headings.bold_weight_min,
                    italic: run.italic,
                    smallcaps: false,
                    superscript: run.superscript,
                    subscript: run.subscript,
                    monospace: false,
                },
                noteref: noterefs.get(&(page, run.id)).copied(),
                link: None,
            })
            .collect();
        trim_edges(&mut line_pieces);
        line_pieces.retain(|piece| !piece.text.is_empty());
        if line_pieces.is_empty() {
            continue;
        }
        if !pieces.is_empty() {
            // The join `layout` uses, as a span of its own. It carries no style because it is
            // not text the book set: it is the boundary between two lines of one paragraph.
            pieces.push(Span::plain(" "));
        }
        pieces.extend(line_pieces);
    }

    merge_adjacent(pieces)
}

/// Strip the leading and trailing whitespace of a line, across run boundaries.
///
/// `LineView::text` is the runs concatenated and then trimmed, so a line whose first run is a
/// single space contributes nothing from it. Doing this piece by piece rather than on the
/// joined string is what keeps each surviving character attached to the style it was set in.
fn trim_edges(pieces: &mut [Span]) {
    for piece in pieces.iter_mut() {
        piece.text = piece.text.trim_start().to_owned();
        if !piece.text.is_empty() {
            break;
        }
    }
    for piece in pieces.iter_mut().rev() {
        piece.text = piece.text.trim_end().to_owned();
        if !piece.text.is_empty() {
            break;
        }
    }
}

/// Fold neighbouring spans that carry the same style and the same reference.
///
/// Runs are a property of how the producer drew the page — a justified line can be a dozen of
/// them — and emitting `<span>` per run would bury the text in markup that says nothing.
fn merge_adjacent(pieces: Vec<Span>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        match out.last_mut() {
            Some(last) if last.style == piece.style && last.noteref == piece.noteref => {
                last.text.push_str(&piece.text);
            }
            _ => out.push(piece),
        }
    }
    out
}

/// A paragraph whose text is already assembled, for the callers that joined it themselves.
///
/// `notes` is one: a note's body is the lines from its marker to the next marker, which may
/// be a suffix of one block's lines, and the text was built while they were being walked.
pub fn para_of_text(
    minter: &mut Minter,
    page: u32,
    source: &[BlockId],
    lines: Vec<oc_model::text::Line>,
    text: &str,
) -> Para {
    let bbox = lines
        .iter()
        .map(|line| line.bbox)
        .reduce(|a, b| Rect {
            x0: a.x0.min(b.x0),
            y0: a.y0.min(b.y0),
            x1: a.x1.max(b.x1),
            y1: a.y1.max(b.y1),
        })
        .unwrap_or(Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        });
    Para {
        id: minter.mint(page, bbox, text),
        blocks: source.to_vec(),
        first_line_indent: lines.first().is_some_and(|line| line.indent_pt > 0.0),
        lines,
        spans: vec![Span::plain(text.to_owned())],
        text: text.to_owned(),
        pages: (page, page),
        drop_cap: false,
        align: Align::Left,
        lang: None,
        confidence: None,
    }
}
