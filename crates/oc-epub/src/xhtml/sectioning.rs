//! Sectioning content: `<section>` and the headings that name it.
//!
//! Only [`Sectioning`] admits these, which is what makes a `<section>` inside a
//! `<blockquote>` — or inside a `<p>` — a compile error. It also admits everything
//! [`super::Flow`] admits, because a chapter's paragraphs sit directly in its section.

use super::escape;
use super::{El, EpubType, Phrasing, Sectioning};

impl El<Sectioning> {
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
        labelled_by: Option<&str>,
        f: impl FnOnce(El<Sectioning>) -> El<Sectioning>,
    ) -> Self {
        let kind = kind
            .map(|kind| format!(" epub:type=\"{}\"", kind.as_str()))
            .unwrap_or_default();
        let labelled_by = labelled_by
            .map(|target| format!(" aria-labelledby=\"{}\"", escape::attribute(target)))
            .unwrap_or_default();
        self.child(
            &format!(
                "<section{kind} id=\"{}\"{labelled_by}>",
                escape::attribute(id)
            ),
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
        .section(None, "sec1-2", Some("sec1-h"), |section| {
            section.p(|t| t.text("…continued."))
        })
        .finish()
        .expect("serialises");
    assert_eq!(
        markup,
        "<section id=\"sec1-2\" aria-labelledby=\"sec1-h\"><p>…continued.</p></section>"
    );
}
