//! Chapter openers: the headings a book marks by a number and by where it puts it, not by the
//! face it sets it in.
//!
//! A novel very often opens each chapter with nothing but its number — `1`, `IV`, `Chapter 7`,
//! `BÖLÜM 3` — set at the body size, sunk down the page. Size rank cannot see it: there is no
//! style cluster larger than the body for it to belong to. What *is* there is language-free:
//!
//! 1. **The number.** A short block, a few tokens at most, whose first or last token is an
//!    arabic number or a roman numeral. What the other tokens say is not read — `Chapter`,
//!    `Kapitel` and `Bölüm` are all just a word next to the number — so nothing here knows a
//!    language.
//! 2. **The place.** Either the first text on its page and *sunk*, starting well below where
//!    the book's pages normally start; or set under a gap of several body lines. A chapter
//!    number is never the middle of a paragraph.
//! 3. **The sequence.** The numbers, in reading order, count up by one. That is what tells
//!    chapter numbers from a numbered list, a stray folio or a year, and it is checked over the
//!    whole book: a chain of at least `headings.opener_min_chain` members, spread over pages the
//!    way chapters are and folios are not.
//!
//! An OCR layer misreads numerals (`XVllI`, `il`), so a chain may step over one number, and a
//! candidate that sits where the missing number belongs is taken as it.

use oc_core::thresholds::Thresholds;
use oc_model::ids::BlockId;

use crate::headings::numbering::{is_roman, roman_value};
use crate::view::BlockView;

/// A block that numbers a chapter, found by its place and its sequence.
#[derive(Clone, Debug, PartialEq)]
pub struct Opener {
    pub block: BlockId,
    pub order: u32,
    pub page: u32,
    pub text: String,
    /// The number the block carries, as its sequence read it.
    pub value: Option<u32>,
}

/// One block that could number a chapter.
struct Candidate<'a> {
    block: &'a BlockView,
    value: Option<u32>,
}

/// Find the chapter openers among a document's blocks.
///
/// `body_size` is the body's size in points, which is what "several lines of air" and "sunk"
/// are measured in.
pub fn chapter_openers(blocks: &[BlockView], body_size: f32, t: &Thresholds) -> Vec<Opener> {
    if blocks.is_empty() || body_size <= 0.0 {
        return Vec::new();
    }
    let sink = body_size * t.headings.opener_sink_em as f32;
    let top = usual_top(blocks);

    let mut candidates: Vec<Candidate<'_>> = Vec::new();
    let mut previous_page: Option<u32> = None;
    for block in blocks {
        let first_on_page = previous_page != Some(block.page);
        previous_page = Some(block.page);
        if !shaped_like_a_number(block, t) {
            continue;
        }
        let sunk = first_on_page && top.is_some_and(|top| block.bbox.y0 >= top + sink);
        let under_air = !first_on_page && block.space_above_pt >= sink;
        if !(sunk || under_air) {
            continue;
        }
        candidates.push(Candidate {
            block,
            value: number_of(&block.text),
        });
    }

    let mut chosen = vec![false; candidates.len()];
    let min_chain = usize::try_from(t.headings.opener_min_chain.max(1)).unwrap_or(usize::MAX);
    loop {
        let chain = longest_chain(&candidates, &chosen);
        if chain.len() < min_chain || !spread_like_chapters(&candidates, &chain, t) {
            break;
        }
        for &index in &chain {
            chosen[index] = true;
        }
        // A step of two leaves a gap a misread numeral may be sitting in.
        for pair in chain.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let (Some(va), Some(vb)) = (candidates[a].value, candidates[b].value) else {
                continue;
            };
            if vb == va + 2 {
                if let Some(missing) = (a + 1..b).find(|&index| {
                    !chosen[index] && candidates[index].value.is_none_or(|value| value != va + 1)
                }) {
                    chosen[missing] = true;
                }
            }
        }
    }

    candidates
        .iter()
        .zip(&chosen)
        .filter(|(_, chosen)| **chosen)
        .map(|(candidate, _)| Opener {
            block: candidate.block.id,
            order: candidate.block.order,
            page: candidate.block.page,
            text: candidate.block.text.trim().to_owned(),
            value: candidate.value,
        })
        .collect()
}

/// Where the book's pages usually start: the median top of the first text block on a page.
fn usual_top(blocks: &[BlockView]) -> Option<f32> {
    let mut tops: Vec<f32> = Vec::new();
    let mut previous_page: Option<u32> = None;
    for block in blocks {
        if previous_page != Some(block.page) {
            tops.push(block.bbox.y0);
        }
        previous_page = Some(block.page);
    }
    if tops.is_empty() {
        return None;
    }
    tops.sort_by(f32::total_cmp);
    tops.get(tops.len() / 2).copied()
}

