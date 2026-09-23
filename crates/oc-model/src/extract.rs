//! The Stage-1 extraction layer: what a PDF actually contains, before anything interprets
//! it (IMPLEMENTATION_PLAN Phase 1, `docs/IR_SKETCH.md`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::geom::Rect;

/// A font, interned per document so a glyph carries an index rather than a string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FontId(pub u16);

/// One character as the document draws it, with every signal D3 verified available.
///
/// Thirteen fields, and none of them is optional: a signal that could not be read is a
/// backend bug, not a `None`. A1.1 asserts that no field arrives as a placeholder.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Glyph {
    pub ch: char,
    /// The inked box: what the glyph covers.
    pub bbox: Rect,
    /// The advance box: what the glyph occupies, including side bearings and leading.
    pub loose_bbox: Rect,
    /// The baseline point, in normalised page space.
    pub origin: (f32, f32),
    pub font: FontId,
    /// The size the glyph is drawn at, after the text matrix.
    pub size_pt: f32,
    pub weight: u16,
    pub italic: bool,
    /// PDF text rendering mode: 0 fill, 3 invisible, and the rest of table 5.3.
    pub render_mode: u8,
    /// RGBA, so that a fully transparent fill is distinguishable from a white one.
    pub fill: [u8; 4],
    /// Synthesised by the backend rather than present in the document.
    pub generated: bool,
    /// The backend believes this is a hyphen — a signal dehyphenation needs in Phase 3.
    pub hyphen_flag: bool,
    pub angle_deg: f32,
}

/// A font as the document declares it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FontInfo {
    pub id: FontId,
    pub name: String,
    /// The name with subset prefix, style suffix and case removed, so that `ABCDEF+Minion-Bold`
    /// and `Minion-Regular` group together when heading clusters are found in Phase 4.
    pub family_key: String,
    pub serif: bool,
    pub fixed_pitch: bool,
    pub symbolic: bool,
    pub type3: bool,
    pub embedded: bool,
}

/// A multiset of characters — the quantity the conservation law is stated over (D13.4).
///
/// ASCII is a flat array because that is where nearly every character in a Latin-script book
/// lands, and everything else is a `BTreeMap` so the counts iterate in a deterministic order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharHistogram {
    /// Counts for U+0000..U+007F.
    ascii: Vec<u32>,
    /// Counts for everything above, ordered.
    tail: BTreeMap<char, u32>,
}

/// Serialised as a map from character to count, in code-point order — **not** as the two
/// fields above.
///
/// The flat ASCII array is a representation choice, and serialising it would put a hundred and
/// twenty-eight mostly-zero entries into every page of every dump, which is both unreadable
/// and, over a three-hundred-page book, megabytes of nothing. The map is also the honest shape
/// of the value: a `CharHistogram` *is* a multiset of characters, and `iter` already yields it
/// in the deterministic order canonical JSON needs (D13.3).
impl Serialize for CharHistogram {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(None)?;
        for (ch, count) in self.iter() {
            // A one-character string, because a JSON key is a string and canonical JSON
            // refuses any other kind (`CanonError::KeyNotString`).
            map.serialize_entry(ch.encode_utf8(&mut [0u8; 4]), &count)?;
        }
        map.end()
    }
}

/// Read back from the map [`Serialize`] writes: one-character keys, counts as values. A key of
/// any other length is not a character and fails the read.
impl<'de> Deserialize<'de> for CharHistogram {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let counts = BTreeMap::<String, u32>::deserialize(deserializer)?;
        let mut histogram = CharHistogram::new();
        for (key, count) in counts {
            let mut chars = key.chars();
            match (chars.next(), chars.next()) {
                // A zero count is no character; `iter` never writes one, so neither is it read.
                (Some(_), None) if count == 0 => {}
                (Some(ch), None) => histogram.slot(ch, count),
                _ => {
                    return Err(serde::de::Error::custom(format!(
                        "{key:?} is not one character"
                    )))
                }
            }
        }
        Ok(histogram)
    }
}

/// The size of the flat ASCII table.
const ASCII_SLOTS: usize = 128;

