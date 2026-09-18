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

use oc_epub::EpubBytes;
use oc_model::doc::{Severity, Warning};
use oc_model::extract::CharHistogram;
use oc_model::ledger::{c_of, Ledger};

/// Character retention fell below `validate.min_char_retention`.
pub const W_LOW_RETENTION: &str = "W_LOW_RETENTION";

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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I7Result {
    /// `C_0 ⊎ Added_all` minus `C(EPUB) ⊎ Removed_all`: text that went missing unaccounted for.
    pub missing: CharHistogram,
    /// The other direction: text in the container that nothing put there.
    pub extra: CharHistogram,
    /// `|C(EPUB)|`.
    pub epub_chars: u64,
    /// `|C_0|`, the retention denominator (ARCHITECTURE §5.2).
    pub c0_chars: u64,
}

impl I7Result {
    /// Whether `C(EPUB) ⊎ chars(all Removed) == C_0 ⊎ chars(all Added)`.
    pub fn holds(&self) -> bool {
        self.missing.is_empty() && self.extra.is_empty()
    }

    /// `|C(EPUB)| / |C_0|` — the headline number PIPELINE §13 shows the user.
    pub fn retention(&self) -> f32 {
        if self.c0_chars == 0 {
            return 0.0;
        }
        self.epub_chars as f32 / self.c0_chars as f32
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
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Rows 6.2 and 6.3 of the Phase 6 table;
// row 6.1 is the same equation over real containers and lives in `openconvert`.
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
