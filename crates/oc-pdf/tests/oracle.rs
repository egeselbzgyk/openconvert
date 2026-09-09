//! Differential test against an independent PDF text extractor (test 1.18, R9 §B.5).
//!
//! Every other test in this crate asks whether extraction agrees with itself. This one asks
//! whether it agrees with a *different implementation*, which is the only kind of test that
//! can catch a whole class of shared assumption — the bug where PDFium and we are both wrong
//! in the same direction, and no fixture we write will ever say so.
//!
//! **`pdftotext` is never shipped.** Poppler and Xpdf are GPL, and D15 bans both from the
//! shipped tree; this is a CI-only oracle binary, invoked as a subprocess and compared
//! against. Nothing links it.
//!
//! **Gated behind a cargo feature, not a skip attribute.** CLAUDE.md bans marking a test as
//! skipped, and `xtask ci-lint` enforces that, for a good reason: a skipped test reads as a
//! green one. The plan suggests running this "only when `pdftotext` is on PATH", but a test
//! that silently passes when its oracle is missing is the same failure in another costume. So
//! the whole file is `#![cfg(feature = "poppler-oracle")]`: off, it does not exist and claims
//! nothing; on, it must find the binary or fail. The `poppler-oracle` CI job turns it on.

#![cfg(feature = "poppler-oracle")]

use std::process::Command;

use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use unicode_normalization::UnicodeNormalization;

/// The three characters a line-break hyphen arrives as, folded to one for comparison.
///
/// This test found the reason they need folding. On `f02`, `pdftotext` reports `projec` +
/// **U+00AD** - a soft hyphen, because Typst hyphenated `projection` automatically - while
/// PDFium reports the same position as **U+0002**. On `f01`, where the author typed a real
/// `pipe-` in the source, `pdftotext` reports **U+002D** and PDFium reports **U+0002** again.
///
/// So PDFium collapses a soft hyphen and a hard one into a single marker, and `is_hyphen()`
/// only says *that* a character is a hyphen, never *which*. That is a real loss and it is
/// recorded in `docs/DECISIONS_LOG.md`; it is not this test's business. Test 1.18 asks whether
/// any text went missing, and folding the three forms is what keeps it asking that rather than
/// re-reporting an encoding difference as a lost word.
const HYPHEN_FORMS: [char; 3] = ['\u{2}', '\u{ad}', '-'];

/// What they fold to.
const HYPHEN: char = '-';

/// Test 1.18.
///
/// Containment, not equality. `pdftotext` reconstructs its own spacing and line breaks and we
/// do not reconstruct any yet — words are Phase 2's — so the assertion is that every word the
/// oracle found is *present* in our character stream. Anything the oracle sees and we do not
/// is text we would have dropped from the book.
#[test]
fn differential_pdftotext_coverage_f01() {
    assert_oracle_coverage("f01_prose_single_column");
}

/// The same comparison over the two-column fixture, which is where it can actually differ.
///
/// `f01` is one column of prose, so both extractors walk it the same way. `f02` has two
/// columns and two words hyphenated across line breaks, which is where an extractor's own
/// idea of reading order starts to matter — and where a shared assumption between PDFium and
/// us would be most likely to show up as a word only the oracle found.
#[test]
fn differential_pdftotext_coverage_f02() {
    assert_oracle_coverage("f02_two_column");
}

fn assert_oracle_coverage(name: &str) {
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(format!("{name}.pdf"));
    assert!(
        fixture.is_file(),
        "missing {}; run `cargo run -p xtask -- fixtures`",
        fixture.display()
    );

    let oracle = Command::new("pdftotext")
        .arg("-enc")
        .arg("UTF-8")
        .arg(&fixture)
        .arg("-")
        .output()
        .expect(
            "the poppler-oracle feature is on, so `pdftotext` must be installed; \
             the CI job installs poppler-utils",
        );
    assert!(
        oracle.status.success(),
        "pdftotext failed: {}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let oracle_text = String::from_utf8(oracle.stdout).expect("pdftotext was asked for UTF-8");

    let ours = our_character_stream(&fixture);

    // Not vacuous: both fixtures are pages of prose, so a comparison over a handful of words
    // would prove nothing whichever way it came out.
    let words: Vec<&str> = oracle_text.split_whitespace().collect();
    assert!(
        words.len() > 100,
        "pdftotext found only {} words in {name}",
        words.len()
    );

    let missing: Vec<&str> = words
        .iter()
        .copied()
        .filter(|word| !ours.contains(&normalise(word)))
        .collect();

    assert!(
        missing.is_empty(),
        "in {name}, pdftotext extracted {} words we did not: {:?}",
        missing.len(),
        // Enough to see the pattern, not so many that the failure is unreadable.
        missing.iter().take(20).collect::<Vec<_>>()
    );
}

/// Every character we extracted, from every page, normalised for comparison.
fn our_character_stream(fixture: &std::path::Path) -> String {
    let bytes = std::fs::read(fixture).expect("the fixture is readable");
    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let document = backend.open(&bytes, None).expect("the fixture opens");

    let mut text = String::new();
    for index in 0..document.page_count() {
        let page = document.page_glyphs(index).expect("the page extracts");
        text.extend(page.glyphs.iter().map(|glyph| glyph.ch));
    }
    normalise(&text)
}

/// The form both sides are compared in: NFC, PDFium's hyphen marker mapped back, whitespace
/// removed.
///
/// Whitespace goes because neither side's spacing is authoritative — `pdftotext` inserts its
/// own, and ours is whatever the content stream's positioning implies, which is precisely the
/// reconstruction Phase 2 exists to do. Comparing it here would be comparing two guesses.
fn normalise(text: &str) -> String {
    text.nfc()
        .map(|ch| {
            if HYPHEN_FORMS.contains(&ch) {
                HYPHEN
            } else {
                ch
            }
        })
        .filter(|ch| !ch.is_whitespace())
        .collect()
}
