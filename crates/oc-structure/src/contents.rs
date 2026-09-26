//! The printed contents page: found by its shape, read for the headings it names, and emitted
//! as links to them.
//!
//! A contents page is recognised by geometry and arithmetic, never by its title: rows that end
//! in a page number, separated from their titles by a leader or by white space or set flush to
//! one right edge, whose numbers do not go down. `İçindekiler`, `Inhalt` and `Contents` are
//! never read — which is what lets this work in a language nobody listed.
//!
//! Three things are done with it:
//!
//! 1. **Its entries become links.** The page stays in the book as the book printed it — every
//!    character of it, leaders and numbers included — and each entry's title becomes a link to
//!    the heading it names, so the printed contents page works in the reflowed book.
//! 2. **It finds the headings the typography hid.** An entry whose page holds no heading, but
//!    holds a short block that reads as the entry's title, has found a heading set at the body
//!    size: that block is promoted.
//! 3. **It is not a table.** Two columns of text and numbers look like one to the table
//!    detector; the page is claimed here first.

use std::collections::{BTreeMap, BTreeSet};

use oc_core::thresholds::Thresholds;
use oc_model::ids::{BlockId, ClusterId};
use oc_model::lang::LangTag;
use oc_text::fold::fold_key;
use oc_text::similarity::normalised_edit_distance;

use crate::headings::levels::{HeadingAssignment, LevelSource};
use crate::headings::numbering::{is_roman, roman_value};
use crate::headings::toc_page::{TocEntry, TocPage};
use crate::view::BlockView;

/// The characters a leader is drawn with, besides the spaces between them.
const LEADER_FILL: [char; 7] = [
    '.', '\u{00B7}', '\u{2026}', '_', '-', '\u{2027}', '\u{2022}',
];

/// One entry of a printed contents page.
#[derive(Clone, Debug, PartialEq)]
pub struct ContentsEntry {
    pub block: BlockId,
    /// The positions, in `block`, of the lines the entry is printed on.
    pub positions: Vec<usize>,
    /// The title as printed, without its leader and page number.
    pub title: String,
    /// The page number as printed; `None` for a line that names a part and gives no page.
    pub folio: Option<String>,
    /// 1-based, from the title's indent.
    pub level: u8,
    pub page: u32,
    /// The heading the entry names, once it has been found.
    pub target: Option<BlockId>,
    /// Where the title starts on the line, which is what its level is read from.
    pub indent: f32,
}

/// A book's printed contents, over one or more consecutive pages.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Contents {
    pub pages: Vec<u32>,
    pub entries: Vec<ContentsEntry>,
}

impl Contents {
    /// The blocks the contents page is printed in: every block holding at least one entry.
    pub fn blocks(&self) -> BTreeSet<BlockId> {
        self.entries.iter().map(|entry| entry.block).collect()
    }

    /// The entries that give a page, as the older contents parser's shape, which the level
    /// assignment reads.
    pub fn as_toc_page(&self) -> Option<TocPage> {
        let entries: Vec<TocEntry> = self
            .entries
            .iter()
            .filter_map(|entry| {
                Some(TocEntry {
                    title: entry.title.clone(),
                    folio: entry.folio.clone()?,
                    level: entry.level,
                })
            })
            .collect();
        (!entries.is_empty()).then(|| TocPage {
            page: self.pages.first().copied().unwrap_or_default(),
            entries,
            line_share: 1.0,
        })
    }
}

/// One line of a candidate page, read as a possible entry.
#[derive(Clone, Debug)]
struct Row {
    block: BlockId,
    position: usize,
    x0: f32,
    x1: f32,
    title: String,
    /// The page number and its value; arabic or roman.
    folio: Option<(String, i64, bool)>,
    /// A leader, or white space wider than a word gap, between title and number.
    separated: bool,
}

