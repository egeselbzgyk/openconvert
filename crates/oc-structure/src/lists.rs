//! Lists: markers, hanging indents, siblings and nesting (PIPELINE §8.5).
//!
//! `List-item` is the best-detected structural class in DocLayNet — 86.2 mAP against human
//! agreement of 87–88 — which says the visual definition is crisp and that heuristics should
//! do well here (R10 §6.11). The interesting part is therefore not finding items, it is
//! refusing to find them where they are not:
//!
//! **`1984 was a strange year.` must stay a paragraph.** Two guards, and both are needed. The
//! marker pattern requires a *terminator* after the number — `1.` or `1)`, never a bare
//! `1984` — and a run of at least `list.min_siblings` items sharing marker kind and indent is
//! required before any of them is a list. One line that looks like an item is not a list.

use oc_core::thresholds::Thresholds;
use oc_model::confidence::{Confidence, Signal};
use oc_model::doc::{Content, List, ListItem, Severity, Warning};
use oc_model::ids::BlockId;
use serde::Serialize;

use crate::build::{para_of, Minter};
use crate::headings::numbering::is_roman;
use crate::view::{BlockView, LineView};

/// An ordered list's numbers are not contiguous (PIPELINE §8's end-of-stage validation).
pub const W_LIST_NUMBERING_GAP: &str = "W_LIST_NUMBERING_GAP";

/// The bullet glyphs PIPELINE §8.5 names.
const BULLETS: [char; 8] = [
    '\u{2022}', '\u{00B7}', '\u{2013}', '\u{2014}', '*', '\u{2023}', '\u{25AA}', '\u{25E6}',
];

/// The characters that close a numbered marker. A number with none of them after it is a
/// number in a sentence.
const TERMINATORS: [char; 2] = ['.', ')'];

/// What kind of marker a line opens with. Two items are siblings only if their kinds match:
/// a bulleted list inside a numbered one is a different list, not a continuation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerKind {
    Bullet,
    Decimal,
    Alpha,
    Roman,
}

impl MarkerKind {
    pub fn is_ordered(&self) -> bool {
        !matches!(self, MarkerKind::Bullet)
    }
}

/// One line that opens with a list marker.
#[derive(Clone, Debug, PartialEq, Serialize)]
struct Marked {
    /// Index of the line within `lines`.
    line: usize,
    block: BlockId,
    page: u32,
    kind: MarkerKind,
    /// The marker as printed, including its terminator: `"1."`, `"•"`.
    marker: String,
    /// The ordinal a numbered marker carries. `None` for a bullet.
    value: Option<u32>,
    indent_pt: f32,
}

/// Find the document's lists.
///
/// Works on *lines* rather than on blocks, because a run of one-line items is one block to
/// Docstrum — five items set a body leading apart are geometrically one paragraph — and the
/// marker is what distinguishes them.
pub fn detect_lists(blocks: &[BlockView], skip: &[BlockId], t: &Thresholds) -> ListOutcome {
    let lines: Vec<(&BlockView, &LineView)> = blocks
        .iter()
        // A note at the foot of a page opens with `*` and so does a bulleted item, and two
        // notes on one page are two siblings at one indent. The note zone is decided first —
        // by the font size and the band, which a list has neither of — and the blocks it
        // claimed do not enter this one.
        .filter(|block| !skip.contains(&block.id))
        .flat_map(|block| block.lines.iter().map(move |line| (block, line)))
        .collect();

    let marked: Vec<Option<Marked>> = lines
        .iter()
        .enumerate()
        .map(|(index, (block, line))| {
            read_marker(line.text.trim()).map(|(kind, marker, value)| Marked {
                line: index,
                block: block.id,
                page: block.page,
                kind,
                marker,
                value,
                indent_pt: line.bbox().x0,
            })
        })
        .collect();

    let minimum = usize::try_from(t.list.min_siblings).unwrap_or(2);
    let tolerance = t.list.indent_step_tolerance_pt as f32;
    let mut minter = Minter::new();
    let mut warnings = Vec::new();
    let mut lists = Vec::new();
    let mut consumed_lines: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();

    // A list is a maximal run of marked lines, allowing unmarked continuation lines between
    // them, that holds at least `min_siblings` markers at its shallowest indent.
    let mut start = 0usize;
    while start < marked.len() {
        if marked[start].is_none() {
            start += 1;
            continue;
        }
        let mut end = start;
        let mut last_marked = start;
        while end < marked.len() {
            if marked[end].is_some() {
                last_marked = end;
                end += 1;
                continue;
            }
            // An unmarked line continues the item above it only while it stays inside the
            // list: a line indented back to the body margin has left.
            let inside = marked
                .get(last_marked)
                .and_then(Option::as_ref)
                .zip(lines.get(end))
                .is_some_and(|(marker, (_, line))| line.bbox().x0 >= marker.indent_pt - tolerance);
            if !inside {
                break;
            }
            end += 1;
        }
        let run = &marked[start..=last_marked];
        if let Some(list) = build_list(
            run,
            &lines,
            minimum,
            tolerance,
            t,
            &mut minter,
            &mut warnings,
        ) {
            lists.push(list);
            // Every line of the run, marked or continuation, is inside the list now.
            consumed_lines.extend(start..=last_marked);
        }
        start = last_marked + 1;
    }

    // A block is consumed only when *every* one of its non-empty lines is inside a list. A
    // block that mixes a list with the prose introducing it keeps its prose in the flow, and
    // the conservation check across the stage is what would catch the alternative.
    let mut covered: std::collections::BTreeMap<BlockId, (usize, usize)> =
        std::collections::BTreeMap::new();
    for (index, (block, line)) in lines.iter().enumerate() {
        if line.text.trim().is_empty() {
            continue;
        }
        let entry = covered.entry(block.id).or_insert((0, 0));
        entry.1 += 1;
        if consumed_lines.contains(&index) {
            entry.0 += 1;
        }
    }
    let consumed = covered
        .into_iter()
        .filter(|(_, (inside, total))| *total > 0 && inside == total)
        .map(|(block, _)| block)
        .collect();

    ListOutcome {
        lists,
        consumed,
        warnings,
    }
}