/// `Default` is [`CharHistogram::new`] and not the derived one.
///
/// The derived implementation leaves the flat ASCII array empty, so an ASCII character
/// counted into a defaulted histogram lands in the `tail` map instead of its slot — the same
/// text in two representations, unequal under `PartialEq`. Two conversions of one book must
/// not compare unequal because of which constructor a caller reached for.
impl Default for CharHistogram {
    fn default() -> Self {
        Self::new()
    }
}

impl CharHistogram {
    pub fn new() -> Self {
        Self {
            ascii: vec![0; ASCII_SLOTS],
            tail: BTreeMap::new(),
        }
    }

    /// Count one character.
    ///
    /// Whitespace is counted like anything else here; it is `C`'s definition, applied by the
    /// caller, that leaves whitespace out of the conservation quantity (D13.4).
    pub fn add(&mut self, ch: char) {
        self.slot(ch, 1);
    }

    pub fn add_str(&mut self, text: &str) {
        for ch in text.chars() {
            self.add(ch);
        }
    }

    pub fn count(&self, ch: char) -> u32 {
        let index = usize::try_from(u32::from(ch)).unwrap_or(usize::MAX);
        match self.ascii.get(index) {
            Some(count) => *count,
            None => self.tail.get(&ch).copied().unwrap_or_default(),
        }
    }

    pub fn total(&self) -> u64 {
        let ascii: u64 = self.ascii.iter().map(|c| u64::from(*c)).sum();
        let tail: u64 = self.tail.values().map(|c| u64::from(*c)).sum();
        ascii + tail
    }

    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }

    /// Every character present, with its count, in code-point order.
    pub fn iter(&self) -> impl Iterator<Item = (char, u32)> + '_ {
        let ascii = self
            .ascii
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 0)
            .filter_map(|(index, count)| {
                u32::try_from(index)
                    .ok()
                    .and_then(char::from_u32)
                    .map(|ch| (ch, *count))
            });
        let tail = self.tail.iter().map(|(ch, count)| (*ch, *count));
        ascii.chain(tail)
    }

    /// The multiset union, used to add what a stage produced.
    pub fn union(&self, other: &Self) -> Self {
        let mut out = self.clone();
        for (ch, count) in other.iter() {
            out.slot(ch, count);
        }
        out
    }

    /// The multiset difference, saturating at zero.
    ///
    /// Saturating rather than signed because a negative count has no meaning: a stage that
    /// removed more of a character than existed is a bug the invariant check catches by
    /// comparing totals, not something to represent here.
    pub fn difference(&self, other: &Self) -> Self {
        let mut out = self.clone();
        for (ch, count) in other.iter() {
            let index = usize::try_from(u32::from(ch)).unwrap_or(usize::MAX);
            match out.ascii.get_mut(index) {
                Some(slot) => *slot = slot.saturating_sub(count),
                None => {
                    if let Some(slot) = out.tail.get_mut(&ch) {
                        *slot = slot.saturating_sub(count);
                    }
                }
            }
        }
        out.tail.retain(|_, count| *count > 0);
        out
    }

    fn slot(&mut self, ch: char, by: u32) {
        let index = usize::try_from(u32::from(ch)).unwrap_or(usize::MAX);
        match self.ascii.get_mut(index) {
            Some(slot) => *slot = slot.saturating_add(by),
            None => {
                let slot = self.tail.entry(ch).or_default();
                *slot = slot.saturating_add(by);
            }
        }
    }
}

/// Which page something is on, and what that page is called in the book.
///
/// The label is the *printed* page number when one has been detected — `"iv"`, `"12"` — which
/// is not the index: front matter restarts the numbering, and a reader who asks for page 12
/// means the one with 12 on it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRef {
    pub index: u32,
    pub label: Option<String>,
}

impl PageRef {
    /// A page with no detected label. Phase 4 fills the label in.
    pub fn new(index: u32) -> Self {
        Self { index, label: None }
    }
}

/// An image, interned per document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ImageId(pub u32);