/// Find the printed contents: the first run of consecutive pages near the front, or failing
/// that near the back, whose rows are entries.
pub fn find_contents(blocks: &[BlockView], page_count: u32, t: &Thresholds) -> Option<Contents> {
    let front = u32::try_from(t.toc.max_front_pages.max(0)).unwrap_or(u32::MAX);
    let back = u32::try_from(t.toc.max_back_pages.max(0)).unwrap_or(0);
    let mut pages: Vec<u32> = blocks.iter().map(|block| block.page).collect();
    pages.sort_unstable();
    pages.dedup();

    let searched: Vec<u32> = pages
        .iter()
        .copied()
        .filter(|page| *page < front || page.saturating_add(back) >= page_count)
        .collect();

    let mut found: Option<Contents> = None;
    for page in searched {
        let rows = rows_of(blocks, page, t);
        let continuing = found
            .as_ref()
            .is_some_and(|contents| contents.pages.last() == Some(&(page.saturating_sub(1))));
        if !is_contents(&rows, continuing, t) {
            if found.is_some() && !continuing {
                break;
            }
            if found.is_some() {
                break;
            }
            continue;
        }
        let contents = found.get_or_insert_with(Contents::default);
        contents.pages.push(page);
        contents.entries.extend(entries_of(&rows, page));
    }
    let mut contents = found?;
    assign_levels(&mut contents.entries, t);
    Some(contents)
}

/// Read every line of one page as a row.
fn rows_of(blocks: &[BlockView], page: u32, t: &Thresholds) -> Vec<Row> {
    let gap_em = t.toc.min_gap_em as f32;
    let min_fill = usize::try_from(t.toc.min_leader_chars.max(1)).unwrap_or(usize::MAX);
    let mut rows = Vec::new();
    for block in blocks.iter().filter(|block| block.page == page) {
        for (position, line) in block.lines.iter().enumerate() {
            let text = line.text.trim();
            if text.is_empty() {
                continue;
            }
            let (title, folio, fill) = split_folio(text);
            let separated = folio.is_some()
                && (fill >= min_fill || {
                    // White space: the run the number is set in starts well clear of the run
                    // before it.
                    let size = line.size_pt().max(1.0);
                    let last = line
                        .runs
                        .iter()
                        .rev()
                        .find(|run| !run.text.trim().is_empty());
                    let before = line
                        .runs
                        .iter()
                        .rev()
                        .filter(|run| !run.text.trim().is_empty())
                        .nth(1);
                    match (last, before, &folio) {
                        (Some(last), Some(before), Some((printed, _, _)))
                            if last.text.trim().ends_with(printed.as_str()) =>
                        {
                            last.bbox.x0 - before.bbox.x1 >= gap_em * size
                        }
                        _ => false,
                    }
                });
            rows.push(Row {
                block: block.id,
                position,
                x0: line.bbox().x0,
                x1: line.bbox().x1,
                title,
                folio,
                separated,
            });
        }
    }
    // Set flush to one right edge: the numbers of a contents page line up on the right however
    // long the titles are, and a line whose number sits on that edge is separated from its
    // title by the page's own layout.
    let edges: Vec<f32> = rows
        .iter()
        .filter(|row| row.folio.is_some())
        .map(|row| row.x1)
        .collect();
    if let Some(edge) = modal(&edges) {
        let tolerance = t.toc.right_edge_tolerance_pt as f32;
        for row in rows.iter_mut() {
            if row.folio.is_some() && (row.x1 - edge).abs() <= tolerance {
                row.separated = true;
            }
        }
    }
    rows
}

