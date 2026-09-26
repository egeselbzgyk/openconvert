//! Metadata: XMP, then the Info dictionary, then the page (PIPELINE §8.8).
//!
//! The ordering is not arbitrary. XMP is where a modern producer writes what the *author*
//! said; the Info dictionary is where the *application* writes what it knows — which is why a
//! Word export carries `"Microsoft Word - draft.docx"` as its title and the real one nowhere
//! in the file at all.
//!
//! **Boilerplate is worse than nothing because it looks valid.** A converter that trusts
//! `/Title` ships a library whose every second book is called `Microsoft Word - draft`, and
//! nothing downstream can tell that apart from a title somebody meant. So the blocklist is a
//! rejection, not a preference: a title that matches it is discarded and the heuristic runs.
//!
//! `dc:identifier` is a `urn:uuid` minted deterministically from `source_sha256`, so
//! reconverting the same bytes keeps the same identity and a reading system does not treat an
//! update as a new book (R5 §A2).

use oc_core::thresholds::Thresholds;
use oc_model::confidence::{Confidence, Signal};
use oc_model::doc::{MetaSource, Metadata};
use oc_model::lang::LangTag;
use oc_text::fold::fold_key;

use crate::view::BlockView;

/// The namespace UUIDv5 is minted under.
///
/// The RFC 4122 URL namespace, because the identifier names a document by its content hash
/// and a hash is a name rather than a DNS host or an OID. Fixed forever: changing it changes
/// every book's identity, which is the one thing this function exists to prevent.
const NAMESPACE: uuid::Uuid = uuid::Uuid::NAMESPACE_URL;

/// How many pages the heuristic is allowed to read (PIPELINE §8.8, "pages 1-3").
const HEURISTIC_PAGES: u32 = 3;

/// The author patterns, in the three target languages.
const BY_WORDS: [&str; 5] = ["by", "von", "yazan", "aus", "geschrieben"];

/// What a document's Info dictionary said.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InfoDict {
    pub title: Option<String>,
    pub author: Option<String>,
}

/// Everything about the *file* that metadata resolution reads.
///
/// A struct rather than five parameters, because four of the five are claims made by the
/// file about itself and grouping them is what makes the precedence in [`metadata`] readable:
/// XMP, then the Info dictionary, then the page, then the name.
#[derive(Clone, Debug)]
pub struct MetaSources {
    pub xmp: oc_pdf_meta::XmpMeta,
    pub info: InfoDict,
    /// The file's own name, for the last-resort title.
    pub filename: String,
    /// The hash the identifier is minted from (R5 §A2).
    pub source_sha256: String,
    /// The language `text` detected, used when XMP names none.
    pub language: LangTag,
}

/// Whether a title is boilerplate a producer wrote rather than a title anyone chose.
///
/// The list is PIPELINE §8.8's, and every entry is there because a real producer emits it.
pub fn is_boilerplate(title: &str) -> bool {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_lowercase();

    if lower.starts_with("microsoft word - ") || lower.starts_with("microsoft powerpoint - ") {
        return true;
    }
    // PIPELINE §8.8 writes this one as `^untitled`, which also matches `Untitledness` — a
    // real word and a possible title. A word boundary is the same rule without that cost:
    // `untitled`, `Untitled 3` and `untitled-1` are caught, `Untitledness` is not.
    if let Some(tail) = lower.strip_prefix("untitled") {
        if tail.chars().next().is_none_or(|c| !c.is_alphabetic()) {
            return true;
        }
    }
    // A filename, whatever is in front of it.
    if [
        ".doc", ".docx", ".indd", ".pages", ".ppt", ".pptx", ".pdf", ".rtf", ".odt",
    ]
    .iter()
    .any(|extension| lower.ends_with(extension))
    {
        return true;
    }
    // `Document`, `Document1`, `Dokument3` — a word processor's default name.
    let stem = lower.trim_end_matches(|c: char| c.is_ascii_digit());
    matches!(stem, "document" | "dokument" | "belge" | "presentation")
}

/// Resolve the document's metadata (PIPELINE §8.8).
pub fn metadata(
    sources: &MetaSources,
    blocks: &[BlockView],
    body_size_pt: f32,
    // Read for the front pages' extent, `toc.max_front_pages`: how far into a book its title
    // page and copyright page can be.
    t: &Thresholds,
) -> (Metadata, Confidence) {
    let MetaSources {
        xmp,
        info,
        filename,
        source_sha256,
        language,
    } = sources;
    let from_xmp = xmp
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !is_boilerplate(title));
    let from_info = info
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !is_boilerplate(title));

    let (title, source) = match (from_xmp, from_info) {
        (Some(title), _) => (Some(title.to_owned()), MetaSource::Xmp),
        (None, Some(title)) => (Some(title.to_owned()), MetaSource::InfoDict),
        (None, None) => (
            largest_block_title(blocks, body_size_pt).or_else(|| title_from_filename(filename)),
            MetaSource::Heuristic,
        ),
    };

    // The file's own author field is whoever made the file — often the person who scanned or
    // uploaded it, not the person who wrote the book. It is believed only when the book's front
    // pages print that name too.
    let front = front_text(blocks, language, t);
    let printed = |name: &str| {
        let key = squash_key(name, language);
        !key.is_empty() && front.contains(&key)
    };
    let authors = if !xmp.creators.is_empty() {
        xmp.creators.clone()
    } else {
        info.author
            .as_deref()
            .map(str::trim)
            .filter(|author| !is_boilerplate(author))
            .filter(|author| printed(author))
            .map(|author| vec![author.to_owned()])
            .unwrap_or_else(|| author_from_page(blocks, language).into_iter().collect())
    };

    let signals = vec![
        Signal::new("xmp_title", f32::from(u8::from(from_xmp.is_some()))),
        Signal::new("info_title", f32::from(u8::from(from_info.is_some()))),
        Signal::new(
            "info_title_boilerplate",
            f32::from(u8::from(info.title.as_deref().is_some_and(is_boilerplate))),
        ),
        Signal::new("authors", authors.len() as f32),
    ];

    let meta = Metadata {
        title,
        subtitle: None,
        authors,
        translator: None,
        publisher: None,
        date: None,
        identifier: identifier(source_sha256),
        language: xmp
            .language
            .as_deref()
            .map(LangTag::new)
            .unwrap_or_else(|| language.clone()),
        source,
    };
    let confidence = if source == MetaSource::Heuristic {
        Confidence::fallback(signals)
    } else {
        Confidence::deterministic(signals)
    };
    (meta, confidence)
}

