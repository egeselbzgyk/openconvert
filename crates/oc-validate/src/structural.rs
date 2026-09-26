//! The structural validator: invariant I-7, character retention, and the checks that are about
//! the *book* rather than the container (D7, R6 §11, PIPELINE §11).
//!
//! Tier 1 asks whether the archive is a well-formed EPUB. This asks whether it is the book that
//! went in. The two questions are independent: a container can satisfy every EPUB 3.3 rule and
//! be missing a chapter, and I-7 is the only check in the project that would notice.
//!
//! **I-7 is measured over the archive, not over the emitter's output.** `oc-core`'s `epub` stage
//! already checks the same equation against `BuiltEpub::emitted`, which is the emitter's own
//! account of what it serialised; this reads the zip back, resolves the spine through the
//! package document, and parses each content document. The two agree today. When they stop
//! agreeing, the bug is between the emitter and the zip writer — which is precisely the gap a
//! release gate is for.

use std::collections::BTreeMap;

use oc_core::thresholds::Thresholds;
use oc_epub::EpubBytes;
use oc_model::doc::{Severity, Warning};
use oc_model::document::Document;
use oc_model::extract::CharHistogram;
use oc_model::ledger::{c_of, Ledger};
use oc_text::stats::{quality_stats, QualityStats, Region};

use crate::blocks::{blocks_of, Block};
use crate::tier1::Tier1Report;

/// Character retention fell below `validate.min_char_retention`.
pub const W_LOW_RETENTION: &str = "W_LOW_RETENTION";
/// The heading tree jumps a level: an `h3` directly under an `h1`.
pub const W_HEADING_LEVEL_SKIP: &str = "W_HEADING_LEVEL_SKIP";
/// The number of top-level headings is outside `validate.h1_count_min … h1_count_max`.
pub const W_HEADING_COUNT_IMPLAUSIBLE: &str = "W_HEADING_COUNT_IMPLAUSIBLE";
/// The headings do not run in page order.
pub const W_HEADINGS_OUT_OF_PAGE_ORDER: &str = "W_HEADINGS_OUT_OF_PAGE_ORDER";
/// More of the container's blocks repeat than `validate.dup_block_frac` allows.
pub const W_DUPLICATE_BLOCKS: &str = "W_DUPLICATE_BLOCKS";

/// Why the structural validator could not run.
///
/// Not a finding: a container Tier 1 has already rejected as unreadable cannot be measured
/// against the document it came from, and reporting "retention 0.0" for it would be a lie in the
/// most alarming direction.
#[derive(Debug, thiserror::Error)]
pub enum StructuralError {
    /// Carried as text rather than as `zip::result::ZipError`, so that `oc-validate` does not
    /// take a direct dependency on the zip crate to name a type it only ever prints.
    #[error("the container is not a readable zip: {0}")]
    Archive(String),
    #[error("the container has no package document reachable from META-INF/container.xml")]
    NoPackage,
    #[error("a content document could not be read back: {0}")]
    Reparse(#[from] oc_epub::textcontent::TextError),
}

/// What I-7 found.
///
/// Both differences are carried, not a boolean: "the book lost 412 characters" is actionable and
/// "I-7 failed" is not. `missing` is the multiset the source had and the output does not account
/// for — the direction that loses a reader's text — and `extra` is the other one.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct I7Result {
    /// `C_0 ⊎ Added_all` minus `C(EPUB) ⊎ Removed_all`: text that went missing unaccounted for.
    pub missing: CharHistogram,
    /// The other direction: text in the container that nothing put there.
    pub extra: CharHistogram,
    /// `|C(EPUB)|`.
    pub epub_chars: u64,
    /// `|C_0|`, the retention denominator (ARCHITECTURE §5.2).
    pub c0_chars: u64,
    /// What OCR added to the book (`Reason::Ocr`). Part of I-7's equation like every addition, and
    /// taken out of the retention numerator: text read off pixels is not text retained from the
    /// document (I-6, RT C1). Omitted from the report when zero, which is every born-digital book.
    #[serde(skip_serializing_if = "is_zero")]
    pub ocr_chars: u64,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

impl I7Result {
    /// Whether `C(EPUB) ⊎ chars(all Removed) == C_0 ⊎ chars(all Added)`.
    pub fn holds(&self) -> bool {
        self.missing.is_empty() && self.extra.is_empty()
    }

