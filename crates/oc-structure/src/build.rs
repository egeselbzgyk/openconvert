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
    let text = join_lines(lines.iter().copied());
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
/// paragraph's own text would make I-3 pass on a document that does not exist.
///
/// So the text is not re-derived from the runs — it is **split**. The line's own text is walked
/// once, each run's text is located in it in order, and whatever lies between two runs is
/// emitted as a plain span. The concatenation of the pieces is the line's text by construction,
/// whatever join produced it.
///
/// That last clause is the whole reason for the alignment. `LayoutLine::text` is the runs
/// concatenated for an ordinary line and the runs joined *by a single space* for one that
/// `split_lines_at_gutters` rebuilt from segments — so a table row in a two-column book reaches
/// here with spaces the runs do not contain. Reproducing one join was wrong on the other, and
/// which one a line took is not knowable from here. Found by the first real book this was run
/// on, at page 42 of *AI Engineering*.
///
/// A run whose text cannot be located leaves the rest of the line as one plain span: the text
/// is still exact and only the styling is lost, which is the right way round for this to fail.
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
    let mut glued = false;

    for line in lines {
        let text = line.text.trim();
        if text.is_empty() {
            continue;
        }
        if !pieces.is_empty() && !glued {
            // The join `para_of` uses between lines, as a span of its own. It carries no style
            // because it is not text the book set: it is the boundary between two lines.
            pieces.push(Span::plain(" "));
        }
        pieces.extend(align(page, text, &line.runs, noterefs, t));
        glued = line.glue;
    }

    merge_adjacent(pieces)
}

/// Lines joined as a reader reads them: by a single space, except after a line whose broken
/// word `paragraphs` joined, which runs straight on into the next.
///
/// The one join every paragraph in `structure` is built with, so the text a paragraph carries
/// and the spans it is split into cannot disagree about a line boundary.
pub fn join_lines<'a>(lines: impl IntoIterator<Item = &'a LineView>) -> String {
    let mut text = String::new();
    let mut glued = false;
    for line in lines {
        let piece = line.text.trim();
        if piece.is_empty() {
            continue;
        }
        if !text.is_empty() && !glued {
            text.push(' ');
        }
        text.push_str(piece);
        glued = line.glue;
    }
    text
}

/// Carry a paragraph on with the next piece of it: the rest of a paragraph a page or a column
/// broke off, as `paragraphs` decided. `glued` is whether the piece before ended on a word
/// `paragraphs` joined across the break, which then runs on without a space.
pub fn continue_para(para: &mut Para, next: Para, glued: bool) {
    let mut spans = std::mem::take(&mut para.spans);
    if !glued {
        spans.push(Span::plain(" "));
        para.text.push(' ');
    }
    para.text.push_str(&next.text);
    spans.extend(next.spans);
    para.spans = merge_adjacent(spans);
    para.blocks.extend(next.blocks);
    para.lines.extend(next.lines);
    para.pages.1 = para.pages.1.max(next.pages.1);
}

/// Make the first `prefix` bytes of a paragraph's text a link to `target`: a printed contents
/// entry's title, pointed at the heading it names. The text is untouched; only its spans are
/// cut at the prefix's end, and a note reference is never folded into a link.
pub fn link_prefix(para: &mut Para, prefix: usize, target: BlockId) {
    let mut out: Vec<Span> = Vec::new();
    let mut at = 0usize;
    for span in std::mem::take(&mut para.spans) {
        let end = at + span.text.len();
        if at >= prefix || span.noteref.is_some() {
            out.push(span);
        } else if end <= prefix {
            out.push(Span {
                link: Some(oc_model::doc::LinkTarget::Internal(target)),
                ..span
            });
        } else {
            let cut = prefix - at;
            if span.text.is_char_boundary(cut) {
                let (head, tail) = span.text.split_at(cut);
                out.push(Span {
                    text: head.to_owned(),
                    link: Some(oc_model::doc::LinkTarget::Internal(target)),
                    ..span.clone()
                });
                out.push(Span {
                    text: tail.to_owned(),
                    ..span
                });
            } else {
                out.push(span);
            }
        }
        at = end;
    }
    para.spans = merge_adjacent(out);
}

/// The hyphens a line break is made with, which `paragraphs` may have taken off a line's text
/// while the run that drew the line still has it.
const BREAK_HYPHENS: [char; 2] = ['\u{002D}', '\u{2010}'];