/// The number that ends a line, if one does, with the title before it and how many leader
/// characters stood between them.
fn split_folio(text: &str) -> (String, Option<(String, i64, bool)>, usize) {
    let Some(last) = text.split_whitespace().next_back() else {
        return (text.to_owned(), None, 0);
    };
    // A leader drawn without spaces runs straight into the number: `Method.....12`.
    let number: String = last
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let reading =
        if !number.is_empty() && number.len() <= 4 && number.chars().all(|c| c.is_ascii_digit()) {
            number.parse::<i64>().ok().map(|value| (value, true))
        } else if is_roman(&number)
            && (number.chars().all(|c| c.is_ascii_lowercase())
                || number.chars().all(|c| c.is_ascii_uppercase()))
        {
            roman_value(&number).map(|value| (i64::from(value), false))
        } else {
            None
        };
    let Some((value, arabic)) = reading else {
        return (text.to_owned(), None, 0);
    };
    let head = &text[..text.len() - number.len()];
    let title = head
        .trim_end_matches(|c: char| c.is_whitespace() || LEADER_FILL.contains(&c))
        .trim()
        .to_owned();
    let fill = head[title.len().min(head.len())..]
        .chars()
        .filter(|c| LEADER_FILL.contains(c))
        .count();
    if !title.chars().any(char::is_alphabetic) {
        return (text.to_owned(), None, 0);
    }
    (title, Some((number, value, arabic)), fill)
}

/// Whether a page's rows are a contents page: enough separated entries, a large enough share
/// of its rows, and numbers that do not go down.
fn is_contents(rows: &[Row], continuing: bool, t: &Thresholds) -> bool {
    let entries: Vec<&Row> = rows
        .iter()
        .filter(|row| row.folio.is_some() && row.separated)
        .collect();
    let min_entries = usize::try_from(t.toc.min_entries.max(1)).unwrap_or(usize::MAX);
    let needed = if continuing { 2 } else { min_entries };
    if entries.len() < needed || rows.is_empty() {
        return false;
    }
    if (entries.len() as f64) < t.toc.contents_min_row_share * rows.len() as f64 {
        return false;
    }
    let mut pairs = 0usize;
    let mut rising = 0usize;
    for pair in entries.windows(2) {
        let (Some((_, a, arabic_a)), Some((_, b, arabic_b))) = (&pair[0].folio, &pair[1].folio)
        else {
            continue;
        };
        if arabic_a != arabic_b {
            continue;
        }
        pairs += 1;
        if b >= a {
            rising += 1;
        }
    }
    pairs == 0 || rising as f64 >= t.toc.contents_min_rising_share * pairs as f64
}

/// The entries of an accepted page. A line with no number directly above an entry in the same
/// block, whose entry continues it in lower case, is the first line of that entry's title;
/// any other line with no number is an entry of its own that gives no page — a part title.
fn entries_of(rows: &[Row], page: u32) -> Vec<ContentsEntry> {
    let mut out: Vec<ContentsEntry> = Vec::new();
    let mut pending: Option<&Row> = None;
    for row in rows {
        match &row.folio {
            Some((printed, _, _)) if row.separated => {
                let mut positions = vec![row.position];
                let mut title = row.title.clone();
                if let Some(before) = pending.take() {
                    // A title wrapped onto a second line: the first line gives no page, the
                    // second does, and the second either continues in lower case or is plainly
                    // not an entry of its own — the first opens with the entry's number and the
                    // second does not.
                    let adjacent = before.block == row.block && before.position + 1 == row.position;
                    let numbered = |text: &str| {
                        text.split_whitespace().next().is_some_and(|token| {
                            let bare = token.trim_matches(|c: char| !c.is_alphanumeric());
                            !bare.is_empty()
                                && (bare.chars().all(|c| c.is_ascii_digit())
                                    || (is_roman(bare)
                                        && bare.chars().all(|c| c.is_ascii_uppercase())))
                        })
                    };
                    let continues = adjacent
                        && (row.title.chars().next().is_some_and(char::is_lowercase)
                            || (numbered(&before.title) && !numbered(&row.title)));
                    if continues {
                        positions.insert(0, before.position);
                        title = format!("{} {}", before.title, row.title);
                    } else {
                        out.push(unnumbered(before, page));
                    }
                }
                out.push(ContentsEntry {
                    block: row.block,
                    positions,
                    title,
                    folio: Some(printed.clone()),
                    level: 1,
                    page,
                    target: None,
                    indent: row.x0,
                });
            }
            _ => {
                if let Some(before) = pending.replace(row) {
                    out.push(unnumbered(before, page));
                }
            }
        }
    }
    if let Some(before) = pending {
        out.push(unnumbered(before, page));
    }
    // Only blocks that hold an entry with a page are the contents; a title row above them in a
    // block of its own stays what it is.
    let numbered: BTreeSet<BlockId> = out
        .iter()
        .filter(|entry| entry.folio.is_some())
        .map(|entry| entry.block)
        .collect();
    out.retain(|entry| numbered.contains(&entry.block));
    out
}