/// What list detection produced.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ListOutcome {
    pub lists: Vec<List>,
    /// Blocks every one of whose lines is inside a list, so the flow does not emit them twice.
    pub consumed: Vec<BlockId>,
    pub warnings: Vec<Warning>,
}

/// Turn one run of marked lines into a list, if it is one.
fn build_list(
    run: &[Option<Marked>],
    lines: &[(&BlockView, &LineView)],
    minimum: usize,
    tolerance: f32,
    t: &Thresholds,
    minter: &mut Minter,
    warnings: &mut Vec<Warning>,
) -> Option<List> {
    let markers: Vec<&Marked> = run.iter().flatten().collect();
    if markers.len() < minimum {
        return None;
    }
    // The shallowest indent is the top level of this list; anything deeper is nested.
    let top = markers
        .iter()
        .map(|marker| marker.indent_pt)
        .fold(f32::INFINITY, f32::min);
    let siblings: Vec<&&Marked> = markers
        .iter()
        .filter(|marker| (marker.indent_pt - top).abs() <= tolerance)
        .collect();
    if siblings.len() < minimum {
        return None;
    }
    // Sharing a marker kind is part of the definition of siblings (PIPELINE §8.5).
    let kind = siblings.first()?.kind;
    if !siblings.iter().all(|marker| marker.kind == kind) {
        return None;
    }

    let items = build_level(&markers, top, tolerance, 1, t, minter, lines);
    check_numbering(&siblings, warnings);

    Some(List {
        id: minter.mint(
            siblings.first()?.page,
            lines
                .get(siblings.first()?.line)
                .map(|(_, line)| line.bbox())
                .unwrap_or(oc_model::geom::Rect {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 0.0,
                    y1: 0.0,
                }),
            "list",
        ),
        ordered: kind.is_ordered(),
        start: siblings
            .first()
            .and_then(|marker| marker.value)
            .filter(|value| *value != 1),
        items,
        confidence: Confidence::deterministic(vec![
            Signal::new("siblings", siblings.len() as f32),
            Signal::new("depth", 1.0),
        ]),
    })
}

/// Build the items at one indent level, recursing into the deeper ones.
fn build_level(
    markers: &[&Marked],
    indent: f32,
    tolerance: f32,
    depth: u8,
    t: &Thresholds,
    minter: &mut Minter,
    lines: &[(&BlockView, &LineView)],
) -> Vec<ListItem> {
    let max_depth = u8::try_from(t.list.max_depth).unwrap_or(5);
    let at_level: Vec<usize> = markers
        .iter()
        .enumerate()
        .filter(|(_, marker)| (marker.indent_pt - indent).abs() <= tolerance)
        .map(|(position, _)| position)
        .collect();

    at_level
        .iter()
        .enumerate()
        .filter_map(|(position, start)| {
            let marker = markers.get(*start)?;
            let end = at_level.get(position + 1).copied().unwrap_or(markers.len());
            let deeper: Vec<&&Marked> = markers
                .get(start + 1..end)
                .unwrap_or_default()
                .iter()
                .filter(|inner| inner.indent_pt > indent + tolerance)
                .collect();

            // The item's own lines: its marked line and every unmarked line beneath it
            // before the next marker.
            let own = lines.get(marker.line).map(|(_, line)| *line)?;
            let para = para_of(minter, marker.page, &[marker.block], &[own]);

            let nested = if deeper.is_empty() || depth >= max_depth {
                None
            } else {
                let child_indent = deeper
                    .iter()
                    .map(|marker| marker.indent_pt)
                    .fold(f32::INFINITY, f32::min);
                let owned: Vec<&Marked> = deeper.iter().map(|marker| **marker).collect();
                let kind = owned.first()?.kind;
                Some(Box::new(List {
                    id: minter.mint(owned.first()?.page, own.bbox(), "nested list"),
                    ordered: kind.is_ordered(),
                    start: owned.first().and_then(|m| m.value).filter(|v| *v != 1),
                    items: build_level(
                        &owned,
                        child_indent,
                        tolerance,
                        depth.saturating_add(1),
                        t,
                        minter,
                        lines,
                    ),
                    confidence: Confidence::deterministic(vec![Signal::new(
                        "depth",
                        f32::from(depth) + 1.0,
                    )]),
                }))
            };

            Some(ListItem {
                content: vec![Content::Paragraph(para)],
                nested,
                marker: Some(marker.marker.clone()),
            })
        })
        .collect()
}

