//! Which tool made this PDF (D13.10, D18, IMPLEMENTATION_PLAN Phase 0 detail 4).
//!
//! The producer is recorded in the report, picks a parameter preset, and is the axis the
//! corpus is stratified along (D18) — a score that looks good only because the corpus is
//! mostly our own renderers is not a score at all.

use std::sync::OnceLock;

use regex::RegexSet;
use serde::{Deserialize, Serialize};

/// The tool that produced a PDF, as far as its metadata admits.
///
/// The spelling of each variant is a committed interface: it appears in `inspect --json`,
/// in the conversion report, and as the stratum key the corpus is reported by (D18).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProducerFamily {
    /// Any TeX engine that writes PDF directly.
    #[serde(rename = "pdfTeX")]
    PdfTeX,
    InDesign,
    Word,
    Ghostscript,
    /// A scanner or a scan-processing tool: ABBYY, FineReader, Kofax and friends.
    Scanner,
    /// Our own fixture renderer (D18: `ours(Typst)`).
    Typst,
    /// Our own second synthetic renderer (D18: `ours(WeasyPrint)`).
    WeasyPrint,
    /// Anything printing through Chromium or Skia.
    Chromium,
    Unknown,
}

impl ProducerFamily {
    /// Every variant, so a test can prove its table covers all of them.
    pub const ALL: [ProducerFamily; 9] = [
        ProducerFamily::PdfTeX,
        ProducerFamily::InDesign,
        ProducerFamily::Word,
        ProducerFamily::Ghostscript,
        ProducerFamily::Scanner,
        ProducerFamily::Typst,
        ProducerFamily::WeasyPrint,
        ProducerFamily::Chromium,
        ProducerFamily::Unknown,
    ];
}

/// The ordered table of Phase 0 detail 4. Order matters: the first pattern that matches
/// wins, so a more specific tool must come before a more general one.
const PATTERNS: [(&str, ProducerFamily); 8] = [
    (r"(?i)pdftex|xetex|luatex", ProducerFamily::PdfTeX),
    (r"(?i)indesign", ProducerFamily::InDesign),
    (r"(?i)microsoft.*word|word for", ProducerFamily::Word),
    (r"(?i)ghostscript", ProducerFamily::Ghostscript),
    (
        r"(?i)abbyy|finereader|scanner|kofax",
        ProducerFamily::Scanner,
    ),
    // Anchored: a tool that merely mentions Typst in a longer string is not Typst.
    (r"(?i)^typst", ProducerFamily::Typst),
    (r"(?i)weasyprint", ProducerFamily::WeasyPrint),
    (r"(?i)chrom(e|ium)|skia", ProducerFamily::Chromium),
];

/// Identify the producing tool from `/Producer`, falling back to `/Creator`.
///
/// `/Producer` is the tool that wrote the file and `/Creator` the application the document
/// came from, so the producer is the better signal — but plenty of files leave it empty or
/// generic, and then the creator is all there is. A recognised producer is never overridden
/// by the creator.
///
/// Either string may come from the `/Info` dictionary or from XMP; this function does not
/// care which, so the caller is free to prefer whichever it trusts.
pub fn producer_family(producer: Option<&str>, creator: Option<&str>) -> ProducerFamily {
    match match_family(producer) {
        ProducerFamily::Unknown => match_family(creator),
        recognised => recognised,
    }
}

fn match_family(text: Option<&str>) -> ProducerFamily {
    let Some(text) = text.map(str::trim).filter(|t| !t.is_empty()) else {
        return ProducerFamily::Unknown;
    };
    // `RegexSet` reports every pattern that matched; taking the lowest index is exactly the
    // "first rule in the table wins" the plan specifies, in one pass over the string.
    patterns()
        .matches(text)
        .iter()
        .next()
        .and_then(|index| PATTERNS.get(index).map(|(_, family)| *family))
        .unwrap_or(ProducerFamily::Unknown)
}

