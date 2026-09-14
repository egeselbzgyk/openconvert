//! Block-level content: paragraphs, lists, quotes, figures, tables, asides, page breaks.
//!
//! Two markers admit this content model — [`Flow`] and [`Sectioning`] — because everything
//! that may appear in a `<blockquote>` may also appear directly in a `<section>`. The reverse
//! is not true, which is why `section` and `heading` are on [`Sectioning`] alone.

use super::escape;
use super::{CssClass, El, EpubType, Flow, PhrasingFrag, Sectioning};

/// A content model that admits block-level content.
pub trait FlowContext {}
impl FlowContext for Flow {}
impl FlowContext for Sectioning {}

/// A finished run of block-level content: one list item, one note body.
pub struct FlowFrag {
    pub(crate) markup: String,
    pub(crate) error: Option<escape::IllegalChar>,
}

/// Build a standalone run of block-level content.
pub fn flow(f: impl FnOnce(El<Flow>) -> El<Flow>) -> FlowFrag {
    let built = f(El::<Flow>::new());
    FlowFrag {
        markup: built.buf,
        error: built.error,
    }
}

/// An image, with alt text that cannot be empty.
///
/// EPUB marks an image decorative with `alt=""`, and `oc_model::doc::Figure::alt` is empty
/// when nothing could be derived — but Tier 1 requires every `<img>` to carry at least one
/// non-space character (the ACC-001 class, D6), and a converter that emitted `alt=""` for
/// every unlabelled figure would be declaring a book's illustrations decorative on no
/// evidence. So the *type* refuses an empty one and the caller has to decide what to say;
/// `figures.rs` is where that decision is made and documented.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImgRef {
    href: String,
    alt: String,
}

impl ImgRef {
    /// `None` when the alt text is empty or all whitespace.
    pub fn new(href: impl Into<String>, alt: impl Into<String>) -> Option<Self> {
        let alt = alt.into();
        if alt.trim().is_empty() {
            return None;
        }
        Some(Self {
            href: href.into(),
            alt,
        })
    }

    pub fn href(&self) -> &str {
        &self.href
    }

    pub fn alt(&self) -> &str {
        &self.alt
    }
}

/// Whether a list is ordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKind {
    Ordered,
    Unordered,
}

/// One cell of an emitted table.
pub struct TableCell {
    pub content: PhrasingFrag,
    pub colspan: u16,
    pub rowspan: u16,
    pub header: bool,
}

impl<C: FlowContext> El<C> {
    /// `<p>`.
    pub fn p(self, f: impl FnOnce(El<super::Phrasing>) -> El<super::Phrasing>) -> Self {
        self.child("<p>", "</p>", f)
    }

    /// `<p class="…">`, for a stanza, a caption or an epigraph line.
    pub fn p_class(
        self,
        class: CssClass,
        f: impl FnOnce(El<super::Phrasing>) -> El<super::Phrasing>,
    ) -> Self {
        self.child(&format!("<p class=\"{}\">", class.as_str()), "</p>", f)
    }

    /// `<div class="…">`, the wrapper a poem or a table fallback needs.
    pub fn div_class(self, class: CssClass, f: impl FnOnce(El<Flow>) -> El<Flow>) -> Self {
        self.child(&format!("<div class=\"{}\">", class.as_str()), "</div>", f)
    }

    /// `<div epub:type="…">`, for the block-level semantics that are not a section.
    pub fn div_type(self, kind: EpubType, f: impl FnOnce(El<Flow>) -> El<Flow>) -> Self {
        self.child(
            &format!("<div epub:type=\"{}\">", kind.as_str()),
            "</div>",
            f,
        )
    }

    /// `<blockquote>`.
    pub fn blockquote(self, f: impl FnOnce(El<Flow>) -> El<Flow>) -> Self {
        self.child("<blockquote>", "</blockquote>", f)
    }

    /// `<pre>`: a monospace block whose line breaks and spaces are content.
    ///
    /// The lines are joined with a line feed and nothing is trimmed, because in `<pre>` the
    /// whitespace *is* the text.
    pub fn pre(mut self, lines: &[String]) -> Self {
        let joined = lines.join("\n");
        match escape::check(&joined) {
            Ok(()) => {
                self.buf.push_str("<pre>");
                self.buf.push_str(&escape::text(&joined));
                self.buf.push_str("</pre>");
            }
            Err(illegal) => self.error = self.error.or(Some(illegal)),
        }
        self
    }

    /// `<hr/>`: a printed rule that is not a footnote separator.
    pub fn hr(self) -> Self {
        self.raw("<hr/>")
    }