    /// `(|C(EPUB)| − |OCR-added|) / |C_0|` — the headline number PIPELINE §13 shows the user. OCR
    /// text is in neither side, so an OCR'd page can never be scored as if it had been extracted.
    pub fn retention(&self) -> f32 {
        if self.c0_chars == 0 {
            return 0.0;
        }
        self.epub_chars.saturating_sub(self.ocr_chars) as f32 / self.c0_chars as f32
    }
}

/// `C(EPUB)`: the non-whitespace scalars of every **spine** document's `<body>`.
///
/// The spine and not the manifest, because `nav.xhtml` is a content document by media type and
/// its text is nav metadata — outside `C` by ARCHITECTURE §5.2, and counting it would credit the
/// book twice for every chapter title.
pub fn epub_chars(epub: &EpubBytes) -> Result<CharHistogram, StructuralError> {
    let entries =
        oc_epub::read_entries(epub).map_err(|error| StructuralError::Archive(error.to_string()))?;
    let package = crate::tier1::opf::parse(&entries).ok_or(StructuralError::NoPackage)?;

    let mut chars = CharHistogram::new();
    for path in package.spine_paths() {
        let Some(bytes) = entries.get(&path) else {
            continue;
        };
        let markup = String::from_utf8_lossy(bytes);
        chars = chars.union(&c_of(&oc_epub::textcontent::body_text(&markup)?));
    }
    Ok(chars)
}

/// Check I-7 against a ledger, given `C(EPUB)` already measured.
///
/// Split from [`epub_chars`] so that the equation is testable without a container: the mutation
/// test deletes a paragraph from a histogram and asserts the exact multiset that comes back.
pub fn check_i7(epub: &CharHistogram, ledger: &Ledger) -> I7Result {
    let left = epub.union(&ledger.removed_all());
    let right = ledger.c_0.union(&ledger.added_all());
    I7Result {
        missing: right.difference(&left),
        extra: left.difference(&right),
        epub_chars: epub.total(),
        c0_chars: ledger.c_0.total(),
        ocr_chars: ledger.ocr_added().total(),
    }
}

/// The retention warning, if the measured ratio is under `floor`.
///
/// The floor is `validate.min_char_retention` and is passed in rather than read from
/// `oc_core::thresholds::T`: `oc-validate` may depend on `oc-model` and `oc-epub` and nothing
/// else (ARCHITECTURE §3.1), and `oc-core` is the crate that would close the cycle.
///
/// A `Vec` rather than an `Option` because this is one of the report's warning sources and the
/// caller concatenates several; returning the same shape from each keeps the call site honest
/// about there being no privileged one.
pub fn retention_warnings(result: &I7Result, floor: f64) -> Vec<Warning> {
    // A book whose source carried no text has no retention ratio, and reporting 0.0 for one
    // would tell a reader that a scanned book lost everything. Appendix D states the retention
    // gate over the "non-scanned stratum" for the same reason.
    if result.c0_chars == 0 {
        return Vec::new();
    }
    let retention = f64::from(result.retention());
    if retention >= floor {
        return Vec::new();
    }
    vec![Warning::new(W_LOW_RETENTION, Severity::Warn)
        .with_arg("retention", format!("{retention:.3}"))
        .with_arg("floor", format!("{floor:.3}"))]
}

// ---------------------------------------------------------------------------
// Heading-tree sanity
// ---------------------------------------------------------------------------

/// What the heading tree looks like, as the container carries it.
///
/// Read out of the XHTML rather than off the `Document`, because the emitter clamps a level into
/// the range XHTML has and a split chapter's continuation borrows the first fragment's heading:
/// what a reader and a screen reader meet is the markup, and the markup is what a level skip is a
/// statement about.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct HeadingSanity {
    /// How many `h1`s the spine carries.
    pub h1_count: u32,
    /// Every place the level jumps down by more than one, as `path: h1 → h3`.
    pub level_skips: Vec<String>,
    /// Whether the headings run in source-page order.
    ///
    /// `None` when the document has no headings to order.
    pub monotone_with_pages: Option<bool>,
    /// Whether `h1_count` is inside the plausible range.
    ///
    /// `None` when the book is shorter than `validate.h1_count_min_pages`, where "a book has at
    /// least two chapters" is arithmetic about the page count rather than evidence about the
    /// detector.
    pub h1_count_plausible: Option<bool>,
}

