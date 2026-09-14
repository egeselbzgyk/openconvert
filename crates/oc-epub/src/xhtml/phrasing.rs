//! Inline content: text, emphasis, spans, note references and links.
//!
//! Two markers share this content model — [`Phrasing`] and [`NoAnchor`] — and the difference
//! between them is one method. Everything else is implemented once, over the
//! [`PhrasingContext`] trait, so the two cannot drift apart: an element that is legal inside a
//! paragraph and illegal inside a link would be a content-model bug of exactly the kind this
//! module exists to prevent.

use super::escape;
use super::{CssClass, El, EpubType, NoAnchor, Phrasing};

/// A content model that admits inline content.
///
/// Sealed by being private in effect: the trait is public so the generic methods are callable,
/// but nothing outside this crate can name a new marker type usefully, because [`El`]'s
/// constructor is crate-private.
pub trait PhrasingContext {}
impl PhrasingContext for Phrasing {}
impl PhrasingContext for NoAnchor {}

/// A finished run of inline content, ready to be spliced somewhere that wants one.
///
/// The plan's `Option<PhrasingFrag>` for a figure caption: a caption is optional, and
/// `Option<impl FnOnce(..)>` is unusable at a call site that wants `None`.
pub struct PhrasingFrag {
    pub(crate) markup: String,
    pub(crate) error: Option<escape::IllegalChar>,
}

impl PhrasingFrag {
    pub fn is_empty(&self) -> bool {
        self.markup.is_empty()
    }
}

/// Build a standalone run of inline content.
pub fn frag(f: impl FnOnce(El<Phrasing>) -> El<Phrasing>) -> PhrasingFrag {
    let built = f(El::<Phrasing>::new());
    PhrasingFrag {
        markup: built.buf,
        error: built.error,
    }
}

impl<C: PhrasingContext> El<C> {
    /// Plain text, escaped.
    pub fn text(self, s: &str) -> Self {
        self.escaped(s)
    }

    /// `<em>`: stress emphasis, which is what italic body type means in a book.
    pub fn em(self, f: impl FnOnce(El<C>) -> El<C>) -> Self {
        self.child("<em>", "</em>", f)
    }

    /// `<strong>`: what bold body type means.
    pub fn strong(self, f: impl FnOnce(El<C>) -> El<C>) -> Self {
        self.child("<strong>", "</strong>", f)
    }

    /// `<sup>` and `<sub>`, for the raised and lowered runs `text` read off the geometry.
    ///
    /// Spelled out rather than named after the tags: `sub` is `std::ops::Sub::sub`, and a
    /// method that can be confused with an operator on a builder that is all method chaining
    /// is a reading hazard rather than a compile error.
    pub fn superscript(self, f: impl FnOnce(El<C>) -> El<C>) -> Self {
        self.child("<sup>", "</sup>", f)
    }

    pub fn subscript(self, f: impl FnOnce(El<C>) -> El<C>) -> Self {
        self.child("<sub>", "</sub>", f)
    }

    /// `<code>`: a monospace run inside running text.
    pub fn code(self, f: impl FnOnce(El<C>) -> El<C>) -> Self {
        self.child("<code>", "</code>", f)
    }

    /// `<span class="…">`, from the closed class list the stylesheet defines.
    pub fn span_class(self, class: CssClass, f: impl FnOnce(El<C>) -> El<C>) -> Self {
        self.child(
            &format!("<span class=\"{}\">", class.as_str()),
            "</span>",
            f,
        )
    }

    /// `<br/>`: a line break that is content, as it is inside a line of verse.
    pub fn br(self) -> Self {
        self.raw("<br/>")
    }

    /// Splice a finished inline run.
    pub fn frag(mut self, frag: &PhrasingFrag) -> Self {
        self.error = self.error.or_else(|| frag.error.clone());
        self.raw(&frag.markup)
    }
}

impl El<Phrasing> {
    /// The note reference half of Apple's pop-up footnote pattern (R5 §A5):
    /// `<a epub:type="noteref" href="#fnN">N</a>`.
    ///
    /// The marker is plain text and there is no closure, so nothing can be placed inside the
    /// anchor and `<a>` inside `<a>` cannot arise from this method at all. The return type is
    /// still `Self` and deliberately so: a paragraph with two footnotes is ordinary, and an
    /// emitter that loops over a paragraph's spans cannot change the type of its accumulator
    /// halfway through the loop. See `docs/DECISIONS_LOG.md`.
    pub fn noteref(self, note_id: &str, marker: &str) -> Self {
        self.child::<NoAnchor>(
            &format!(
                "<a epub:type=\"{}\" role=\"doc-noteref\" href=\"{}\">",
                EpubType::Noteref.as_str(),
                escape::attribute(&format!("#{note_id}"))
            ),
            "</a>",
            |anchor| anchor.text(marker),
        )
    }

    /// A link, whose content is [`NoAnchor`] — which is the whole `<a>`-inside-`<a>` defence.
    pub fn link(self, href: &str, f: impl FnOnce(El<NoAnchor>) -> El<NoAnchor>) -> Self {
        self.child::<NoAnchor>(
            &format!("<a href=\"{}\">", escape::attribute(href)),
            "</a>",
            f,
        )
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2)
// ---------------------------------------------------------------------------

/// The pop-up footnote pattern, and the fact that two of them in one paragraph is ordinary.
#[test]
fn a_paragraph_may_carry_more_than_one_note_reference() {
    let markup = frag(|t| {
        t.text("A claim")
            .noteref("fn1", "1")
            .text(" and another")
            .noteref("fn2", "2")
            .text(".")
    });
    assert!(markup.error.is_none());
    assert_eq!(
        markup.markup,
        "A claim<a epub:type=\"noteref\" role=\"doc-noteref\" href=\"#fn1\">1</a> and \
         another<a epub:type=\"noteref\" role=\"doc-noteref\" href=\"#fn2\">2</a>."
    );
}

/// Nesting is what the type forbids: the inside of a link is `NoAnchor`, which has no `link`
/// and no `noteref`. `tests/compile_fail.rs` proves the negative; this pins the positive.
#[test]
fn a_link_may_hold_emphasis_but_its_content_model_is_not_phrasing() {
    let markup = frag(|t| t.link("#sec2", |a| a.em(|e| e.text("see chapter two"))));
    assert_eq!(
        markup.markup,
        "<a href=\"#sec2\"><em>see chapter two</em></a>"
    );
}

/// A span's class comes from the closed list, so a typo is a compile error rather than a rule
/// that silently never matches.
#[test]
fn a_span_carries_a_class_from_the_stylesheet_and_nothing_else() {
    let markup = frag(|t| t.span_class(CssClass::SmallCaps, |s| s.text("Chapter")));
    assert_eq!(markup.markup, "<span class=\"smallcaps\">Chapter</span>");
}
