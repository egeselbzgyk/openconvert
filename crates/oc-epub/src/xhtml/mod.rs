//! The typed XHTML builder: the phase's core idea (D5, RT A6.1).
//!
//! A hand-rolled EPUB's characteristic bug is a **content-model violation** — `<figure>`
//! inside `<p>`, `<a>` inside `<a>`, `<aside>` where only phrasing is allowed. Those produce
//! perfectly well-formed XML, so neither a parser nor our own Tier-1 well-formedness check can
//! see them; only a schema engine or EPUBCheck can, and by then the book has been written.
//!
//! So the content model is in the *types*. [`El<C>`] carries a marker saying which content
//! model it accepts, and the methods live on the marker rather than on the element:
//! `El<Phrasing>` has no `figure`, no `aside_footnote`, no `blockquote`, no `table`, no `ol`
//! and no `section`, so the illegal document does not compile. That is dramatically cheaper
//! for a *generator* than validating after the fact, because a generator only ever needs to
//! emit the legal shapes (RT A6).
//!
//! The three contexts, and what each admits:
//!
//! | Marker | Admits | Emitted by |
//! |---|---|---|
//! | [`Sectioning`] | sections, headings, and everything `Flow` admits | `<body>`, `<section>` |
//! | [`Flow`] | paragraphs, lists, quotes, figures, tables, asides | `<blockquote>`, `<li>`, `<aside>` |
//! | [`Phrasing`] | text, emphasis, spans, note references, links | `<p>`, `<h1>`…`<h6>`, `<figcaption>` |
//! | [`NoAnchor`] | everything `Phrasing` admits except anchors | inside `<a>` |
//!
//! Errors are carried rather than returned so that the builder stays chainable: a character
//! XML cannot represent poisons the element it was written into and every element it is
//! spliced into, and [`El::finish`] is where it surfaces. A builder that returned `Result`
//! from every method would make the emitter unreadable and would tempt exactly the `unwrap`
//! the workspace forbids.

pub mod escape;
mod flow;
mod phrasing;
mod sectioning;

use std::marker::PhantomData;

pub use escape::IllegalChar;
pub use flow::{flow, FlowContext, FlowFrag, ImgRef, ListKind, TableCell};
pub use phrasing::{frag, PhrasingContext, PhrasingFrag};

/// Accepts sections and headings as well as flow content: `<body>` and `<section>`.
pub struct Sectioning;

/// Accepts block-level content: paragraphs, lists, quotes, figures, tables, asides.
pub struct Flow;

/// Accepts inline content: text, emphasis, spans, note references, links.
pub struct Phrasing;

/// Phrasing content minus anchors, which is what the inside of an `<a>` may hold.
///
/// This is the whole of the `<a>`-inside-`<a>` defence: [`El::<Phrasing>::link`] hands its
/// closure an `El<NoAnchor>`, and `El<NoAnchor>` has no `link` and no `noteref`.
pub struct NoAnchor;

/// An element under construction, in a named content model.
///
/// Holds the serialised bytes of its children and the first illegal character anyone wrote
/// into it. Construction is by closure — `p(|t| t.text("…"))` — so a child element cannot
/// escape its parent and cannot be spliced into a context that does not admit it.
pub struct El<C> {
    buf: String,
    error: Option<IllegalChar>,
    marker: PhantomData<C>,
}

impl<C> El<C> {
    /// An empty element body in this content model.
    pub(crate) fn new() -> Self {
        Self {
            buf: String::new(),
            error: None,
            marker: PhantomData,
        }
    }

    /// Append raw serialised markup. Private: everything public goes through a method that
    /// knows the content model, which is the point of the type parameter.
    pub(crate) fn raw(mut self, markup: &str) -> Self {
        self.buf.push_str(markup);
        self
    }

    /// Build a child in content model `D`, then splice its bytes and carry its error.
    pub(crate) fn child<D>(
        mut self,
        open: &str,
        close: &str,
        f: impl FnOnce(El<D>) -> El<D>,
    ) -> Self {
        let inner = f(El::<D>::new());
        self.error = self.error.or(inner.error);
        self.buf.push_str(open);
        self.buf.push_str(&inner.buf);
        self.buf.push_str(close);
        self
    }

    /// Append text, checking it against XML 1.0's character range first.
    pub(crate) fn escaped(mut self, raw: &str) -> Self {
        match escape::check(raw) {
            Ok(()) => self.buf.push_str(&escape::text(raw)),
            Err(illegal) => self.error = self.error.or(Some(illegal)),
        }
        self
    }

    /// The serialised children, or the first character XML could not carry.
    pub fn finish(self) -> Result<String, IllegalChar> {
        match self.error {
            Some(illegal) => Err(illegal),
            None => Ok(self.buf),
        }
    }

    /// How many bytes this element has serialised so far.
    ///
    /// The splitter needs it: `xhtml.split_bytes` is a size bound on a *file*, and the only
    /// way to respect it on a paragraph boundary is to know what a paragraph costs before
    /// deciding whether it fits.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

/// One `epub:type` value (R5 §A5).
///
/// A closed enum rather than a string, because `epub:type` is a vocabulary and a typo in one
/// is invisible: a reading system that does not recognise `chaptr` simply ignores it, and the
/// book loses its semantics with no error anywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EpubType {
    Cover,
    Frontmatter,
    Bodymatter,
    Backmatter,
    Part,
    Chapter,
    Toc,
    Landmarks,
    PageList,
    Footnote,
    Noteref,
    Footnotes,
    Pagebreak,
}