impl HeadingSanity {
    /// Whether nothing is wrong: no level skip, page order kept, and a plausible `h1` count where
    /// the question applies.
    pub fn holds(&self) -> bool {
        self.level_skips.is_empty()
            && self.monotone_with_pages != Some(false)
            && self.h1_count_plausible != Some(false)
    }
}

/// Measure the heading tree.
///
/// `heading_pages` is the source page each heading opened on, in document order — taken as a slice
/// rather than as a `Document`, because a heading in the markup carries no page and the section
/// that produced it does. [`heading_source_pages`] is how a caller gets it.
pub fn heading_sanity(
    heading_pages: &[u32],
    blocks: &[Block],
    page_count: u32,
    t: &Thresholds,
) -> HeadingSanity {
    let headings: Vec<(&Block, u8)> = blocks
        .iter()
        .filter_map(|block| block.heading_level.map(|level| (block, level)))
        .collect();

    let h1_count =
        u32::try_from(headings.iter().filter(|(_, level)| *level == 1).count()).unwrap_or(u32::MAX);

    let mut level_skips = Vec::new();
    for pair in headings.windows(2) {
        let (from, to) = (pair[0].1, pair[1].1);
        if to > from + 1 {
            level_skips.push(format!("{}: h{from} → h{to}", pair[1].0.path));
        }
    }
    // A document whose first heading is not an `h1` has skipped from the root, which is the same
    // defect and the windows above cannot see it.
    if let Some((block, level)) = headings.first() {
        if *level > 1 {
            level_skips.push(format!("{}: → h{level} with no h1 above it", block.path));
        }
    }

    let monotone_with_pages = (!heading_pages.is_empty())
        .then(|| heading_pages.windows(2).all(|pair| pair[0] <= pair[1]));

    let long_enough_to_be_a_book =
        page_count >= u32::try_from(t.validate.h1_count_min_pages).unwrap_or(u32::MAX);
    let h1_count_plausible = long_enough_to_be_a_book.then(|| {
        let min = u32::try_from(t.validate.h1_count_min).unwrap_or(0);
        let max = u32::try_from(t.validate.h1_count_max).unwrap_or(u32::MAX);
        (min..=max).contains(&h1_count)
    });

    HeadingSanity {
        h1_count,
        level_skips,
        monotone_with_pages,
        h1_count_plausible,
    }
}

/// The source page each of a document's headings opened on, in document order.
///
/// Sections walk depth first, which is the order their headings are emitted in.
pub fn heading_source_pages(src: &Document) -> Vec<u32> {
    src.walk()
        .iter()
        .filter(|section| section.heading.is_some())
        .map(|section| section.source_pages.0)
        .collect()
}

// ---------------------------------------------------------------------------
// Duplicate detection
// ---------------------------------------------------------------------------

/// How much of the container repeats itself.
///
/// Stated over *blocks of the output* rather than over lines of the source, because the failure
/// mode it exists for is a pipeline that emitted the same paragraph twice — a table detector that
/// claimed text which stayed in the flow, a drop cap counted in two places. A reflowable book has
/// no lines, so the Gopher line statistic has nothing to say about it; the paragraph statistic is
/// in [`StructuralReport::quality`] and is the same family measured over the whole text.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct DuplicateStats {
    /// How many blocks the spine carries.
    pub blocks: u32,
    /// How many of them are a repeat of an earlier one: a block seen `k` times contributes `k−1`.
    pub duplicates: u32,
    /// The worst offenders, longest first, at most ten, each with how many times it appears.
    pub worst: Vec<(String, u32)>,
}

impl DuplicateStats {
    /// `duplicates / blocks`.
    pub fn frac(&self) -> f32 {
        if self.blocks == 0 {
            return 0.0;
        }
        f64::from(self.duplicates) as f32 / f64::from(self.blocks) as f32
    }
}

