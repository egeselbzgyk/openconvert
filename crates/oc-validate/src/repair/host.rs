//! The real [`Emit`](super::Emit): build the container, validate it, hash it.
//!
//! One iteration of the loop costs one `epub` regeneration plus one `validate` pass (PIPELINE §12),
//! and this is that pair. Tier 1 runs first because it is milliseconds and catches the structural
//! classes; the structural validator's findings are folded in after it, so that one table keyed by
//! message id covers both sources (ARCHITECTURE §7.2).
//!
//! The container hash is taken **with timestamps excluded**, which is what makes oscillation
//! detection a statement about the book rather than about the clock. Two things could carry a
//! timestamp: `dcterms:modified` in the package document, which is elided here, and the zip
//! entries' own mtimes, which the deterministic writer fixes for every build (D13.8). Eliding the
//! one and relying on the other being constant is deliberate — a caller that passed a different
//! `modified` on each iteration would otherwise make every container look new.

use oc_epub::images::SourceImage;
use oc_epub::{BuiltEpub, EpubBytes, EpubError, EpubOptions};
use oc_model::document::Document;

use super::{Emission, Emit};
use crate::structural::StructuralError;
use crate::tier1::{validate_tier1, Expectations, Finding};

/// Why one iteration could not be made.
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error(transparent)]
    Emit(#[from] EpubError),
    #[error(transparent)]
    Structural(#[from] StructuralError),
}

/// A host that emits through `oc-epub` and validates through Tier 1 and the structural validator.
pub struct EpubHost<'a> {
    images: &'a [SourceImage],
    options: &'a EpubOptions,
    expectations: Expectations,
    /// The container of the emission the loop last asked for, kept so the caller does not have to
    /// emit a third time to get the bytes it writes.
    last: Option<BuiltEpub>,
}

impl<'a> EpubHost<'a> {
    pub fn new(
        images: &'a [SourceImage],
        options: &'a EpubOptions,
        expectations: Expectations,
    ) -> Self {
        Self {
            images,
            options,
            expectations,
            last: None,
        }
    }

    /// The container the loop settled on, if it ever emitted one.
    pub fn take_built(&mut self) -> Option<BuiltEpub> {
        self.last.take()
    }
}

impl Emit for EpubHost<'_> {
    type Error = HostError;

    fn emit_and_validate(&mut self, document: &Document) -> Result<Emission, Self::Error> {
        let built = oc_epub::build_epub(document, self.images, self.options)?;

        let mut findings: Vec<Finding> = validate_tier1(&built.bytes, &self.expectations).findings;
        // The structural validator's own findings, keyed on the same vocabulary. Only the two it
        // can state as message ids — I-7 and the block-duplicate bound — because the rest of its
        // output is projections of Tier 1's findings and folding them back in would double-count.
        findings.extend(structural_findings(document, &built.bytes)?);

        let hash = content_hash(&built.bytes)?;
        self.last = Some(built);
        Ok(Emission { hash, findings })
    }
}

/// `OC-I7`: the end-to-end conservation gate failed.
pub const OC_I7: &str = "OC-I7";
/// The structural findings the repair table can be keyed on.
///
/// One, for now: I-7. It has no repair — a chapter that did not reach the container cannot be put
/// back by editing the document that produced it — and it is in the loop's finding list all the
/// same, because the measure is what decides whether the report says `invalid`, and a book that
/// lost a chapter must not be called a clean conversion because the container happened to be
/// well-formed. The rest of the structural report is either a projection of Tier 1's findings,
/// which would double-count, or a flag with a measured value rather than a message id.
fn structural_findings(
    document: &Document,
    epub: &EpubBytes,
) -> Result<Vec<Finding>, StructuralError> {
    use crate::tier1::Severity;

    let chars = crate::structural::epub_chars(epub)?;
    let i7 = crate::structural::check_i7(&chars, &document.ledger);

    let mut findings = Vec::new();
    if !i7.holds() {
        findings.push(Finding {
            id: OC_I7,
            severity: Severity::Fatal,
            location: String::new(),
            detail: format!(
                "I-7 fails: {} characters unaccounted for, {} unexplained additions",
                i7.missing.total(),
                i7.extra.total()
            ),
        });
    }
    Ok(findings)
}

/// The container's content hash, with `dcterms:modified` elided.
fn content_hash(epub: &EpubBytes) -> Result<[u8; 32], StructuralError> {
    let entries =
        oc_epub::read_entries(epub).map_err(|error| StructuralError::Archive(error.to_string()))?;

    let mut hasher = blake3::Hasher::new();
    for (path, bytes) in &entries {
        hasher.update(path.as_bytes());
        hasher.update(&[0]);
        let text = String::from_utf8_lossy(bytes);
        let stripped = strip_modified(&text);
        hasher.update(stripped.as_bytes());
        hasher.update(&[0]);
    }
    Ok(*hasher.finalize().as_bytes())
}

/// The `<meta property="dcterms:modified">…</meta>` element, removed.
///
/// Textual rather than a parse, because this is applied to every entry including the images: a
/// function that had to know which entries were XML would be a second, weaker copy of the manifest.
/// An image whose bytes happen to contain the marker would be hashed a few bytes short of itself,
/// which changes nothing — the hash only ever has to distinguish two containers from each other.
fn strip_modified(text: &str) -> String {
    const OPEN: &str = "<meta property=\"dcterms:modified\">";
    const CLOSE: &str = "</meta>";

    let Some(start) = text.find(OPEN) else {
        return text.to_owned();
    };
    let rest = &text[start + OPEN.len()..];
    let Some(end) = rest.find(CLOSE) else {
        return text.to_owned();
    };
    format!("{}{}", &text[..start], &rest[end + CLOSE.len()..])
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). The hash's one non-obvious property; the host
// as a whole is exercised by rows 6.8 and 6.10 in `openconvert`, over real conversions.
// ---------------------------------------------------------------------------

/// The timestamp is out of the hash, so two builds of one book at different times are one container
/// as far as oscillation detection is concerned. Without this the cycle rule would never fire on a
/// real conversion and would never be tested by one either.
#[test]
fn the_modified_timestamp_is_not_part_of_the_content_hash() {
    let with_one = "<metadata><meta property=\"dcterms:modified\">2026-01-01T00:00:00Z</meta>\
                    <dc:title>A Book</dc:title></metadata>";
    let with_another = "<metadata><meta property=\"dcterms:modified\">2027-06-30T12:34:56Z</meta>\
                        <dc:title>A Book</dc:title></metadata>";

    assert_eq!(strip_modified(with_one), strip_modified(with_another));
    assert!(strip_modified(with_one).contains("A Book"));
    assert!(!strip_modified(with_one).contains("2026"));
}

/// Anything else in the package document is still hashed: the point is to exclude the clock, not to
/// stop noticing that the book changed.
#[test]
fn everything_but_the_timestamp_is_still_hashed() {
    let one =
        "<meta property=\"dcterms:modified\">2026-01-01T00:00:00Z</meta><dc:title>One</dc:title>";
    let two =
        "<meta property=\"dcterms:modified\">2026-01-01T00:00:00Z</meta><dc:title>Two</dc:title>";
    assert_ne!(strip_modified(one), strip_modified(two));

    // And a document with no timestamp at all comes back unchanged.
    let plain = "<dc:title>One</dc:title>";
    assert_eq!(strip_modified(plain), plain);
}
