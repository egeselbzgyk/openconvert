//! Footnotes, and the bijection that is the whole point of them (PIPELINE §8.3).
//!
//! Footnote handling is widely acknowledged as the hardest part of PDF→EPUB, and the
//! research is explicit about where the value is: **getting the linkage right matters more
//! than getting the classification right** (R2 §B.6). A note misfiled as a paragraph is a
//! cosmetic defect; a `noteref` pointing at nothing is EPUBCheck `RSC-007`, and a note
//! nothing points at is orphaned content a reader never reaches. Calibre has no footnote
//! handling at all (R1 §C.2 #4), so this is a place where doing the obvious thing is already
//! ahead of the field.
//!
//! The requirement is therefore stated as a **bijection**: every marker resolves to exactly
//! one note and every note is referenced exactly once. It is cheap, it is total, and
//! `NoteLinkStats.match_rate` reports it as a number rather than as a hope.

use oc_core::thresholds::Thresholds;
use oc_model::confidence::{Confidence, Signal};
use oc_model::doc::{Note, NoteKind, Severity, Warning};
use oc_model::extract::VectorRegion;
use oc_model::ids::{BlockId, NoteId};
use serde::Serialize;

use crate::view::BlockView;

/// A marker and a note could not be paired (PIPELINE §8.3, "the bijection is asserted").
pub const W_NOTE_UNMATCHED: &str = "W_NOTE_UNMATCHED";

/// The symbol cycle, which **resets on every page** (PIPELINE §8.3). That reset is why a
/// symbol may only ever be matched within its own page: the `*` on page 12 and the `*` on
/// page 13 are two different notes, and a matcher keying on symbol equality across the book
/// links both bodies to the first and orphans the second.
const SYMBOL_CYCLE: [&str; 6] = ["*", "\u{2020}", "\u{2021}", "§", "\u{2016}", "¶"];

/// How well the markers and the notes paired up.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NoteLinkStats {
    pub markers: u32,
    pub notes: u32,
    pub matched: u32,
    /// `matched / max(markers, notes)`. One exactly when the pairing is a bijection; the
    /// denominator is the larger side so that an unreferenced note counts against it just as
    /// an unresolved marker does.
    pub match_rate: f32,
    /// How many notes were found under a separator rule. A confidence signal, never a
    /// requirement: plenty of books draw no rule at all.
    pub separator_rules: u32,
    /// Characters carried by an *unmarked* note: note-zone text that arrived while no note
    /// was open, because its marker was not one the rule recognises. They used to be dropped;
    /// now they are kept, and this says how much of a book's note text had no marker read
    /// (PHASE 7.5, `structure/lost/unattached-note-text`).
    pub unmarked_chars: u32,
    pub warnings: Vec<Warning>,
}

/// Where a note reference sits in the body.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NoteRef {
    pub note: NoteId,
    /// The body block the marker is in.
    pub block: BlockId,
    /// The run within that block, by its id, so the stage can turn exactly that run into a
    /// `Span` carrying the `noteref`.
    pub run: oc_model::text::RunId,
    pub marker: String,
    pub page: u32,
}