    /// `<ol>` or `<ul>`, from items already built as flow fragments.
    ///
    /// `printed_markers` adds the class that turns the reading system's own markers off. The
    /// book printed its markers and `structure` left them inside the item text, because
    /// `epub` is Conserving and `Reason` has no variant for a list marker — so a list that
    /// drew markers of its own would show every item numbered twice
    /// (`docs/DECISIONS_LOG.md`).
    pub fn list(
        mut self,
        kind: ListKind,
        start: Option<u32>,
        printed_markers: bool,
        items: Vec<FlowFrag>,
    ) -> Self {
        let tag = match kind {
            ListKind::Ordered => "ol",
            ListKind::Unordered => "ul",
        };
        let class = if printed_markers {
            format!(" class=\"{}\"", CssClass::ListPrintedMarkers.as_str())
        } else {
            String::new()
        };
        let start = match (kind, start) {
            (ListKind::Ordered, Some(n)) if n != 1 => format!(" start=\"{n}\""),
            _ => String::new(),
        };
        self.buf.push_str(&format!("<{tag}{class}{start}>"));
        for item in items {
            self.error = self.error.or(item.error);
            self.buf.push_str("<li>");
            self.buf.push_str(&item.markup);
            self.buf.push_str("</li>");
        }
        self.buf.push_str(&format!("</{tag}>"));
        self
    }

    /// `<figure><img alt="…"/><figcaption>…</figcaption></figure>` (PIPELINE §10).
    pub fn figure(mut self, image: &ImgRef, caption: Option<&PhrasingFrag>) -> Self {
        match escape::check(image.alt()).and_then(|()| escape::check(image.href())) {
            Ok(()) => {}
            Err(illegal) => {
                self.error = self.error.or(Some(illegal));
                return self;
            }
        }
        self.buf.push_str("<figure>");
        self.buf.push_str(&format!(
            "<img src=\"{}\" alt=\"{}\"/>",
            escape::attribute(image.href()),
            escape::attribute(image.alt())
        ));
        if let Some(caption) = caption.filter(|caption| !caption.is_empty()) {
            self.error = self.error.or_else(|| caption.error.clone());
            self.buf.push_str("<figcaption>");
            self.buf.push_str(&caption.markup);
            self.buf.push_str("</figcaption>");
        }
        self.buf.push_str("</figure>");
        self
    }

    /// A real table, as a grid.
    pub fn table(mut self, caption: Option<&PhrasingFrag>, rows: Vec<Vec<TableCell>>) -> Self {
        self.buf.push_str("<table>");
        if let Some(caption) = caption.filter(|caption| !caption.is_empty()) {
            self.error = self.error.or_else(|| caption.error.clone());
            self.buf.push_str("<caption>");
            self.buf.push_str(&caption.markup);
            self.buf.push_str("</caption>");
        }
        for row in rows {
            self.buf.push_str("<tr>");
            for cell in row {
                self.error = self.error.or_else(|| cell.content.error.clone());
                let tag = if cell.header { "th" } else { "td" };
                let colspan = span_attribute("colspan", cell.colspan);
                let rowspan = span_attribute("rowspan", cell.rowspan);
                self.buf.push_str(&format!("<{tag}{colspan}{rowspan}>"));
                self.buf.push_str(&cell.content.markup);
                self.buf.push_str(&format!("</{tag}>"));
            }
            self.buf.push_str("</tr>");
        }
        self.buf.push_str("</table>");
        self
    }

    /// `<details><summary>…</summary>…</details>`: the text of a table that had to be emitted
    /// as an image.
    ///
    /// An image of a table takes the content away from anyone who cannot see it, which DAISY
    /// names as a failure in its own right (R10 §6.12). The image goes in and the extracted
    /// text goes in beside it.
    pub fn details(
        self,
        summary: &str,
        class: CssClass,
        f: impl FnOnce(El<Flow>) -> El<Flow>,
    ) -> Self {
        let summary = match escape::check(summary) {
            Ok(()) => escape::text(summary),
            Err(illegal) => return self.poison(illegal),
        };
        self.child(
            &format!(
                "<details class=\"{}\"><summary>{summary}</summary>",
                class.as_str()
            ),
            "</details>",
            f,
        )
    }

    /// The footnote half of the pop-up pattern: `<aside epub:type="footnote" id="fnN">`.
    ///
    /// On [`Flow`] and not on `Phrasing`, which is acceptance criterion A5.5: a footnote body
    /// inside a paragraph does not compile.
    pub fn aside_footnote(self, note_id: &str, f: impl FnOnce(El<Flow>) -> El<Flow>) -> Self {
        self.child(
            &format!(
                "<aside epub:type=\"{}\" role=\"doc-footnote\" id=\"{}\">",
                EpubType::Footnote.as_str(),
                escape::attribute(note_id)
            ),
            "</aside>",
            f,
        )
    }