fn unnumbered(row: &Row, page: u32) -> ContentsEntry {
    ContentsEntry {
        block: row.block,
        positions: vec![row.position],
        title: row.title.clone(),
        folio: None,
        level: 1,
        page,
        target: None,
        indent: row.x0,
    }
}

/// Levels from the indent ladder: the shallowest title is level 1 and each distinct deeper
/// indent one more, to at most three.
fn assign_levels(entries: &mut [ContentsEntry], t: &Thresholds) {
    let step = t.toc.indent_step_pt as f32;
    // Indents are read against each page's own left edge: facing pages of a two-page contents
    // are set in different margins, and the second page's shift is not a level.
    let mut left: BTreeMap<u32, f32> = BTreeMap::new();
    for entry in entries.iter().filter(|entry| entry.folio.is_some()) {
        let edge = left.entry(entry.page).or_insert(entry.indent);
        *edge = edge.min(entry.indent);
    }
    for entry in entries.iter_mut() {
        if let Some(edge) = left.get(&entry.page) {
            entry.indent -= edge;
        }
    }
    let mut ladder: Vec<f32> = entries
        .iter()
        .filter(|entry| entry.folio.is_some())
        .map(|entry| entry.indent)
        .collect();
    ladder.sort_by(f32::total_cmp);
    ladder.dedup_by(|a, b| (*a - *b).abs() < step);
    for entry in entries.iter_mut() {
        let rung = ladder
            .iter()
            .rposition(|rung| entry.indent + step > *rung)
            .unwrap_or(0);
        entry.level = u8::try_from(rung + 1).unwrap_or(1).clamp(1, 3);
    }
}

/// The most common value, to the nearest point.
fn modal(values: &[f32]) -> Option<f32> {
    let mut counts: BTreeMap<i64, (usize, f32)> = BTreeMap::new();
    for value in values {
        let entry = counts.entry(value.round() as i64).or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 += value;
    }
    counts
        .into_values()
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(count, sum)| sum / count as f32)
}