/// A short block of a few tokens, one end of which is a number.
fn shaped_like_a_number(block: &BlockView, t: &Thresholds) -> bool {
    let max_lines = usize::try_from(t.headings.opener_max_lines.max(1)).unwrap_or(1);
    let max_tokens = usize::try_from(t.headings.opener_max_tokens.max(1)).unwrap_or(1);
    let text = block.text.trim();
    if block.lines.is_empty() || block.lines.len() > max_lines || text.is_empty() {
        return false;
    }
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.len() > max_tokens {
        return false;
    }
    let numeric = |token: &&str| numeral(token).is_some() || looks_like_a_misread_numeral(token);
    tokens.first().is_some_and(numeric) || tokens.last().is_some_and(numeric)
}

/// Whether a heading's text is a chapter label — a number or numeral, with at most a few words
/// beside it — rather than a title: `3`, `IV`, `Chapter 7`, `BÖLÜM 3`.
pub fn is_number_label(text: &str, t: &Thresholds) -> bool {
    let max_tokens = usize::try_from(t.headings.opener_max_tokens.max(1)).unwrap_or(1);
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let numeric = |token: &&str| numeral(token).is_some() || looks_like_a_misread_numeral(token);
    !tokens.is_empty()
        && tokens.len() <= max_tokens
        && (tokens.first().is_some_and(numeric) || tokens.last().is_some_and(numeric))
}

/// The number a block carries at one of its ends.
fn number_of(text: &str) -> Option<u32> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    tokens
        .first()
        .and_then(|token| numeral(token))
        .or_else(|| tokens.last().and_then(|token| numeral(token)))
}

/// An arabic number of up to three digits or a well-formed roman numeral, with the punctuation
/// a heading hangs on a number stripped.
fn numeral(token: &str) -> Option<u32> {
    let bare = token.trim_matches(|c: char| !c.is_alphanumeric());
    if bare.is_empty() {
        return None;
    }
    if bare.len() <= 3 && bare.chars().all(|c| c.is_ascii_digit()) {
        return bare.parse().ok().filter(|value| *value > 0);
    }
    // Upper or lower case, never mixed: `Mix` and `Did` are words.
    let one_case = bare.chars().all(|c| c.is_ascii_uppercase())
        || bare.chars().all(|c| c.is_ascii_lowercase());
    if one_case && is_roman(bare) {
        let value = roman_value(bare)?;
        // Only a numeral whose canonical spelling is what was printed: `IL` and `VX` are not.
        return (value > 0 && to_roman(value).eq_ignore_ascii_case(bare)).then_some(value);
    }
    None
}

/// A token an OCR layer made out of a roman numeral: its letters, mixed with the digit one and
/// the lower-case l that stand in for `I`. Never read for a value; only allowed to fill a gap.
fn looks_like_a_misread_numeral(token: &str) -> bool {
    let bare = token.trim_matches(|c: char| !c.is_alphanumeric());
    !bare.is_empty()
        && bare.chars().count() <= 6
        && bare
            .chars()
            .all(|c| matches!(c, 'I' | 'V' | 'X' | 'L' | 'C' | 'i' | 'v' | 'x' | 'l' | '1'))
        && bare.chars().any(|c| c.is_ascii_uppercase() || c == '1')
}

fn to_roman(mut value: u32) -> String {
    const TABLE: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (step, letters) in TABLE {
        while value >= step {
            out.push_str(letters);
            value -= step;
        }
    }
    out
}