fn patterns() -> &'static RegexSet {
    static SET: OnceLock<RegexSet> = OnceLock::new();
    SET.get_or_init(|| {
        // The patterns are compile-time constants in this file, so the only way this can
        // fail is an edit to `PATTERNS` that is not a valid regex. `unwrap` is banned
        // outside tests and `main`, so a debug build asserts and a release build degrades
        // to "everything is Unknown" rather than killing a conversion.
        let set = RegexSet::new(PATTERNS.iter().map(|(pattern, _)| *pattern));
        debug_assert!(
            set.is_ok(),
            "a producer pattern is not a valid regex: {set:?}"
        );
        set.unwrap_or_else(|_| RegexSet::empty())
    })
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 0.13 of the Phase 0 table.
// ---------------------------------------------------------------------------

#[test]
fn producer_family_table() {
    use crate::producer::{producer_family, ProducerFamily};

    // Nine real-world `/Producer` strings, one per variant. Every case is a string a PDF in
    // the wild actually carries, not a synthetic one that only exercises the regex.
    let cases = [
        ("pdfTeX-1.40.25", ProducerFamily::PdfTeX),
        ("XeTeX 0.999996", ProducerFamily::PdfTeX),
        ("LuaTeX-1.18.0", ProducerFamily::PdfTeX),
        ("Adobe InDesign 19.0 (Macintosh)", ProducerFamily::InDesign),
        ("Microsoft® Word for Microsoft 365", ProducerFamily::Word),
        ("GPL Ghostscript 10.02.1", ProducerFamily::Ghostscript),
        ("ABBYY FineReader 15", ProducerFamily::Scanner),
        ("Kofax Power PDF", ProducerFamily::Scanner),
        ("Typst 0.15.1", ProducerFamily::Typst),
        ("WeasyPrint 63.0", ProducerFamily::WeasyPrint),
        ("Skia/PDF m131", ProducerFamily::Chromium),
        ("Chromium", ProducerFamily::Chromium),
        ("Acme Publishing Suite 3", ProducerFamily::Unknown),
    ];
    for (producer, expected) in cases {
        assert_eq!(
            producer_family(Some(producer), None),
            expected,
            "producer {producer:?}"
        );
    }

    // All nine variants are reachable from this table, so adding a variant without a case
    // fails here rather than going untested.
    let covered: std::collections::BTreeSet<_> = cases.iter().map(|(_, family)| *family).collect();
    assert_eq!(
        covered.len(),
        ProducerFamily::ALL.len(),
        "every ProducerFamily variant needs a case: missing {:?}",
        ProducerFamily::ALL
            .iter()
            .filter(|f| !covered.contains(f))
            .collect::<Vec<_>>()
    );

    // `/Creator` is consulted only when `/Producer` says nothing useful, and never overrides
    // a producer that was recognised.
    assert_eq!(
        producer_family(None, Some("Adobe InDesign 19.0")),
        ProducerFamily::InDesign
    );
    assert_eq!(
        producer_family(Some("Acme Publishing Suite 3"), Some("Adobe InDesign 19.0")),
        ProducerFamily::InDesign
    );
    assert_eq!(
        producer_family(Some("pdfTeX-1.40.25"), Some("Adobe InDesign 19.0")),
        ProducerFamily::PdfTeX
    );
    assert_eq!(producer_family(None, None), ProducerFamily::Unknown);

    // Typst is the one anchored pattern, so a tool that merely mentions it is not Typst.
    assert_eq!(
        producer_family(Some("Converted from Typst by hand"), None),
        ProducerFamily::Unknown
    );
    // ...but leading whitespace must not defeat the anchor.
    assert_eq!(
        producer_family(Some("  Typst 0.15.1  "), None),
        ProducerFamily::Typst
    );

    // The family is what the inspect JSON and the corpus manifest carry, so its spelling is
    // part of a committed interface.
    assert_eq!(
        serde_json::to_string(&ProducerFamily::PdfTeX).expect("serialises"),
        "\"pdfTeX\""
    );
    assert_eq!(
        serde_json::to_string(&ProducerFamily::Typst).expect("serialises"),
        "\"Typst\""
    );
}