/// Point each entry at the heading it names, promoting a block to a heading where the entry's
/// page holds one that reads as the title and no heading does.
///
/// The page an entry names is read through the book's own page labels (`furniture`'s); where a
/// book has none, through the offset between printed and physical pages that the entries
/// already matched agree on.
#[allow(clippy::too_many_arguments)]
pub fn link_contents(
    contents: &mut Contents,
    headings: &mut Vec<HeadingAssignment>,
    blocks: &[BlockView],
    labels: &[Option<String>],
    lang: &LangTag,
    cluster_of: impl Fn(BlockId) -> ClusterId,
    joined: &BTreeMap<BlockId, String>,
    members: &BTreeSet<BlockId>,
    t: &Thresholds,
) {
    let ned_max = t.toc.match_ned_max as f32;
    let contents_pages: BTreeSet<u32> = contents.pages.iter().copied().collect();
    let by_label: BTreeMap<String, u32> = labels
        .iter()
        .enumerate()
        .filter_map(|(page, label)| {
            Some((label.as_ref()?.trim().to_owned(), u32::try_from(page).ok()?))
        })
        .fold(BTreeMap::new(), |mut map, (label, page)| {
            map.entry(label).or_insert(page);
            map
        });

    let key = |text: &str| -> String { fold_key(&strip_numbering(text), lang.clone()).to_string() };
    // How well a title names a text: the normalised edit distance when it is small, or — for a
    // title printed over two lines of which the heading holds one, or a heading that carries a
    // subtitle the contents page leaves off — containment of one in the other, when the shorter
    // is at least half the longer. Lower is better; `None` is no match.
    let score = |title: &str, text: &str| -> Option<f32> {
        let (a, b) = (key(title), key(text));
        if a.is_empty() || b.is_empty() {
            return None;
        }
        let distance = normalised_edit_distance(&a, &b);
        if distance <= ned_max {
            return Some(distance);
        }
        let (short, long) = if a.chars().count() <= b.chars().count() {
            (&a, &b)
        } else {
            (&b, &a)
        };
        let (short_n, long_n) = (short.chars().count(), long.chars().count());
        (short_n >= 4 && short_n * 2 >= long_n && long.contains(short.as_str()))
            .then(|| ned_max + (1.0 - short_n as f32 / long_n as f32))
    };
    // What a heading reads as: a title set over several blocks reads as all of them, and the
    // blocks after the first are not headings of their own to link to.
    let text_of = |heading: &HeadingAssignment| -> String {
        joined
            .get(&heading.block)
            .cloned()
            .unwrap_or_else(|| heading.text.clone())
    };
    let mut taken: BTreeSet<BlockId> = BTreeSet::new();
    let max_lines = usize::try_from(t.toc.promote_max_lines.max(1)).unwrap_or(1);
    let order_of: BTreeMap<BlockId, u32> =
        blocks.iter().map(|block| (block.id, block.order)).collect();

    // Pass one: by page label and title.
    let mut offsets: Vec<i64> = Vec::new();
    for entry in contents.entries.iter_mut() {
        let Some(folio) = entry.folio.clone() else {
            continue;
        };
        let Some(&page) = by_label.get(folio.trim()) else {
            continue;
        };
        let near = |candidate: u32| candidate + 1 >= page && candidate <= page + 2;
        let found = best_heading(
            headings,
            |heading| {
                near(heading.page)
                    && !contents_pages.contains(&heading.page)
                    && !members.contains(&heading.block)
                    && !taken.contains(&heading.block)
            },
            |heading| {
                score(&entry.title, &text_of(heading)).map(|s| (s, heading.page.abs_diff(page)))
            },
        )
        .map(|heading| (heading.block, heading.page));
        if let Some((block, at)) = found {
            entry.target = Some(block);
            taken.insert(block);
            if let Ok(value) = folio.trim().parse::<i64>() {
                offsets.push(i64::from(at) - value);
            }
        }
    }
    let offset = {
        let mut sorted = offsets.clone();
        sorted.sort_unstable();
        sorted.get(sorted.len() / 2).copied()
    };

    // Pass two: the entries still open — through the offset where there are no labels, then a
    // block on the named page that reads as the title, promoted.
    for entry in contents.entries.iter_mut() {
        if entry.target.is_some() {
            continue;
        }
        let Some(folio) = entry.folio.clone() else {
            continue;
        };
        let page = by_label.get(folio.trim()).copied().or_else(|| {
            let value = folio.trim().parse::<i64>().ok()?;
            u32::try_from(value + offset?).ok()
        });
        let Some(page) = page else { continue };
        let near = |candidate: u32| candidate + 1 >= page && candidate <= page + 2;
        let heading = best_heading(
            headings,
            |heading| {
                near(heading.page)
                    && !contents_pages.contains(&heading.page)
                    && !members.contains(&heading.block)
                    && !taken.contains(&heading.block)
            },
            |heading| {
                score(&entry.title, &text_of(heading)).map(|s| (s, heading.page.abs_diff(page)))
            },
        )
        .map(|heading| heading.block);
        if let Some(block) = heading {
            entry.target = Some(block);
            taken.insert(block);
            continue;
        }
        let promoted = blocks
            .iter()
            .filter(|block| near(block.page) && !contents_pages.contains(&block.page))
            .filter(|block| block.lines.len() <= max_lines && !taken.contains(&block.id))
            .filter_map(|block| score(&entry.title, &block.text).map(|s| (s, block)))
            .min_by(|(a, x), (b, y)| a.total_cmp(b).then(x.order.cmp(&y.order)))
            .map(|(_, block)| block);
        if let Some(block) = promoted {
            entry.target = Some(block.id);
            taken.insert(block.id);
            promote(headings, block, entry.level, &cluster_of);
        }
    }

    // Pass three, for a book whose page numbers were not recovered: the entries in their own
    // order, each matched to the best heading — or short block, promoted — after the entry
    // before it, and after the contents page unless its number is a roman front-matter folio.
    // A contents page lists its chapters in the order they come, which is the constraint that
    // makes a title match safe without a page.
    let after_contents = blocks
        .iter()
        .filter(|block| contents_pages.contains(&block.page))
        .map(|block| block.order)
        .max()
        .unwrap_or(0);
    let mut cursor: Option<u32> = None;
    for entry in contents.entries.iter_mut() {
        if let Some(target) = entry.target {
            if let Some(&order) = order_of.get(&target) {
                cursor = Some(cursor.map_or(order, |at| at.max(order)));
            }
            continue;
        }
        let Some(folio) = entry.folio.clone() else {
            continue;
        };
        let front = folio.trim().chars().all(char::is_alphabetic);
        let floor = cursor.unwrap_or(0);
        let admit = |order: u32, page: u32| {
            (cursor.is_none() || order > floor)
                && !contents_pages.contains(&page)
                && (front || order > after_contents)
        };
        let heading = best_heading(
            headings,
            |heading| {
                admit(heading.order, heading.page)
                    && !members.contains(&heading.block)
                    && !taken.contains(&heading.block)
            },
            |heading| score(&entry.title, &text_of(heading)).map(|s| (s, heading.order)),
        )
        .map(|heading| (heading.block, heading.order));
        if let Some((block, order)) = heading {
            entry.target = Some(block);
            taken.insert(block);
            cursor = Some(order);
            continue;
        }
        let promoted = blocks
            .iter()
            .filter(|block| admit(block.order, block.page) && block.lines.len() <= max_lines)
            .filter(|block| !taken.contains(&block.id))
            .filter_map(|block| score(&entry.title, &block.text).map(|s| (s, block)))
            .min_by(|(a, x), (b, y)| a.total_cmp(b).then(x.order.cmp(&y.order)))
            .map(|(_, block)| block);
        if let Some(block) = promoted {
            entry.target = Some(block.id);
            taken.insert(block.id);
            cursor = Some(block.order);
            promote(headings, block, entry.level, &cluster_of);
        }
    }

    if std::env::var_os("OC_DEBUG_TOC").is_some() {
        for entry in &contents.entries {
            let target = entry
                .target
                .and_then(|id| blocks.iter().find(|block| block.id == id))
                .map(|block| block.text.chars().take(50).collect::<String>());
            eprintln!(
                "DEBUG toc {:?} folio {:?} level {} -> {:?}",
                entry.title, entry.folio, entry.level, target
            );
        }
    }
    // A contents page that named most of its chapters has said what the book's top level is:
    // the headings it links to take the levels it printed them at, and every other heading
    // sits below the top of them. Type size is weaker evidence than the book's own contents.
    let numbered = contents
        .entries
        .iter()
        .filter(|entry| entry.folio.is_some())
        .count();
    let linked: BTreeMap<BlockId, u8> = contents
        .entries
        .iter()
        .filter(|entry| entry.folio.is_some())
        .filter_map(|entry| Some((entry.target?, entry.level)))
        .collect();
    let min_linked = usize::try_from(t.toc.min_entries.max(1)).unwrap_or(usize::MAX);
    // The book's own outline outranks the contents page's indents: it is the hierarchy the
    // publisher encoded, where the indent ladder is read off the page's geometry — which set
    // the chapters of a technical book one step under its preface (2026-09-26).
    let outline_levels = headings
        .iter()
        .any(|heading| heading.source == LevelSource::Outline);
    if !outline_levels
        && linked.len() >= min_linked
        && linked.len() as f64 >= t.toc.levels_min_linked_share * numbered as f64
    {
        let top = linked.values().copied().min().unwrap_or(1);
        for heading in headings.iter_mut() {
            match linked.get(&heading.block) {
                Some(level) => {
                    heading.level = *level;
                    heading.source = LevelSource::TocPage;
                }
                // The contents page's own title is not one of the chapters the page names,
                // and is not put under them.
                None if contents_pages.contains(&heading.page) => {}
                None => heading.level = heading.level.max(top.saturating_add(1)).min(6),
            }
        }
    }

    headings.sort_by_key(|heading| heading.order);
    crate::headings::levels::repair_levels(headings);
}