/// Link the body's markers to the notes at the foot of the page.
pub fn link_notes(
    blocks: &[BlockView],
    rules: &[VectorRegion],
    body_size_pt: f32,
    t: &Thresholds,
) -> (Vec<Note>, Vec<NoteRef>, NoteLinkStats) {
    let mut minter = crate::build::Minter::new();
    let zone: Vec<&BlockView> = blocks
        .iter()
        .filter(|block| is_note_block(block, body_size_pt, t))
        .collect();
    let zone_ids: Vec<BlockId> = zone.iter().map(|block| block.id).collect();

    let markers = body_markers(blocks, &zone_ids);
    let (mut notes, unmarked_chars) = note_bodies(&zone);
    let separator_rules = notes
        .iter()
        .filter(|note| has_separator_rule(note, blocks, rules, body_size_pt, t))
        .count();

    // Pass 1: exact symbol equality, on the same page. This is the pass that does all the
    // work, and the page restriction is what makes the symbol cycle safe.
    let mut taken = vec![false; markers.len()];
    for note in &mut notes {
        note.marker_index = markers.iter().enumerate().position(|(index, marker)| {
            !taken[index] && marker.page == note.page && marker.text == note.marker
        });
        if let Some(index) = note.marker_index {
            taken[index] = true;
        }
    }
    // Pass 2: exact equality anywhere, for numbered notes whose reference is on another page
    // — a note carried over to the foot of the next page, which is common in dense settings.
    // Symbols are excluded by construction: the cycle resets, so equality across pages is
    // meaningless for them.
    for note in &mut notes {
        if note.marker_index.is_some() || is_symbol(&note.marker) {
            continue;
        }
        note.marker_index = markers.iter().enumerate().position(|(index, marker)| {
            !taken[index] && marker.text == note.marker && !is_symbol(&marker.text)
        });
        if let Some(index) = note.marker_index {
            taken[index] = true;
        }
    }
    // Pass 3: order within the page. The last resort PIPELINE §8.3 names, for the case where
    // the marker's glyph and the note's differ — a superscript `1` against a `1.` at the foot.
    for note in &mut notes {
        if note.marker_index.is_some() {
            continue;
        }
        note.marker_index = markers
            .iter()
            .enumerate()
            .position(|(index, marker)| !taken[index] && marker.page == note.page);
        if let Some(index) = note.marker_index {
            taken[index] = true;
        }
    }

    let matched = notes
        .iter()
        .filter(|note| note.marker_index.is_some())
        .count();
    let denominator = markers.len().max(notes.len());
    let match_rate = if denominator == 0 {
        // No markers and no notes is a bijection between two empty sets, and a book without
        // footnotes must not be reported as a book whose footnotes all failed.
        1.0
    } else {
        (matched as f64 / denominator as f64) as f32
    };

    let mut warnings = Vec::new();
    if match_rate < 1.0 {
        warnings.push(
            Warning::new(W_NOTE_UNMATCHED, Severity::Warn)
                .with_arg("markers", markers.len().to_string())
                .with_arg("notes", notes.len().to_string())
                .with_arg("matched", matched.to_string()),
        );
    }

    let signals = vec![
        Signal::new("note_match_rate", match_rate),
        Signal::new("note_count", notes.len() as f32),
        Signal::new("separator_rules", separator_rules as f32),
    ];

    let mut built = Vec::with_capacity(notes.len());
    let mut references = Vec::new();
    for (index, note) in notes.iter().enumerate() {
        let id = NoteId(u32::try_from(index).unwrap_or(u32::MAX));
        let anchor = note
            .marker_index
            .and_then(|index| markers.get(index))
            .map(|marker| marker.block);
        if let Some(marker) = note.marker_index.and_then(|index| markers.get(index)) {
            references.push(NoteRef {
                note: id,
                block: marker.block,
                run: marker.run,
                marker: marker.text.clone(),
                page: marker.page,
            });
        }
        built.push(Note {
            id,
            kind: NoteKind::Footnote,
            marker: note.marker.clone(),
            // The note's own text, marker and all. `structure` is Conserving: the marker is
            // part of what the page printed, and the note is where those characters live once
            // the block they were in has left the flow.
            body: vec![oc_model::doc::Content::Paragraph(
                crate::build::para_of_text(
                    &mut minter,
                    note.page,
                    &[note.block],
                    note.lines.clone(),
                    &note.text,
                ),
            )],
            anchor,
            page: oc_model::extract::PageRef::new(note.page),
            confidence: if anchor.is_some() {
                Confidence::deterministic(signals.clone())
            } else {
                Confidence::fallback(signals.clone())
            },
        });
    }

    (
        built,
        references,
        NoteLinkStats {
            markers: u32::try_from(markers.len()).unwrap_or(u32::MAX),
            notes: u32::try_from(notes.len()).unwrap_or(u32::MAX),
            matched: u32::try_from(matched).unwrap_or(u32::MAX),
            match_rate,
            separator_rules: u32::try_from(separator_rules).unwrap_or(u32::MAX),
            unmarked_chars,
            warnings,
        },
    )
}

/// One note found at the foot of a page, before it has an id.
struct Found {
    marker: String,
    page: u32,
    block: BlockId,
    /// The line of its block the note starts on, for the separator-rule test.
    top_y: f32,
    text: String,
    /// The lines the note was printed on, so its body is a paragraph with real geometry.
    lines: Vec<oc_model::text::Line>,
    marker_index: Option<usize>,
}

/// One marker found in the body flow.
struct Marker {
    text: String,
    block: BlockId,
    run: oc_model::text::RunId,
    page: u32,
}

/// Whether a block is in the note zone: near the foot of its page and set small.
///
/// Both conditions, and neither is sufficient. A page number is in the band and set small; a
/// caption is set small and is not in the band. What separates a note from the page number is
/// the third test, applied where the notes are split out: a note *starts with a marker*.
fn is_note_block(block: &BlockView, body_size_pt: f32, t: &Thresholds) -> bool {
    if body_size_pt <= 0.0 {
        return false;
    }
    let small = f64::from(block.size_pt() / body_size_pt) < t.footnote.font_size_ratio_max;
    let low = f64::from(block.band()) >= t.footnote.zone_band_min;
    small
        && low
        && block
            .lines
            .iter()
            .any(|line| leading_marker(line).is_some())
}