/// Split one line's text at its runs, keeping every character.
fn align(
    page: u32,
    text: &str,
    runs: &[oc_model::text::Run],
    noterefs: &NoteRefRuns,
    t: &Thresholds,
) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    let mut rest = text;

    for run in runs {
        let needle = run.text.trim();
        if needle.is_empty() {
            continue;
        }
        // A line's last run may still end on the hyphen `paragraphs` took off the line.
        let found = rest.find(needle).map(|at| (at, needle)).or_else(|| {
            let bare = needle.trim_end_matches(BREAK_HYPHENS);
            (bare.len() < needle.len() && !bare.is_empty())
                .then(|| rest.find(bare).map(|at| (at, bare)))
                .flatten()
        });
        let Some((at, needle)) = found else {
            // Out of step. Everything left goes out as plain text rather than being dropped or
            // re-derived, because the characters matter and the styling does not.
            break;
        };
        if at > 0 {
            out.push(Span::plain(&rest[..at]));
        }
        out.push(Span {
            text: needle.to_owned(),
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
        });
        rest = &rest[at + needle.len()..];
    }

    if !rest.is_empty() {
        out.push(Span::plain(rest));
    }
    out
}

/// Fold neighbouring spans that carry the same style and the same reference.
///
/// Runs are a property of how the producer drew the page — a justified line can be a dozen of
/// them — and emitting `<span>` per run would bury the text in markup that says nothing.
fn merge_adjacent(pieces: Vec<Span>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        match out.last_mut() {
            Some(last)
                if last.style == piece.style
                    && last.noteref == piece.noteref
                    && last.link == piece.link =>
            {
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

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2)
// ---------------------------------------------------------------------------

#[cfg(test)]
fn run_of(id: u32, text: &str, weight: u16) -> oc_model::text::Run {
    oc_model::text::Run {
        id: RunId(id),
        page: oc_model::extract::PageRef::new(0),
        text: text.to_owned(),
        bbox: Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        },
        baseline_y: 0.0,
        font: oc_model::extract::FontId(0),
        size_pt: 10.0,
        weight,
        italic: false,
        superscript: false,
        subscript: false,
        provenance: oc_model::text::TextProvenance::Pdf,
        glyph_range: (0, 0),
    }
}

/// The alignment exists because a line's text and its runs do not always agree about spacing:
/// `LayoutLine::text` is a bare concatenation for an ordinary line and a space-joined one for a
/// line `split_lines_at_gutters` rebuilt from segments. Splitting the text rather than
/// re-deriving it is what makes `spans_text(&spans) == text` hold under both.
#[test]
fn spans_split_the_lines_own_text_whatever_join_produced_it() {
    let thresholds = &oc_core::thresholds::T;
    let noterefs = NoteRefRuns::new();
    let runs = vec![
        run_of(0, "Llama 2-7B", 400),
        run_of(1, "32", 400),
        run_of(2, "4,096", 400),
    ];

    // The gutter-split shape: the runs joined by a space.
    let joined = "Llama 2-7B 32 4,096";
    let spans = align(0, joined, &runs, &noterefs, thresholds);
    assert_eq!(oc_model::doc::spans_text(&spans), joined);

    // The ordinary shape: the runs concatenated.
    let packed = "Llama 2-7B324,096";
    let spans = align(0, packed, &runs, &noterefs, thresholds);
    assert_eq!(oc_model::doc::spans_text(&spans), packed);
}

/// A run the line's text does not contain costs the styling of what follows it and not one
/// character of the text. That is the right way round: a paragraph that reads correctly in the
/// wrong face is a blemish, and a paragraph missing three words is a defect.
#[test]
fn a_run_that_cannot_be_located_costs_styling_and_never_text() {
    let thresholds = &oc_core::thresholds::T;
    let noterefs = NoteRefRuns::new();
    let runs = vec![run_of(0, "present", 400), run_of(1, "absent", 400)];

    let text = "present and then some";
    let spans = align(0, text, &runs, &noterefs, thresholds);
    assert_eq!(oc_model::doc::spans_text(&spans), text);
}

/// Bold survives the split, and neighbouring spans in one style become one span: a justified
/// line can be a dozen runs, and a `<strong>` per run would bury the text in markup.
#[test]
fn a_style_change_splits_the_text_and_a_repeat_of_one_style_does_not() {
    let thresholds = &oc_core::thresholds::T;
    let noterefs = NoteRefRuns::new();
    let bold = u16::try_from(thresholds.headings.bold_weight_min).unwrap_or(700);
    let runs = vec![
        run_of(0, "plain", 400),
        run_of(1, "BOLD", bold),
        run_of(2, "ALSO", bold),
    ];

    let text = "plain BOLD ALSO";
    let spans = merge_adjacent(align(0, text, &runs, &noterefs, thresholds));
    assert_eq!(oc_model::doc::spans_text(&spans), text);
    assert_eq!(
        spans
            .iter()
            .map(|span| (span.text.clone(), span.style.bold))
            .collect::<Vec<_>>(),
        vec![
            ("plain ".to_owned(), false),
            ("BOLD".to_owned(), true),
            (" ".to_owned(), false),
            ("ALSO".to_owned(), true),
        ]
    );
}