/// The best heading for an entry among those a filter admits: by score, then by the second
/// key the caller ranks by (nearness to the named page, or reading order), then reading order.
fn best_heading(
    headings: &[HeadingAssignment],
    admit: impl Fn(&HeadingAssignment) -> bool,
    rank: impl Fn(&HeadingAssignment) -> Option<(f32, u32)>,
) -> Option<&HeadingAssignment> {
    headings
        .iter()
        .filter(|heading| admit(heading))
        .filter_map(|heading| rank(heading).map(|key| (key, heading)))
        .min_by(|((a, da), ha), ((b, db), hb)| {
            a.total_cmp(b)
                .then(da.cmp(db))
                .then(ha.order.cmp(&hb.order))
        })
        .map(|(_, heading)| heading)
}

/// Make a block a heading at a contents entry's level, unless it already is one.
fn promote(
    headings: &mut Vec<HeadingAssignment>,
    block: &BlockView,
    level: u8,
    cluster_of: &impl Fn(BlockId) -> ClusterId,
) {
    if headings.iter().any(|heading| heading.block == block.id) {
        return;
    }
    headings.push(HeadingAssignment {
        block: block.id,
        order: block.order,
        page: block.page,
        text: block.text.trim().to_owned(),
        level,
        cluster: cluster_of(block.id),
        numbering: None,
        source: LevelSource::TocPage,
    });
}