/// The longest run of candidates, in reading order, whose numbers count up by one — or by two
/// once, over a numeral nobody could read — each on a later page than the one before.
fn longest_chain(candidates: &[Candidate<'_>], taken: &[bool]) -> Vec<usize> {
    let count = candidates.len();
    let mut best: Vec<usize> = vec![0; count];
    let mut back: Vec<Option<usize>> = vec![None; count];
    for index in 0..count {
        if taken[index] {
            continue;
        }
        let Some(value) = candidates[index].value else {
            continue;
        };
        best[index] = 1;
        for earlier in 0..index {
            if taken[earlier] || best[earlier] == 0 {
                continue;
            }
            let Some(before) = candidates[earlier].value else {
                continue;
            };
            let step = value.checked_sub(before);
            let fits = matches!(step, Some(1 | 2))
                && candidates[earlier].block.page < candidates[index].block.page;
            if fits && best[earlier] + 1 > best[index] {
                best[index] = best[earlier] + 1;
                back[index] = Some(earlier);
            }
        }
    }
    let Some(end) = (0..count).max_by_key(|&index| (best[index], std::cmp::Reverse(index))) else {
        return Vec::new();
    };
    if best[end] == 0 {
        return Vec::new();
    }
    let mut chain = vec![end];
    let mut at = end;
    while let Some(previous) = back[at] {
        chain.push(previous);
        at = previous;
    }
    chain.reverse();
    chain
}

/// Whether a chain is spread over the book the way chapters are: a folio the furniture pass
/// missed counts up by one too, but a page apart, every page.
fn spread_like_chapters(candidates: &[Candidate<'_>], chain: &[usize], t: &Thresholds) -> bool {
    let mut gaps: Vec<u32> = chain
        .windows(2)
        .map(|pair| {
            candidates[pair[1]]
                .block
                .page
                .saturating_sub(candidates[pair[0]].block.page)
        })
        .collect();
    if gaps.is_empty() {
        return false;
    }
    gaps.sort_unstable();
    let median = gaps[gaps.len() / 2];
    i64::from(median) >= t.headings.opener_min_page_gap
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::thresholds::T;
    use oc_model::geom::Rect;
    use oc_model::layout::BlockKindHint;
    use oc_model::text::Line;

    use crate::view::LineView;

    fn block(page: u32, order: u32, y0: f32, text: &str, width: f32) -> BlockView {
        let bbox = Rect {
            x0: 50.0,
            y0,
            x1: 50.0 + width,
            y1: y0 + 12.0,
        };
        let line = LineView {
            line: Line {
                runs: Vec::new(),
                bbox,
                baseline_y: y0 + 10.0,
                ends_with_hyphen: false,
                indent_pt: 0.0,
                right_gap_pt: 0.0,
            },
            text: text.to_owned(),
            runs: Vec::new(),
            glue: false,
        };
        BlockView {
            id: BlockId::derive(page, bbox, text),
            page,
            order,
            bbox,
            column: 0,
            kind_hint: BlockKindHint::Text,
            text: text.to_owned(),
            lines: vec![line],
            column_width_pt: 300.0,
            space_above_pt: 0.0,
            page_height_pt: 600.0,
            para_starts: Vec::new(),
            continues: None,
        }
    }

    /// A book of `chapters` chapters, each opening with `label(n)` sunk down a fresh page and
    /// running on for three pages of body text that starts at the top.
    fn book(chapters: u32, label: impl Fn(u32) -> String) -> Vec<BlockView> {
        let mut blocks = Vec::new();
        let mut order = 0;
        for chapter in 1..=chapters {
            let first = (chapter - 1) * 3;
            blocks.push(block(first, order, 150.0, &label(chapter), 10.0));
            order += 1;
            blocks.push(block(
                first,
                order,
                200.0,
                "The body of the chapter, running on at length.",
                300.0,
            ));
            order += 1;
            for page in first + 1..first + 3 {
                blocks.push(block(
                    page,
                    order,
                    50.0,
                    "more body text, set full out across the measure.",
                    300.0,
                ));
                order += 1;
            }
        }
        blocks
    }

    #[test]
    fn bare_numbers_sunk_down_their_pages_are_chapter_openers() {
        let blocks = book(5, |n| n.to_string());
        let openers = chapter_openers(&blocks, 10.0, &T);
        assert_eq!(openers.len(), 5, "{openers:#?}");
        assert_eq!(openers[0].text, "1");
        assert_eq!(openers[4].value, Some(5));
    }

    #[test]
    fn roman_numerals_and_a_word_beside_them_count_whatever_the_language() {
        for word in ["Chapter", "BÖLÜM", "Kapitel", "章"] {
            let roman = ["I", "II", "III", "IV", "V", "VI"];
            let blocks = book(6, |n| format!("{word} {}", roman[(n - 1) as usize]));
            let openers = chapter_openers(&blocks, 10.0, &T);
            assert_eq!(openers.len(), 6, "{word}: {openers:#?}");
        }
    }

    #[test]
    fn a_misread_numeral_between_two_good_ones_is_taken() {
        let labels = ["I", "II", "Ill", "IV", "V"];
        let blocks = book(5, |n| labels[(n - 1) as usize].to_owned());
        let openers = chapter_openers(&blocks, 10.0, &T);
        assert_eq!(openers.len(), 5, "{openers:#?}");
    }

    #[test]
    fn numbers_that_do_not_count_up_are_not_chapters() {
        let values = [3, 17, 4, 12, 9];
        let blocks = book(5, |n| values[(n - 1) as usize].to_string());
        assert!(chapter_openers(&blocks, 10.0, &T).is_empty());
    }

    #[test]
    fn a_number_at_the_usual_top_of_every_page_is_a_folio_not_a_chapter() {
        let mut blocks = Vec::new();
        for page in 0..20u32 {
            blocks.push(block(page, page * 2, 50.0, &(page + 1).to_string(), 10.0));
            blocks.push(block(
                page,
                page * 2 + 1,
                70.0,
                "body text across the whole measure here.",
                300.0,
            ));
        }
        assert!(chapter_openers(&blocks, 10.0, &T).is_empty());
    }

    #[test]
    fn a_long_line_ending_in_a_number_is_not_an_opener() {
        let blocks = book(5, |n| {
            format!("the year the war ended was nineteen forty {n}")
        });
        assert!(chapter_openers(&blocks, 10.0, &T).is_empty());
    }
}