/// The book's `dc:identifier`: `urn:uuid:` over the source hash (R5 §A2).
///
/// A pure function of the bytes, so two conversions of one file agree and a reading system
/// does not shelve an updated conversion as a second book.
pub fn identifier(source_sha256: &str) -> String {
    let uuid = uuid::Uuid::new_v5(&NAMESPACE, source_sha256.trim().as_bytes());
    format!("urn:uuid:{uuid}")
}

/// The front pages' text, folded and with every space removed, for a name to be looked up in.
fn front_text(blocks: &[BlockView], language: &LangTag, t: &Thresholds) -> String {
    let front = u32::try_from(t.toc.max_front_pages.max(0)).unwrap_or(u32::MAX);
    blocks
        .iter()
        .filter(|block| block.page < front)
        .map(|block| squash_key(&block.text, language))
        .collect()
}

/// A lookup key: folded in the book's own locale, spaces and punctuation gone.
fn squash_key(text: &str, language: &LangTag) -> String {
    fold_key(text, language.clone())
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

/// The heuristic title: the largest-font block on the first three pages.
fn largest_block_title(blocks: &[BlockView], body_size_pt: f32) -> Option<String> {
    blocks
        .iter()
        .filter(|block| block.page < HEURISTIC_PAGES)
        .filter(|block| !block.text.trim().is_empty())
        .filter(|block| block.size_pt() > body_size_pt)
        // Largest first, then earliest — a title page's title is the biggest thing on it and,
        // where two things tie, the one a reader meets first.
        .max_by(|left, right| {
            left.size_pt()
                .total_cmp(&right.size_pt())
                .then(right.order.cmp(&left.order))
        })
        .map(|block| block.text.trim().to_owned())
}

/// The heuristic author: a `by | von | yazan` line on the first three pages.
fn author_from_page(blocks: &[BlockView], lang: &LangTag) -> Option<String> {
    blocks
        .iter()
        .filter(|block| block.page < HEURISTIC_PAGES)
        .find_map(|block| {
            let text = block.text.trim();
            let (first, rest) = text.split_once(char::is_whitespace)?;
            BY_WORDS
                .contains(&fold_key(first, lang.clone()).as_str())
                .then(|| rest.trim().to_owned())
                .filter(|name| !name.is_empty())
        })
}

/// The last resort: the file's own name, cleaned up.
fn title_from_filename(filename: &str) -> Option<String> {
    let stem = std::path::Path::new(filename)
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)?;
    let cleaned = stem.replace(['_', '-'], " ");
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    (!cleaned.is_empty() && !is_boilerplate(&cleaned)).then_some(cleaned)
}

/// The XMP type, re-exported under a short path so this module's signature reads as
/// PIPELINE §8.8 states it.
pub mod oc_pdf_meta {
    pub use oc_pdf::meta::XmpMeta;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boilerplate_blocklist_catches_what_producers_emit() {
        for title in [
            "Microsoft Word - draft.docx",
            "microsoft word - Draft",
            "Microsoft PowerPoint - deck.pptx",
            "untitled",
            "Untitled 3",
            "report.indd",
            "book.pages",
            "Document",
            "Document1",
            "Dokument2",
            "",
            "   ",
        ] {
            assert!(is_boilerplate(title), "{title:?} is boilerplate");
        }
        for title in [
            "The Weather in the Delta",
            "A Word on Documents",
            "Pages from a Journal",
            "Untitledness",
        ] {
            assert!(!is_boilerplate(title), "{title:?} is a real title");
        }
    }

    /// Row 4.17. Two runs over the same bytes mint the same `urn:uuid:` — that is the whole
    /// requirement, and it is what stops a reading system shelving a reconversion as a second
    /// book (R5 §A2).
    #[test]
    fn identifier_is_stable_across_reconversions() {
        let hash = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
        assert_eq!(identifier(hash), identifier(hash));
        assert!(identifier(hash).starts_with("urn:uuid:"));
        // Thirty-six characters of UUID after the scheme.
        assert_eq!(identifier(hash).len(), "urn:uuid:".len() + 36);
        // And different bytes are a different book.
        assert_ne!(identifier(hash), identifier("0000"));
        // Whitespace around the hash is not part of it: a hash read from a file with a
        // trailing newline must not be a different book.
        assert_eq!(identifier(hash), identifier(&format!("{hash}\n")));
    }

    #[test]
    fn a_filename_becomes_a_title_only_when_it_is_not_boilerplate() {
        assert_eq!(
            title_from_filename("/books/the_weather_in_the_delta.pdf").as_deref(),
            Some("the weather in the delta")
        );
        assert_eq!(title_from_filename("/tmp/untitled.pdf"), None);
        assert_eq!(title_from_filename("/tmp/Document1.pdf"), None);
    }
}