/// What an image is doing on the page.
///
/// The distinction is not cosmetic: a `FullPageBackground` on an `image_only` page *is* the
/// page and goes to OCR, a `Figure` becomes a `<figure>` with a caption, an `Ornament` is a
/// candidate for dropping once Phase 4 sees it repeat, and a `Strip` is usually a rule that
/// should not survive into a reflowable book at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageKind {
    Figure,
    FullPageBackground,
    Ornament,
    Strip,
    /// The backend could not place it. Kept so that an unplaceable image is visibly
    /// unclassified rather than silently a `Figure`.
    Unknown,
}

/// One image as a page draws it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageRef {
    pub id: ImageId,
    pub page: PageRef,
    /// Where it lands, in normalised page space.
    pub bbox: Rect,
    /// Its own pixel dimensions, before any scaling onto the page.
    pub intrinsic_px: (u32, u32),
    /// Whether it carries a soft mask or a stencil mask — that is, whether part of it is
    /// meant to be transparent.
    pub has_smask: bool,
    /// Whether it was written inline in the content stream (`BI … ID … EI`) rather than as an
    /// XObject. Inline images are small by rule and often decorative.
    pub is_inline: bool,
    /// The colour space as the file names it, e.g. `"DeviceGray"`.
    pub colorspace: String,
    /// Pixels per inch as actually reproduced: `intrinsic_px.0 / (bbox width in inches)`.
    pub effective_dpi: f32,
    pub kind: ImageKind,
}

/// A vector region's identifier, interned per document in draw order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct VecId(pub u32);

/// A vector drawing on a page: one path object's bounding box, and whether it is a rule.
///
/// Two consumers, and both of them only want `is_rule`: the footnote separator (PIPELINE
/// §8.3 — a short rule immediately above a small-font block at the foot of a page) and the
/// table lattice (§8.7 — long thin axis-aligned paths snapped into a grid). Everything else
/// vector is rasterised at 2× in v1 (D16), so the IR carries the box and the count rather
/// than the path data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorRegion {
    pub id: VecId,
    pub page: PageRef,
    pub bbox: Rect,
    /// How many path objects were merged into this region. One, in v1: PDFium reports path
    /// objects individually and nothing merges them yet.
    pub path_count: u32,
    /// A thin, axis-aligned line: long in one dimension and near-zero in the other.
    pub is_rule: bool,
}

impl VectorRegion {
    /// The rule's length along its own axis, or zero if it is not a rule.
    pub fn rule_length(&self) -> f32 {
        if !self.is_rule {
            return 0.0;
        }
        (self.bbox.x1 - self.bbox.x0).max(self.bbox.y1 - self.bbox.y0)
    }

    /// Whether the rule runs left-to-right rather than top-to-bottom. Meaningless for a
    /// region that is not a rule, and the callers that ask are all rule-only.
    pub fn is_horizontal(&self) -> bool {
        (self.bbox.x1 - self.bbox.x0) >= (self.bbox.y1 - self.bbox.y0)
    }
}

/// One entry in a document's outline — a PDF bookmark.
///
/// The outline is the strongest structural signal a PDF carries, and the only one a producer
/// writes deliberately: Phase 4 prefers it over every heuristic it has when the two disagree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutlineEntry {
    pub title: String,
    /// Depth in the tree, zero for a top-level entry.
    pub level: u16,
    /// The page it points at, when the destination could be resolved. `None` for an entry
    /// whose action is something other than "go to a page in this document" — a URI, a
    /// launch, a remote destination.
    pub page: Option<u32>,
}

/// A histogram reads back from its map as the same multiset — ASCII and beyond, counts intact —
/// which is what a saved ledger resumes from (Phase 12, A12.4b).
#[test]
fn a_histogram_reads_back_as_written() {
    let mut histogram = CharHistogram::new();
    histogram.add_str("Çağrı çiçek, ﬁn — Straße");
    let text = serde_json::to_string(&histogram).expect("serialises");
    let back: CharHistogram = serde_json::from_str(&text).expect("reads");
    assert_eq!(back, histogram);
    assert!(serde_json::from_str::<CharHistogram>(r#"{"ab": 1}"#).is_err());
}