/// Count repeated blocks.
pub fn duplicate_stats(blocks: &[Block]) -> DuplicateStats {
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for block in blocks {
        *counts.entry(block.text.as_str()).or_default() += 1;
    }

    let duplicates = counts.values().map(|count| count.saturating_sub(1)).sum();

    // Longest first, because a repeated chapter is the finding and a repeated "Notes" heading is
    // noise; ties broken by the text so the report is the same on two runs.
    let mut worst: Vec<(String, u32)> = counts
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(text, count)| ((*text).to_owned(), *count))
        .collect();
    worst.sort_by(|a, b| {
        b.0.chars()
            .count()
            .cmp(&a.0.chars().count())
            .then_with(|| a.0.cmp(&b.0))
    });
    worst.truncate(10);

    DuplicateStats {
        blocks: u32::try_from(blocks.len()).unwrap_or(u32::MAX),
        duplicates,
        worst,
    }
}

// ---------------------------------------------------------------------------
// The whole report
// ---------------------------------------------------------------------------

/// Everything the structural validator measured about one conversion.
///
/// `image_parity`, `note_bijection` and `hrefs_resolve` are **projections of the Tier-1 report**,
/// not second implementations. All three are checks Tier 1 already runs over the archive
/// (PIPELINE §11 lists them there), and ARCHITECTURE §7.2 lists them again under the structural
/// validator because the structural report is what the user is shown. Two implementations of one
/// bijection would be two chances to get it wrong.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct StructuralReport {
    pub i7: I7Result,
    /// `|C(EPUB)| / |C_0|`, carried explicitly because the report prints it as the headline
    /// number (PIPELINE §13).
    pub retention: f32,
    pub image_parity: bool,
    pub note_bijection: bool,
    pub hrefs_resolve: bool,
    pub heading_sanity: HeadingSanity,
    pub duplicates: DuplicateStats,
    /// The Gopher/MassiveText statistics over the container's text (R10 §6.18).
    pub quality: QualityStats,
    pub warnings: Vec<Warning>,
}

impl StructuralReport {
    /// Whether every structural check passed. I-7 is the release gate and the rest are flags, so
    /// this is the *conjunction* a report prints and not a conversion verdict.
    pub fn holds(&self) -> bool {
        self.i7.holds()
            && self.image_parity
            && self.note_bijection
            && self.hrefs_resolve
            && self.heading_sanity.holds()
    }
}

/// The message ids whose presence in a Tier-1 report means a reference does not resolve.
const UNRESOLVED_IDS: [&str; 2] = ["RSC-007", "RSC-012"];

