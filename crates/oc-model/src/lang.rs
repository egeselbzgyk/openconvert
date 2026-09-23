//! Language tags (D13.11).
//!
//! EPUB 3.3 requires `dc:language`, so a tag is never optional at the package level; what is
//! optional is a per-block `xml:lang`, and only when the evidence for it clears the bar in
//! PIPELINE §4 step 7.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// A BCP-47 language tag, held lower-cased and compared on its primary subtag.
///
/// Stored as a string rather than an enum because the set is open: `whatlang` knows about
/// seventy languages, a job spec may name any tag at all, and an enum would turn "a language
/// we have not thought about" into a parse failure at the one point in the pipeline that must
/// not fail — emitting a valid package. `Cow` so the tags the code names are `const` and cost
/// nothing, while a detected or configured one owns its bytes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LangTag(Cow<'static, str>);

impl LangTag {
    pub const EN: LangTag = LangTag(Cow::Borrowed("en"));
    pub const DE: LangTag = LangTag(Cow::Borrowed("de"));
    pub const TR: LangTag = LangTag(Cow::Borrowed("tr"));
    /// BCP-47 `und`: the language is undetermined.
    ///
    /// `dc:language` is required by EPUB 3.3 and must not be empty, so a book whose language
    /// detection abstained still needs a tag. `und` is the tag that says so; guessing `en`
    /// would tell a screen reader to pronounce a German book in English, which is a worse
    /// failure than admitting to not knowing.
    pub const UND: LangTag = LangTag(Cow::Borrowed("und"));

    /// Build a tag from text. Trimmed and lower-cased, because BCP-47 is case-insensitive
    /// and two spellings of one language must not be two languages.
    pub fn new(tag: &str) -> Self {
        Self(Cow::Owned(tag.trim().to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The primary subtag: `de` of `de-CH`. What locale-sensitive rules key on.
    pub fn primary(&self) -> &str {
        self.0.split('-').next().unwrap_or(&self.0)
    }

    /// Whether this language uses the Turkic dotted/dotless i pairing, where `I` lowercases
    /// to `ı` and `İ` to `i` (R10 §6.3).
    pub fn is_turkic_i(&self) -> bool {
        matches!(self.primary(), "tr" | "az")
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Display for LangTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[test]
fn a_tag_is_case_insensitive_and_keeps_its_region() {
    assert_eq!(LangTag::new("DE-ch").as_str(), "de-ch");
    assert_eq!(LangTag::new(" TR ").primary(), "tr");
    assert_eq!(LangTag::new("TR"), LangTag::TR);
}

/// `und` is a real tag and has to behave like one, because it is the value a book gets when
/// detection abstains and it is emitted into `dc:language` unchanged.
#[test]
fn the_undetermined_tag_is_a_tag() {
    assert_eq!(LangTag::UND.as_str(), "und");
    assert!(!LangTag::UND.is_empty());
    assert_eq!(LangTag::new("UND"), LangTag::UND);
}

#[test]
fn only_turkish_and_azerbaijani_pair_the_dotless_i() {
    assert!(LangTag::TR.is_turkic_i());
    assert!(LangTag::new("az-Latn").is_turkic_i());
    assert!(!LangTag::EN.is_turkic_i());
    assert!(!LangTag::new("tt").is_turkic_i());
}