/// A title without the number it is printed with: `1. Turing's Brains` and `Turing's Brains`
/// name the same chapter, and so do `Chapter 3: Home` and `Home`.
fn strip_numbering(text: &str) -> String {
    let mut tokens: Vec<&str> = text.split_whitespace().collect();
    while tokens.len() > 1 {
        let first = tokens[0].trim_matches(|c: char| !c.is_alphanumeric());
        let numeric = first.chars().all(|c| c.is_ascii_digit() || c == '.')
            || (is_roman(first) && first.chars().all(|c| c.is_ascii_uppercase()));
        if first.is_empty() || numeric {
            tokens.remove(0);
        } else {
            break;
        }
    }
    tokens.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_ending_in_a_number_after_a_leader_is_split() {
        let (title, folio, fill) = split_folio("Teşekkür ........ 11");
        assert_eq!(title, "Teşekkür");
        assert_eq!(
            folio.map(|(printed, value, _)| (printed, value)),
            Some(("11".to_owned(), 11))
        );
        assert!(fill >= 2);
        let (title, folio, _) = split_folio("Method.....12");
        assert_eq!(title, "Method");
        assert!(folio.is_some());
        let (_, folio, _) = split_folio("Preface xii");
        assert_eq!(
            folio.map(|(_, value, arabic)| (value, arabic)),
            Some((12, false))
        );
    }

    #[test]
    fn a_line_that_is_only_a_number_is_not_an_entry() {
        assert!(split_folio("23").1.is_none());
        assert!(split_folio("2019").1.is_none() || split_folio("2019").0 == "2019");
    }

    #[test]
    fn the_number_a_title_is_printed_with_is_not_part_of_its_name() {
        assert_eq!(
            strip_numbering("1. Turing'in Elektronik Beyinleri"),
            "Turing'in Elektronik Beyinleri"
        );
        assert_eq!(strip_numbering("IV Home"), "Home");
        assert_eq!(strip_numbering("1984"), "1984");
    }
}