/// An ordered list's numbers must be contiguous (PIPELINE §8's validation).
fn check_numbering(siblings: &[&&Marked], warnings: &mut Vec<Warning>) {
    let values: Vec<u32> = siblings.iter().filter_map(|marker| marker.value).collect();
    if values.len() < 2 {
        return;
    }
    let contiguous = values
        .windows(2)
        .all(|pair| pair[1] == pair[0].saturating_add(1));
    if !contiguous {
        warnings.push(
            Warning::new(W_LIST_NUMBERING_GAP, Severity::Warn)
                .with_arg("numbers", format!("{values:?}"))
                .with_blocks(siblings.iter().map(|marker| marker.block).collect()),
        );
    }
}

/// Read the marker a line opens with, if it opens with one.
///
/// Returns the kind, the marker as printed, and the ordinal for a numbered one.
pub fn read_marker(text: &str) -> Option<(MarkerKind, String, Option<u32>)> {
    let mut chars = text.chars();
    let first = chars.next()?;
    if BULLETS.contains(&first) {
        // A bullet must be followed by space: an em dash opening a line of dialogue is not a
        // list item, and `—said the clerk` has no space after the dash.
        if !chars.next().is_some_and(char::is_whitespace) {
            return None;
        }
        return Some((MarkerKind::Bullet, first.to_string(), None));
    }

    let token = text.split_whitespace().next()?;
    // A marker is short by construction. Without this a whole sentence ending in a full stop
    // would be read as one enormous alphabetic marker.
    if token.chars().count() > 8 {
        return None;
    }
    let terminator = token.chars().next_back()?;
    if !TERMINATORS.contains(&terminator) {
        return None;
    }
    let body = token.get(..token.len() - terminator.len_utf8())?;
    if body.is_empty() {
        return None;
    }

    if body.chars().all(|c| c.is_ascii_digit()) {
        return Some((
            MarkerKind::Decimal,
            token.to_owned(),
            body.parse::<u32>().ok(),
        ));
    }
    if is_roman(body) {
        return Some((
            MarkerKind::Roman,
            token.to_owned(),
            crate::headings::numbering::roman_value(body),
        ));
    }
    if body.chars().count() == 1 && body.chars().all(char::is_alphabetic) {
        let value = body
            .chars()
            .next()
            .map(|c| u32::from(c.to_ascii_lowercase()) - u32::from('a') + 1);
        return Some((MarkerKind::Alpha, token.to_owned(), value));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_of_every_kind_are_read_with_their_ordinal() {
        assert_eq!(
            read_marker("1. Open the document"),
            Some((MarkerKind::Decimal, "1.".to_owned(), Some(1)))
        );
        assert_eq!(
            read_marker("12) Twelfth"),
            Some((MarkerKind::Decimal, "12)".to_owned(), Some(12)))
        );
        assert_eq!(
            read_marker("a. First"),
            Some((MarkerKind::Alpha, "a.".to_owned(), Some(1)))
        );
        assert_eq!(
            read_marker("iv. Fourth"),
            Some((MarkerKind::Roman, "iv.".to_owned(), Some(4)))
        );
        assert_eq!(
            read_marker("\u{2022} A bullet"),
            Some((MarkerKind::Bullet, "\u{2022}".to_owned(), None))
        );
    }

    /// Row 4.12. The number in a sentence is not a marker, and it is the terminator that says
    /// so: `1984` has none. The `>= 2 siblings` rule is the second guard, tested on the real
    /// fixture; this is the first, and it is the one that does not need a second line.
    #[test]
    fn year_paragraph_is_not_a_list_item() {
        assert_eq!(read_marker("1984 was a strange year."), None);
        assert_eq!(read_marker("2026 and the decade after it"), None);
        // Nor is a sentence that happens to end in a full stop an alphabetic marker: a
        // marker is short.
        assert_eq!(read_marker("Nevertheless. The clerk waited."), None);
        // Nor is a dash opening a line of dialogue.
        assert_eq!(read_marker("\u{2014}said the clerk, quietly."), None);
        assert_eq!(read_marker(""), None);
        assert_eq!(read_marker("."), None);
    }
}