impl EpubType {
    pub fn as_str(self) -> &'static str {
        match self {
            EpubType::Cover => "cover",
            EpubType::Frontmatter => "frontmatter",
            EpubType::Bodymatter => "bodymatter",
            EpubType::Backmatter => "backmatter",
            EpubType::Part => "part",
            EpubType::Chapter => "chapter",
            EpubType::Toc => "toc",
            EpubType::Landmarks => "landmarks",
            EpubType::PageList => "page-list",
            EpubType::Footnote => "footnote",
            EpubType::Noteref => "noteref",
            EpubType::Footnotes => "footnotes",
            EpubType::Pagebreak => "pagebreak",
        }
    }
}

/// The CSS classes `style.css` defines (D13.11).
///
/// Also a closed enum, and for a sharper reason than `epub:type`: the stylesheet and the
/// markup are two files that have to agree, and a class named in one and not the other fails
/// silently and invisibly. Adding a variant here without adding the rule is the only way to
/// get them out of step, and that is a one-line diff in a reviewed file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CssClass {
    Verse,
    Stanza,
    DropCap,
    SmallCaps,
    Caption,
    PageBreak,
    /// A list whose markers were printed by the book and are still in the item text, so the
    /// reading system must not draw markers of its own. See `docs/DECISIONS_LOG.md`.
    ListPrintedMarkers,
    /// The `<details>` fallback under a table that had to be emitted as an image.
    TableFallback,
}

impl CssClass {
    pub fn as_str(self) -> &'static str {
        match self {
            CssClass::Verse => "verse",
            CssClass::Stanza => "stanza",
            CssClass::DropCap => "dropcap",
            CssClass::SmallCaps => "smallcaps",
            CssClass::Caption => "caption",
            CssClass::PageBreak => "pagebreak",
            CssClass::ListPrintedMarkers => "list-printed-markers",
            CssClass::TableFallback => "table-fallback",
        }
    }

    /// Every class, so `css.rs` can prove it defines a rule for each one.
    pub const ALL: &'static [CssClass] = &[
        CssClass::Verse,
        CssClass::Stanza,
        CssClass::DropCap,
        CssClass::SmallCaps,
        CssClass::Caption,
        CssClass::PageBreak,
        CssClass::ListPrintedMarkers,
        CssClass::TableFallback,
    ];
}

/// The XML declaration, the doctype and the `<html>` element around one content document.
///
/// `<!DOCTYPE html>` and nothing else: EPUB 3.3 allows exactly that form, and any internal
/// subset — an entity declaration — is both forbidden here and the shape of the billion-laughs
/// attack (D5, D14).
pub fn content_document(
    title: &str,
    lang: &oc_model::lang::LangTag,
    body: impl FnOnce(El<Sectioning>) -> El<Sectioning>,
) -> Result<String, IllegalChar> {
    escape::check(title)?;
    let inner = body(El::<Sectioning>::new()).finish()?;
    let lang = escape::attribute(lang.as_str());
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE html>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\" \
         xmlns:epub=\"http://www.idpf.org/2007/ops\" \
         xml:lang=\"{lang}\" lang=\"{lang}\">\n\
         <head>\n\
         <meta charset=\"utf-8\"/>\n\
         <title>{}</title>\n\
         <link rel=\"stylesheet\" type=\"text/css\" href=\"../style.css\"/>\n\
         </head>\n\
         <body>\n{inner}</body>\n\
         </html>\n",
        escape::text(title)
    ))
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). The compile-*failure* rows 5.1 and 5.2
// are `tests/compile_fail.rs`; a test that has to *not* compile cannot live in the crate.
// ---------------------------------------------------------------------------

/// The shape of an ordinary page, so that the arrangement the compile-fail tests forbid has a
/// positive counterpart that is pinned.
#[test]
fn a_page_serialises_as_the_markup_it_was_built_from() {
    use oc_model::lang::LangTag;

    let html = content_document("Chapter One", &LangTag::EN, |body| {
        body.section(Some(EpubType::Chapter), "sec1", Some("sec1-h"), |section| {
            section
                .heading(1, "sec1-h", |h| h.text("Chapter One"))
                .p(|t| t.text("It was a dark and stormy night."))
        })
    })
    .expect("ordinary text serialises");

    assert!(html.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n"));
    assert!(html.contains("xml:lang=\"en\" lang=\"en\""));
    assert!(html.contains("<section epub:type=\"chapter\" id=\"sec1\" aria-labelledby=\"sec1-h\">"));
    assert!(html.contains("<h1 id=\"sec1-h\">Chapter One</h1>"));
    assert!(html.contains("<p>It was a dark and stormy night.</p>"));
    assert!(!html.contains("<!ENTITY"), "no internal subset, ever (D5)");
}

/// A character XML cannot carry poisons the element it was written into, and the poison has to
/// survive being spliced into a parent — otherwise a control character deep in a quoted list
/// would reach the file.
#[test]
fn an_illegal_character_propagates_out_of_every_nesting_it_was_written_into() {
    use oc_model::lang::LangTag;

    let refused = content_document("Fine", &LangTag::EN, |body| {
        body.blockquote(|quote| quote.p(|t| t.text("deep\u{1}inside")))
    })
    .expect_err("U+0001 cannot be serialised");
    assert_eq!(refused.ch, '\u{1}');

    // And the same text without it goes through, so the check is not simply always failing.
    assert!(content_document("Fine", &LangTag::EN, |body| {
        body.blockquote(|quote| quote.p(|t| t.text("deep inside")))
    })
    .is_ok());
}