    /// The marker a `page-list` entry points at (R5 §A4, PIPELINE §9 step 3).
    ///
    /// The label is an **attribute**, never text. `furniture` removed the printed folio from
    /// the flow under `Reason::PageNumber`, and attribute values are outside `C`
    /// (ARCHITECTURE §5.2) — which is exactly what makes that removal clean. Writing the label
    /// back as text would put the character back into the document and break the law from the
    /// other end.
    pub fn pagebreak(mut self, page_id: &str, label: &str) -> Self {
        match escape::check(label) {
            Ok(()) => self.buf.push_str(&format!(
                "<span epub:type=\"{}\" role=\"doc-pagebreak\" id=\"{}\" aria-label=\"{}\"></span>",
                EpubType::Pagebreak.as_str(),
                escape::attribute(page_id),
                escape::attribute(label)
            )),
            Err(illegal) => self.error = self.error.or(Some(illegal)),
        }
        self
    }

    /// Splice a finished run of block-level content.
    pub fn flow_frag(mut self, frag: &FlowFrag) -> Self {
        self.error = self.error.or_else(|| frag.error.clone());
        self.raw(&frag.markup)
    }

    fn poison(mut self, illegal: escape::IllegalChar) -> Self {
        self.error = self.error.or(Some(illegal));
        self
    }
}

/// `colspan`/`rowspan`, omitted when it is the default of one.
fn span_attribute(name: &str, value: u16) -> String {
    match value {
        0 | 1 => String::new(),
        n => format!(" {name}=\"{n}\""),
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2)
// ---------------------------------------------------------------------------

/// An image with no alt text is unrepresentable rather than emitted with `alt=""`, because
/// `alt=""` is a claim that the image is decorative and the pipeline rarely knows that.
#[test]
fn an_image_without_alt_text_cannot_be_built() {
    assert!(ImgRef::new("images/i1.png", "").is_none());
    assert!(ImgRef::new("images/i1.png", "   ").is_none());
    assert!(ImgRef::new("images/i1.png", "A map of the estuary").is_some());
}

/// The page-list marker carries its label as an attribute and holds no text, which is what
/// keeps the folio `furniture` removed from coming back into `C`.
#[test]
fn a_page_break_marker_holds_no_text() {
    let markup = flow(|f| f.pagebreak("page-4", "iv"));
    assert_eq!(
        markup.markup,
        "<span epub:type=\"pagebreak\" role=\"doc-pagebreak\" id=\"page-4\" \
         aria-label=\"iv\"></span>"
    );
    assert!(markup.error.is_none());
}

/// A list whose markers the book printed must not be numbered twice.
#[test]
fn a_list_that_kept_its_printed_markers_says_so() {
    use super::frag;

    let items = vec![
        flow(|f| f.p(|t| t.text("1. first"))),
        flow(|f| f.p(|t| t.text("2. second"))),
    ];
    let markup = flow(|f| f.list(ListKind::Ordered, Some(1), true, items));
    assert!(markup
        .markup
        .starts_with("<ol class=\"list-printed-markers\">"));
    assert!(markup.markup.contains("<li><p>1. first</p></li>"));

    // A list that starts elsewhere says so, and one that starts at one does not need to.
    let later = flow(|f| f.list(ListKind::Ordered, Some(7), false, Vec::new()));
    assert_eq!(later.markup, "<ol start=\"7\"></ol>");

    // An unordered list never carries `start`, whatever it is handed.
    let unordered = flow(|f| f.list(ListKind::Unordered, Some(7), false, Vec::new()));
    assert_eq!(unordered.markup, "<ul></ul>");

    let _ = frag(|t| t.text("frag is reachable from here"));
}

/// The two halves of a table that could not be trusted as a grid: the image, and the text.
#[test]
fn a_fallback_table_carries_its_text_beside_its_image() {
    use super::frag;

    let image = ImgRef::new("images/t1.png", "Table 2: rainfall by month").expect("alt is present");
    let markup = flow(|f| {
        f.figure(&image, Some(&frag(|t| t.text("Table 2"))))
            .details("Table 2 as text", CssClass::TableFallback, |body| {
                body.p(|t| t.text("January 40mm"))
            })
    });

    assert!(markup
        .markup
        .contains("<img src=\"images/t1.png\" alt=\"Table 2: rainfall by month\"/>"));
    assert!(markup.markup.contains("<figcaption>Table 2</figcaption>"));
    assert!(markup
        .markup
        .contains("<details class=\"table-fallback\"><summary>Table 2 as text</summary>"));
}