/// Run every structural check over one finished conversion.
///
/// `tier1` is taken rather than run, because ARCHITECTURE §7.2 fixes the order — Tier 1 first,
/// because it is milliseconds and catches the structural classes — and because three of this
/// report's fields are its findings read a second way.
///
/// `C_0` comes off `src.ledger` rather than as a separate argument as the plan's sketch has it:
/// the `Document` already carries the baseline every stage was checked against, and a second copy
/// passed alongside it is a second copy that can disagree.
pub fn validate_structural(
    src: &Document,
    epub: &EpubBytes,
    tier1: &Tier1Report,
    page_count: u32,
    t: &Thresholds,
) -> Result<StructuralReport, StructuralError> {
    let entries =
        oc_epub::read_entries(epub).map_err(|error| StructuralError::Archive(error.to_string()))?;
    let package = crate::tier1::opf::parse(&entries).ok_or(StructuralError::NoPackage)?;

    let mut chars = CharHistogram::new();
    let mut blocks: Vec<Block> = Vec::new();
    for path in package.spine_paths() {
        let Some(bytes) = entries.get(&path) else {
            continue;
        };
        let markup = String::from_utf8_lossy(bytes);
        chars = chars.union(&c_of(&oc_epub::textcontent::body_text(&markup)?));
        blocks.extend(blocks_of(&path, &markup)?);
    }

    let i7 = check_i7(&chars, &src.ledger);
    let heading = heading_sanity(&heading_source_pages(src), &blocks, page_count, t);
    let duplicates = duplicate_stats(&significant_blocks(&blocks, t));

    // One block per paragraph, joined by a blank line, is what makes `dup_para_frac` and the
    // n-gram shares mean over a reflowable book what they mean over a page of a source PDF.
    let text = blocks
        .iter()
        .map(|block| block.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    // `dict_hit_rate` is `None`, not zero: the frequency list is the source text's oracle and a
    // container is not where a missing list should be reported. `oc-text`'s own docs make the
    // distinction load-bearing.
    let quality = quality_stats(&text, src.language.clone(), Region::Document, None);

    let mut warnings = retention_warnings(&i7, t.validate.min_char_retention);
    warnings.extend(heading_warnings(&heading, t));
    warnings.extend(duplicate_warnings(&duplicates, t));

    Ok(StructuralReport {
        retention: i7.retention(),
        i7,
        image_parity: !tier1.has("OC-IMAGE-PARITY"),
        note_bijection: !tier1.has("OC-NOTE-BIJECTION") && !tier1.has("RSC-012"),
        hrefs_resolve: !UNRESOLVED_IDS.iter().any(|id| tier1.has(id)),
        heading_sanity: heading,
        duplicates,
        quality,
        warnings,
    })
}

fn heading_warnings(sanity: &HeadingSanity, t: &Thresholds) -> Vec<Warning> {
    let mut warnings = Vec::new();
    if !sanity.level_skips.is_empty() {
        warnings.push(
            Warning::new(W_HEADING_LEVEL_SKIP, Severity::Warn)
                .with_arg("count", sanity.level_skips.len().to_string())
                .with_arg(
                    "first",
                    sanity.level_skips.first().cloned().unwrap_or_default(),
                ),
        );
    }
    if sanity.h1_count_plausible == Some(false) {
        warnings.push(
            Warning::new(W_HEADING_COUNT_IMPLAUSIBLE, Severity::Warn)
                .with_arg("count", sanity.h1_count.to_string())
                .with_arg("min", t.validate.h1_count_min.to_string())
                .with_arg("max", t.validate.h1_count_max.to_string()),
        );
    }
    if sanity.monotone_with_pages == Some(false) {
        warnings.push(Warning::new(W_HEADINGS_OUT_OF_PAGE_ORDER, Severity::Warn));
    }
    warnings
}

/// The blocks a repeat among means something: at least `validate.dup_block_min_chars` long, and
/// not a table cell. A book legitimately repeats short blocks — an index entry under two parents,
/// a contents line that is its chapter's title, a "Yes" in a table, a line of code — and a block
/// the converter emitted twice is a paragraph (2026-09-26: up to one block in ten repeated in
/// technical books, every one of them short).
fn significant_blocks(blocks: &[Block], t: &Thresholds) -> Vec<Block> {
    let min = usize::try_from(t.validate.dup_block_min_chars.max(0)).unwrap_or(usize::MAX);
    blocks
        .iter()
        .filter(|block| !matches!(block.tag.as_str(), "td" | "th"))
        .filter(|block| block.text.chars().count() >= min)
        .cloned()
        .collect()
}

fn duplicate_warnings(duplicates: &DuplicateStats, t: &Thresholds) -> Vec<Warning> {
    if f64::from(duplicates.frac()) <= t.validate.dup_block_frac {
        return Vec::new();
    }
    vec![Warning::new(W_DUPLICATE_BLOCKS, Severity::Warn)
        .with_arg("fraction", format!("{:.3}", duplicates.frac()))
        .with_arg("bound", format!("{:.3}", t.validate.dup_block_frac))
        .with_arg("blocks", duplicates.duplicates.to_string())
        .with_arg(
            "worst",
            duplicates
                .worst
                .first()
                .map(|(text, count)| format!("{count}× {}", truncate(text, 60)))
                .unwrap_or_default(),
        )]
}

/// The first `chars` characters, with an ellipsis when there were more — so a warning about a
/// repeated chapter does not put the chapter in the report.
fn truncate(text: &str, chars: usize) -> String {
    if text.chars().count() <= chars {
        return text.to_owned();
    }
    let kept: String = text.chars().take(chars).collect();
    format!("{kept}…")
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Rows 6.2, 6.3 and 6.16 of the Phase 6
// table; row 6.1 is the same equation over real containers and lives in `openconvert`.
// ---------------------------------------------------------------------------

#[cfg(test)]
use oc_model::ledger::{LedgerEntry, Reason};

/// A ledger whose `C_0` is `source` and which removed `removed` under one furniture reason.
#[cfg(test)]
fn ledger_of(source: &str, removed: &str) -> Ledger {
    let mut ledger = Ledger {
        c_raw: c_of(source),
        c_0: c_of(source),
        ..Ledger::default()
    };
    if !removed.is_empty() {
        ledger.entries.push(LedgerEntry::removed(
            "furniture",
            Reason::RunningHeader,
            0,
            (
                0,
                u32::try_from(removed.chars().count()).unwrap_or_default(),
            ),
            removed.to_owned(),
        ));
    }
    ledger
}

/// Row 6.2. The whole point of the invariant: a paragraph that leaves the output without a
/// ledger entry is named, exactly, by the multiset it took with it. "I-7 failed" would not be
/// enough to find it — the equation is over a whole book, and the difference is the evidence.
#[test]
fn i7_detects_injected_text_loss() {
    let source = "Call me Ishmael. Some years ago, never mind how long precisely.";
    let paragraph = "Some years ago, never mind how long precisely.";
    let ledger = ledger_of(source, "");

    // The emitted book, then the same book with one paragraph deleted from the XHTML.
    let whole = c_of(source);
    let held = check_i7(&whole, &ledger);
    assert!(held.holds(), "an untouched book satisfies I-7: {held:?}");
    assert!((held.retention() - 1.0).abs() < f32::EPSILON);

    let mutilated = whole.difference(&c_of(paragraph));
    let broken = check_i7(&mutilated, &ledger);
    assert!(!broken.holds(), "a deleted paragraph must fail I-7");
    assert_eq!(
        broken.missing,
        c_of(paragraph),
        "the missing multiset is exactly the paragraph that was deleted"
    );
    assert!(
        broken.extra.is_empty(),
        "nothing was added: {:?}",
        broken.extra
    );
}

/// A removal the ledger accounts for is not a loss. The furniture stage deletes text by design,
/// and an I-7 that could not tell the two apart would be an invariant no book could satisfy.
#[test]
fn i7_accepts_a_ledgered_removal() {
    let source = "Chapter One. THE WHALE 7";
    let ledger = ledger_of(source, "THE WHALE 7");
    let emitted = c_of(source).difference(&c_of("THE WHALE 7"));

    let result = check_i7(&emitted, &ledger);
    assert!(result.holds(), "{result:?}");
    assert!(
        result.retention() < 1.0,
        "furniture removal lowers retention"
    );
}

/// Row 6.3. Retention below `validate.min_char_retention` is a warning carrying the measured
/// value, not a bare flag: a user who is told "some text may be missing" cannot act, and one
/// who is told "95.0 % of the text reached the book" can.
#[test]
fn retention_below_threshold_warns() {
    // A hundred characters of source, five of which never reach the container and are ledgered
    // as furniture — 0.95 against the 0.98 floor.
    let source: String = std::iter::repeat_n('a', 100).collect();
    let removed: String = std::iter::repeat_n('a', 5).collect();
    let ledger = ledger_of(&source, &removed);
    let emitted = c_of(&source).difference(&c_of(&removed));

    let result = check_i7(&emitted, &ledger);
    assert!(result.holds(), "the removal is ledgered: {result:?}");
    assert!((result.retention() - 0.95).abs() < 1e-6, "{result:?}");

    // 0.98 is `validate.min_char_retention`, stated here because a test may hold a literal and
    // the production call site may not.
    let warnings = retention_warnings(&result, 0.98);
    let warning = warnings
        .iter()
        .find(|warning| warning.code == W_LOW_RETENTION)
        .expect("0.95 is below the 0.98 floor");
    assert_eq!(
        warning.args.get("retention").map(String::as_str),
        Some("0.950"),
        "the measured value travels with the warning: {:?}",
        warning.args
    );
    assert_eq!(
        warning.args.get("floor").map(String::as_str),
        Some("0.980"),
        "and so does the floor it was measured against: {:?}",
        warning.args
    );

    // The other direction: a book that kept everything says nothing.
    let whole = check_i7(&c_of(&source), &ledger_of(&source, ""));
    assert!(retention_warnings(&whole, 0.98).is_empty());
}

/// A book whose pages carry no extractable text at all — `f03`, and every scanned book until
/// Phase 13 — has no retention ratio. Reporting 0.0 would say it lost everything, which is the
/// most alarming possible way to describe a book that never had source text to lose.
#[test]
fn a_book_with_no_source_text_has_no_retention_to_report() {
    let empty = Ledger::default();
    let result = check_i7(&CharHistogram::new(), &empty);

    assert!(result.holds(), "nothing in, nothing out: {result:?}");
    assert_eq!(result.c0_chars, 0);
    assert!(retention_warnings(&result, 0.98).is_empty());
}

/// Row 13.15's end-to-end half: a book with an OCR'd plate beside its extracted text keeps a
/// retention of 1 — the plate's characters are in I-7's equation (as an addition) and in neither
/// side of the ratio.
#[test]
fn retention_excludes_ocr_added_characters() {
    use oc_model::ledger::{LedgerDelta, LedgerEntry, Reason};

    let source = "Extracted text of the born-digital pages";
    let plate = "Read off a scanned plate";
    let mut ledger = ledger_of(source, "");
    ledger.push_stage(
        &LedgerDelta::new(vec![LedgerEntry::added(
            "ingest",
            Reason::Ocr,
            0,
            (0, 24),
            plate.to_owned(),
        )]),
        oc_model::ledger::StageCheck {
            stage: "ingest",
            kind: oc_model::ledger::StageKind::Budgeted,
            removed_chars: 0,
            added_chars: c_of(plate).total(),
            retention: 1.0,
        },
    );
    let emitted = c_of(&format!("{source} {plate}"));
    let result = check_i7(&emitted, &ledger);

    assert!(
        result.holds(),
        "OCR text is accounted for by its entry: {result:?}"
    );
    assert_eq!(result.c0_chars, c_of(source).total());
    assert_eq!(result.ocr_chars, c_of(plate).total());
    assert!(
        (result.retention() - 1.0).abs() < 1e-6,
        "OCR text inflates neither side: {}",
        result.retention()
    );
    assert!(retention_warnings(&result, 0.98).is_empty());
}

#[cfg(test)]
fn block(path: &str, tag: &str, text: &str) -> Block {
    Block {
        path: path.to_owned(),
        heading_level: tag
            .strip_prefix('h')
            .and_then(|rest| rest.parse::<u8>().ok())
            .filter(|level| (1..=6).contains(level)),
        tag: tag.to_owned(),
        text: text.to_owned(),
    }
}

/// A level skip is what a screen reader trips over, and it is a claim about the *markup*: an `h3`
/// directly under an `h1` is one whether the pipeline meant it or not.
#[test]
fn a_heading_level_skip_is_found_and_located() {
    let t = &oc_core::thresholds::T;
    let blocks = vec![
        block("t/c1.xhtml", "h1", "One"),
        block("t/c1.xhtml", "p", "Body"),
        block("t/c2.xhtml", "h3", "Skipped"),
        block("t/c2.xhtml", "h4", "Fine"),
    ];

    let sanity = heading_sanity(&[0, 1, 2], &blocks, 3, t);
    assert_eq!(sanity.h1_count, 1);
    assert_eq!(sanity.level_skips.len(), 1, "{sanity:?}");
    assert!(
        sanity.level_skips[0].contains("t/c2.xhtml") && sanity.level_skips[0].contains("h1 → h3"),
        "the skip names where it is: {:?}",
        sanity.level_skips
    );
    assert!(!sanity.holds());
}

/// A document whose first heading is an `h2` has skipped from the root. The pairwise walk cannot
/// see it — there is no earlier heading to compare against — and a reader meets exactly the same
/// defect.
#[test]
fn a_document_that_starts_below_h1_has_skipped_a_level() {
    let t = &oc_core::thresholds::T;
    let blocks = vec![block("t/c1.xhtml", "h2", "Second level, first heading")];

    let sanity = heading_sanity(&[0], &blocks, 1, t);
    assert_eq!(sanity.h1_count, 0);
    assert_eq!(sanity.level_skips.len(), 1, "{sanity:?}");
    assert!(sanity.level_skips[0].contains("no h1 above it"));
}

/// Headings run in page order. The `document` stage can reorder sections that `structure` checked,
/// so the claim is re-measured over the finished tree.
#[test]
fn headings_out_of_page_order_are_reported() {
    let t = &oc_core::thresholds::T;
    let blocks = vec![
        block("t/c1.xhtml", "h1", "One"),
        block("t/c2.xhtml", "h1", "Two"),
    ];

    let forward = heading_sanity(&[0, 4], &blocks, 8, t);
    assert_eq!(forward.monotone_with_pages, Some(true));

    let backward = heading_sanity(&[4, 0], &blocks, 8, t);
    assert_eq!(backward.monotone_with_pages, Some(false));
    assert!(!backward.holds());
}

/// The plausible `h1` range is stated "for a book", and a two-page fixture is not one. Below
/// `validate.h1_count_min_pages` the question is arithmetic about the page count rather than
/// evidence about the detector, and the answer is `None` rather than a failure.
#[test]
fn the_h1_count_range_says_nothing_about_a_document_too_short_to_be_a_book() {
    let t = &oc_core::thresholds::T;
    let one_heading = vec![block("t/c1.xhtml", "h1", "The only chapter")];

    let short = heading_sanity(&[0], &one_heading, 2, t);
    assert_eq!(short.h1_count_plausible, None, "{short:?}");
    assert!(short.holds(), "a two-page fixture is not implausible");

    let long = heading_sanity(&[0], &one_heading, 300, t);
    assert_eq!(
        long.h1_count_plausible,
        Some(false),
        "one h1 in a 300-page book is a chapter detector that collapsed: {long:?}"
    );
    assert!(!long.holds());

    let plenty: Vec<Block> = (0..30)
        .map(|index| block("t/c1.xhtml", "h1", &format!("Chapter {index}")))
        .collect();
    let pages: Vec<u32> = (0..30).collect();
    let sane = heading_sanity(&pages, &plenty, 300, t);
    assert_eq!(sane.h1_count_plausible, Some(true), "{sane:?}");
}

/// Row 6.16, in its deterministic half. A block emitted twice is a pipeline bug — a table
/// detector that claimed text which stayed in the flow — and the statistic names the worst
/// offender rather than only its own value.
#[test]
fn a_repeated_block_is_counted_and_named() {
    let repeated = "A paragraph long enough to be the worst offender in the report.";
    let blocks = vec![
        block("t/c1.xhtml", "p", repeated),
        block("t/c1.xhtml", "p", "Something else"),
        block("t/c2.xhtml", "p", repeated),
        block("t/c2.xhtml", "h1", "Notes"),
        block("t/c3.xhtml", "h1", "Notes"),
    ];

    let stats = duplicate_stats(&blocks);
    assert_eq!(stats.blocks, 5);
    assert_eq!(stats.duplicates, 2, "{stats:?}");
    assert!((stats.frac() - 0.4).abs() < 1e-6, "{stats:?}");
    assert_eq!(
        stats
            .worst
            .first()
            .map(|(text, count)| (text.as_str(), *count)),
        Some((repeated, 2)),
        "the longest repeat is the one worth reporting: {:?}",
        stats.worst
    );

    let t = &oc_core::thresholds::T;
    let warnings = duplicate_warnings(&stats, t);
    assert_eq!(warnings.len(), 1, "0.4 is over the bound");
    assert_eq!(warnings[0].code, W_DUPLICATE_BLOCKS);

    // A book that repeats nothing says nothing.
    let unique = duplicate_stats(&blocks[..2]);
    assert_eq!(unique.duplicates, 0);
    assert!(duplicate_warnings(&unique, t).is_empty());
}

/// A book's short repeats — index entries, contents lines, table cells — are not what the
/// duplicate check is for, and a paragraph emitted twice still is.
#[test]
fn only_long_blocks_outside_tables_count_as_repeats() {
    let t = &oc_core::thresholds::T;
    let long = "A paragraph long enough that emitting it twice can only be the converter's doing.";
    let blocks = vec![
        block("t/c1.xhtml", "p", "evaluation pipeline design, 200-208"),
        block("t/c1.xhtml", "p", "evaluation pipeline design, 200-208"),
        block("t/c1.xhtml", "td", "Yes"),
        block("t/c1.xhtml", "td", "Yes"),
        block("t/c2.xhtml", "p", long),
    ];
    let quiet = duplicate_stats(&significant_blocks(&blocks, t));
    assert_eq!(quiet.duplicates, 0, "{quiet:?}");

    let mut twice = blocks.clone();
    twice.push(block("t/c3.xhtml", "p", long));
    let loud = duplicate_stats(&significant_blocks(&twice, t));
    assert_eq!(loud.duplicates, 1, "{loud:?}");
    assert_eq!(duplicate_warnings(&loud, t).len(), 1);
}