/// The marker a note-zone line opens with, if it opens with one.
///
/// Two forms, because producers set note entries two ways. Typst and most professional
/// typesetting raise the entry's marker exactly as the body's is raised, so the first *run*
/// of the line is a superscript marker on its own. Plenty of other books simply print
/// `"* First note."` at the note's own size, and there the marker is the first token of the
/// line's text.
///
/// The second form is deliberately narrower than the first. A superscript run that is wholly
/// a digit is a marker wherever it appears; a line merely *beginning* with digits is not —
/// `"12 point leading"` would be note twelve — so the unraised form admits only the symbol
/// cycle and a digit run closed by `.` or `)`, which is how an unraised marker is always set.
fn leading_marker(line: &crate::view::LineView) -> Option<String> {
    if let Some(run) = line.runs.first() {
        let text = run.text.trim();
        if run.superscript && is_marker_text(text) {
            return Some(text.to_owned());
        }
    }

    let text = line.text.trim_start();
    let token = text.split_whitespace().next()?;
    if is_symbol(token) {
        return Some(token.to_owned());
    }
    let digits = token.trim_end_matches(['.', ')']);
    (digits.len() < token.len() && !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
        .then(|| digits.to_owned())
}

fn is_marker_text(text: &str) -> bool {
    !text.is_empty() && (text.chars().all(|c| c.is_ascii_digit()) || is_symbol(text))
}

fn is_symbol(text: &str) -> bool {
    SYMBOL_CYCLE.contains(&text)
}

/// Split the zone's blocks into notes, one per line that opens with a marker.
fn note_bodies(zone: &[&BlockView]) -> (Vec<Found>, u32) {
    let mut notes: Vec<Found> = Vec::new();
    for block in zone {
        for line in &block.lines {
            match leading_marker(line) {
                Some(marker) => notes.push(Found {
                    marker,
                    page: block.page,
                    block: block.id,
                    top_y: line.bbox().y0,
                    text: line.text.trim().to_owned(),
                    lines: vec![line.line.clone()],
                    marker_index: None,
                }),
                // A continuation line belongs to the note above it.
                None => {
                    if let Some(open) = notes.last_mut() {
                        open.text.push(' ');
                        open.text.push_str(line.text.trim());
                        open.lines.push(line.line.clone());
                    } else {
                        // Nothing is open, so the line opens a note of its own, with no marker.
                        // It used to be dropped here — while its block, claimed through the
                        // notes opening later in it, left the flow — which is where the 644
                        // characters on page 42 went (PHASE 7.5,
                        // `structure/lost/unattached-note-text`). An unmarked note still reaches
                        // the book: `epub` emits an unreferenced note as an aside, and the
                        // linker's order-within-the-page pass may still pair it with its marker.
                        notes.push(Found {
                            marker: String::new(),
                            page: block.page,
                            block: block.id,
                            top_y: line.bbox().y0,
                            text: line.text.trim().to_owned(),
                            lines: vec![line.line.clone()],
                            marker_index: None,
                        });
                    }
                }
            }
        }
    }
    // Counted once the notes are complete, so the continuation lines an unmarked note went on
    // to collect are counted with it, not only the line that opened it.
    let unmarked = notes
        .iter()
        .filter(|note| note.marker.is_empty())
        .map(|note| note.text.chars().filter(|ch| !ch.is_whitespace()).count())
        .sum::<usize>();
    (notes, u32::try_from(unmarked).unwrap_or(u32::MAX))
}

/// Every superscript marker in the body flow, in reading order.
fn body_markers(blocks: &[BlockView], zone: &[BlockId]) -> Vec<Marker> {
    blocks
        .iter()
        .filter(|block| !zone.contains(&block.id))
        .flat_map(|block| {
            block.runs().filter_map(move |run| {
                let text = run.text.trim();
                (run.superscript && is_marker_text(text)).then(|| Marker {
                    text: text.to_owned(),
                    block: block.id,
                    run: run.id,
                    page: block.page,
                })
            })
        })
        .collect()
}

/// Whether a short rule sits immediately above this note.
///
/// A confidence signal and never a requirement: many books set their notes with no rule at
/// all, and a detector that required one would find no notes in them.
fn has_separator_rule(
    note: &Found,
    blocks: &[BlockView],
    rules: &[VectorRegion],
    body_size_pt: f32,
    t: &Thresholds,
) -> bool {
    let column_width = blocks
        .iter()
        .find(|block| block.id == note.block)
        .map(|block| block.column_width_pt)
        .unwrap_or_default();
    if column_width <= 0.0 {
        return false;
    }
    let gap = body_size_pt * t.footnote.rule_gap_max_em as f32;
    rules.iter().any(|rule| {
        rule.is_rule
            && rule.page.index == note.page
            && rule.is_horizontal()
            && f64::from(rule.rule_length() / column_width) < t.footnote.rule_max_width_ratio
            && rule.bbox.y1 <= note.top_y
            && note.top_y - rule.bbox.y1 <= gap
    })
}

#[cfg(test)]
mod note_text_tests {
    use super::*;
    use oc_model::geom::Rect;
    use oc_model::layout::BlockKindHint;
    use oc_model::text::Line;

    fn line(text: &str, y: f32) -> crate::view::LineView {
        let bbox = Rect {
            x0: 0.0,
            y0: y,
            x1: 300.0,
            y1: y + 8.0,
        };
        crate::view::LineView {
            line: Line {
                runs: Vec::new(),
                bbox,
                baseline_y: y + 7.0,
                ends_with_hyphen: false,
                indent_pt: 0.0,
                right_gap_pt: 0.0,
            },
            text: text.to_owned(),
            runs: Vec::new(),
        }
    }

    fn block(page: u32, lines: &[&str]) -> BlockView {
        let views: Vec<_> = lines
            .iter()
            .enumerate()
            .map(|(index, text)| line(text, 700.0 + 10.0 * index as f32))
            .collect();
        let text = lines.join(" ");
        BlockView {
            id: BlockId::derive(
                page,
                Rect {
                    x0: 0.0,
                    y0: 700.0,
                    x1: 300.0,
                    y1: 760.0,
                },
                &text,
            ),
            page,
            order: 0,
            bbox: Rect {
                x0: 0.0,
                y0: 700.0,
                x1: 300.0,
                y1: 760.0,
            },
            column: 0,
            kind_hint: BlockKindHint::Text,
            text,
            lines: views,
            column_width_pt: 300.0,
            space_above_pt: 0.0,
            page_height_pt: 792.0,
        }
    }

    fn c(text: &str) -> oc_model::extract::CharHistogram {
        oc_model::ledger::c_of(text)
    }

    /// `structure/lost/unattached-note-text`, as an invariant: **note assembly carries every
    /// character of the zone it is given**, whatever the markers look like.
    ///
    /// The first shape is the one this phase opened on — page 42, footnote 8's marker glued to
    /// its first word so the rule does not read it, and 644 characters discarded until footnote
    /// 9's marker opened something. The others are the ways the same fall-through arm is
    /// reached: every line unmarked, a lettered note, a marker in the middle of the zone.
    #[test]
    fn note_assembly_keeps_every_character_of_the_zone() {
        let zones: Vec<Vec<BlockView>> = vec![
            // Page 42: glued marker, continuation, then a marker the rule does read.
            vec![block(
                41,
                &[
                    "8Cornelia Koppetsch zu zitieren, bedarf einer Fußnote.",
                    "Die an der TU Darmstadt lehrende Soziologin",
                    "9. Eine zweite Anmerkung.",
                ],
            )],
            // Nothing recognisable at all.
            vec![block(3, &["a Lettered note text", "and its continuation"])],
            // Marked, then a second block whose first line is unmarked continuation.
            vec![
                block(7, &["1. First note", "runs on"]),
                block(8, &["carried over from page seven", "2. Second note"]),
            ],
        ];

        for zone in &zones {
            let refs: Vec<&BlockView> = zone.iter().collect();
            let (notes, _) = note_bodies(&refs);

            let mut given = oc_model::extract::CharHistogram::new();
            for block in zone {
                for line in &block.lines {
                    given = given.union(&c(&line.text));
                }
            }
            let mut kept = oc_model::extract::CharHistogram::new();
            for note in &notes {
                kept = kept.union(&c(&note.text));
            }

            assert_eq!(
                kept,
                given,
                "note assembly dropped {:?} from a zone of {} blocks",
                given.difference(&kept).iter().collect::<Vec<_>>(),
                zone.len()
            );
        }
    }

    /// The unmarked note is reported, and counted whole: its continuation lines included.
    #[test]
    fn an_unmarked_note_is_counted_with_its_continuations() {
        let zone = [block(3, &["a Lettered note text", "and its continuation"])];
        let refs: Vec<&BlockView> = zone.iter().collect();
        let (notes, unmarked) = note_bodies(&refs);

        assert_eq!(notes.len(), 1);
        assert!(notes[0].marker.is_empty());
        assert_eq!(
            unmarked as u64,
            c("a Lettered note text and its continuation").total()
        );
    }
}
