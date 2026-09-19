//! Sectioning content: `<section>` and the headings that name it.
//!
//! Only [`Sectioning`] admits these, which is what makes a `<section>` inside a
//! `<blockquote>` — or inside a `<p>` — a compile error. It also admits everything
//! [`super::Flow`] admits, because a chapter's paragraphs sit directly in its section.

use super::escape;
use super::{El, EpubType, Phrasing, Sectioning};

/// A finished run of sectioning content: one chapter, or one piece of one.
///
/// The splitter works in these. A spine document is a list of pieces already serialised by the
/// typed builder, packed into files up to `xhtml.split_bytes`; splicing a finished piece is
/// safe precisely because the content model was enforced when it was built.
pub struct SectioningFrag {
    pub(crate) markup: String,
    pub(crate) error: Option<escape::IllegalChar>,
}

impl SectioningFrag {
    /// How many bytes this piece costs a file.
    pub fn len(&self) -> usize {
        self.markup.len()
    }

    pub fn is_empty(&self) -> bool {
        self.markup.is_empty()
    }

    /// The character XML could not carry, if one was written into this piece.
    ///
    /// Exposed because the splitter holds pieces for a while before they are spliced into a
    /// file, and a piece that is never spliced would otherwise carry its poison away with it.
    pub fn error_ref(&self) -> Option<&escape::IllegalChar> {
        self.error.as_ref()
    }
}

/// Build a standalone run of sectioning content.
pub fn sectioning(f: impl FnOnce(El<Sectioning>) -> El<Sectioning>) -> SectioningFrag {
    let built = f(El::<Sectioning>::new());
    SectioningFrag {
        markup: built.buf,
        error: built.error,
    }
}

/// How a section is named to assistive technology.
///
/// Two spellings, and which one is right depends on where the heading is. A first fragment
/// carries its own heading and points at it, so the accessible name and the printed one cannot
/// drift apart. A continuation fragment's heading is in *another file*, and `aria-labelledby`
/// may only reference an element in the same document — so the continuation repeats the
/// heading's text instead. Both keep the fragments reading as one chapter
/// (IMPLEMENTATION_PLAN Phase 5 detail 6); only one of them is valid in each place.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum SectionLabel {
    #[default]
    None,
    /// The id of the heading in this same document.
    By(String),
    /// The heading's text, for a fragment that does not contain the heading.
    Text(String),
}

impl El<Sectioning> {
    /// Splice a finished run of sectioning content.
    pub fn sectioning_frag(mut self, frag: &SectioningFrag) -> Self {
        self.error = self.error.or_else(|| frag.error.clone());
        self.raw(&frag.markup)
    }

    /// `<section>`, optionally with an `epub:type` and the heading that names it.
    ///
    /// `aria-labelledby` rather than `aria-label`: the accessible name of a chapter is the
    /// heading the book prints, and pointing at it keeps the two from drifting apart. It is
    /// also what a mid-chapter split needs — the continuation fragments carry the same
    /// `aria-labelledby` as the first, which is how they stay one chapter to a screen reader
    /// (IMPLEMENTATION_PLAN Phase 5 detail 6).
    pub fn section(
        self,
        kind: Option<EpubType>,
        id: &str,
        label: &SectionLabel,
        f: impl FnOnce(El<Sectioning>) -> El<Sectioning>,
    ) -> Self {
        // The `epub:type` and its DPUB-ARIA role together, because EPUB Accessibility requires
        // them to agree and a screen reader only reads the role (`EpubType::role`).
        let kind = kind
            .map(|kind| match kind.role() {
                Some(role) => format!(" epub:type=\"{}\" role=\"{role}\"", kind.as_str()),
                None => format!(" epub:type=\"{}\"", kind.as_str()),
            })
            .unwrap_or_default();
        let label = match label {
            SectionLabel::None => String::new(),
            SectionLabel::By(target) => {
                format!(" aria-labelledby=\"{}\"", escape::attribute(target))
            }
            SectionLabel::Text(text) => format!(" aria-label=\"{}\"", escape::attribute(text)),
        };
        self.child(
            &format!("<section{kind} id=\"{}\"{label}>", escape::attribute(id)),
            "</section>",
            f,
        )
    }

    /// `<h1>`…`<h6>`.
    ///
    /// The level is clamped into the range XHTML has rather than checked, because
    /// `oc_model::doc::Heading` already clamps on the way in and a second error path here
    /// would only give the emitter something to fail at.
    pub fn heading(
        self,
        level: u8,
        id: &str,
        f: impl FnOnce(El<Phrasing>) -> El<Phrasing>,
    ) -> Self {
        let level = oc_model::doc::Heading::clamp_level(level);
        self.child(
            &format!("<h{level} id=\"{}\">", escape::attribute(id)),
            &format!("</h{level}>"),
            f,
        )
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2)
// ---------------------------------------------------------------------------

/// A heading level outside the range XHTML has would produce `<h9>`, which is not an element.
#[test]
fn a_heading_level_is_clamped_into_the_range_xhtml_has() {
    let deep = El::<Sectioning>::new()
        .heading(9, "h", |t| t.text("Too deep"))
        .finish()
        .expect("serialises");
    assert_eq!(deep, "<h6 id=\"h\">Too deep</h6>");

    let shallow = El::<Sectioning>::new()
        .heading(0, "h", |t| t.text("Too shallow"))
        .finish()
        .expect("serialises");
    assert_eq!(shallow, "<h1 id=\"h\">Too shallow</h1>");
}

/// A continuation fragment of a split chapter is a plain `<section>` carrying the first
/// fragment's `aria-labelledby`, so the two read as one chapter.
#[test]
fn a_continuation_section_borrows_the_first_fragments_heading() {
    let markup = El::<Sectioning>::new()
        .section(
            None,
            "sec1-2",
            &SectionLabel::Text("Chapter One".to_owned()),
            |section| section.p(|t| t.text("…continued.")),
        )
        .finish()
        .expect("serialises");
    assert_eq!(
        markup,
        "<section id=\"sec1-2\" aria-label=\"Chapter One\"><p>…continued.</p></section>"
    );
}
