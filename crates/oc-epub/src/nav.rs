//! `nav.xhtml`: the table of contents, the landmarks and the page list (R5 §A4).
//!
//! Written by a serialiser of its own rather than through the typed builder, and the reason is
//! that the navigation document has a *different, stricter* content model than XHTML: the
//! EPUB 3.3 spec fixes the content of a nav `<li>` as an `<a>` or a `<span>`, optionally
//! followed by one nested `<ol>`. The generic builder, which is right about ordinary flow,
//! would wrap that anchor in a `<p>` — legal XHTML, and an EPUBCheck error here.
//!
//! The grammar is small and fully determined by the tree it is handed, so there is nothing for
//! a content-model type to protect against: no caller chooses what goes inside a nav item.
//!
//! The **page list** is the part worth having. It carries the printed page numbers `furniture`
//! recovered, so a reflowed book can still be cited by print page — which Calibre does not do
//! (R1 §C.4 #7) and which is the whole reason `PageBreak` exists in the IR.

use oc_model::lang::LangTag;

use crate::content::{Emitted, Landmark, NavPoint, PageTarget};
use crate::xhtml::{escape, EpubType};

/// Where the navigation document lives inside the container.
pub const NAV_PATH: &str = "nav.xhtml";

/// Serialise `nav.xhtml`.
pub fn nav(title: &str, language: &LangTag, emitted: &Emitted, style_href: &str) -> String {
    let mut body = String::new();

    body.push_str(&format!(
        "<nav epub:type=\"{}\" role=\"doc-toc\" id=\"toc\">\n<h1>{}</h1>\n",
        EpubType::Toc.as_str(),
        escape::text("Contents")
    ));
    body.push_str(&toc_list(&emitted.toc));
    body.push_str("</nav>\n");

    if !emitted.landmarks.is_empty() {
        body.push_str(&format!(
            "<nav epub:type=\"{}\" hidden=\"hidden\">\n<h2>{}</h2>\n<ol>\n",
            EpubType::Landmarks.as_str(),
            escape::text("Landmarks")
        ));
        for landmark in &emitted.landmarks {
            body.push_str(&landmark_item(landmark));
        }
        body.push_str("</ol>\n</nav>\n");
    }

    if !emitted.page_list.is_empty() {
        body.push_str(&format!(
            "<nav epub:type=\"{}\" role=\"doc-pagelist\" hidden=\"hidden\">\n<h2>{}</h2>\n<ol>\n",
            EpubType::PageList.as_str(),
            escape::text("Page list")
        ));
        for target in &emitted.page_list {
            body.push_str(&page_item(target));
        }
        body.push_str("</ol>\n</nav>\n");
    }

    document(title, language, style_href, &body)
}

/// One level of the table of contents.
fn toc_list(entries: &[NavPoint]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut out = String::from("<ol>\n");
    for point in entries {
        out.push_str(&format!(
            "<li><a href=\"{}\">{}</a>",
            escape::attribute(&point.href),
            escape::text(&point.title)
        ));
        out.push_str(&toc_list(&point.children));
        out.push_str("</li>\n");
    }
    out.push_str("</ol>\n");
    out
}

fn landmark_item(landmark: &Landmark) -> String {
    format!(
        "<li><a epub:type=\"{}\" href=\"{}\">{}</a></li>\n",
        escape::attribute(landmark.kind),
        escape::attribute(&landmark.href),
        escape::text(&landmark.title)
    )
}

fn page_item(target: &PageTarget) -> String {
    format!(
        "<li><a href=\"{}\">{}</a></li>\n",
        escape::attribute(&target.href),
        escape::text(&target.label)
    )
}

/// The XHTML around the navs. The same shape as a content document, minus the section.
fn document(title: &str, language: &LangTag, style_href: &str, body: &str) -> String {
    let lang = escape::attribute(language.as_str());
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE html>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\" \
         xmlns:epub=\"http://www.idpf.org/2007/ops\" \
         xml:lang=\"{lang}\" lang=\"{lang}\">\n\
         <head>\n\
         <meta charset=\"utf-8\"/>\n\
         <title>{}</title>\n\
         <link rel=\"stylesheet\" type=\"text/css\" href=\"{}\"/>\n\
         </head>\n\
         <body>\n{body}</body>\n\
         </html>\n",
        escape::text(title),
        escape::attribute(style_href)
    )
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2)
// ---------------------------------------------------------------------------

#[cfg(test)]
fn sample() -> Emitted {
    Emitted {
        files: Vec::new(),
        toc: vec![NavPoint {
            title: "Chapter One".to_owned(),
            href: "text/c0001.xhtml#sec0h".to_owned(),
            children: vec![NavPoint {
                title: "A section of it".to_owned(),
                href: "text/c0001.xhtml#sec1h".to_owned(),
                children: Vec::new(),
            }],
        }],
        page_list: vec![PageTarget {
            label: "iv".to_owned(),
            href: "text/c0001.xhtml#page3".to_owned(),
        }],
        landmarks: vec![Landmark {
            kind: "bodymatter",
            title: "Chapter One".to_owned(),
            href: "text/c0001.xhtml#sec0h".to_owned(),
        }],
        used_images: Vec::new(),
    }
}

/// A nav `<li>` holds an anchor and, at most, one nested `<ol>`. Anything else — a `<p>`
/// around the anchor, a `<div>` between the levels — is an EPUBCheck error, and it is exactly
/// what a generic flow serialiser would produce.
#[test]
fn a_nav_item_is_an_anchor_and_at_most_a_nested_list() {
    let markup = nav("A Short Novel", &LangTag::EN, &sample(), "style.css");

    assert!(markup.contains(
        "<li><a href=\"text/c0001.xhtml#sec0h\">Chapter One</a><ol>\n\
         <li><a href=\"text/c0001.xhtml#sec1h\">A section of it</a></li>\n\
         </ol>\n</li>"
    ));
    assert!(
        !markup.contains("<li><p>"),
        "no paragraph between the item and its anchor"
    );
}

/// The page list is the differentiator: printed page numbers surviving into a reflowed book.
/// It is `hidden`, because it is a machine-readable index and not a second table of contents.
#[test]
fn the_page_list_carries_the_printed_folios() {
    let markup = nav("A Short Novel", &LangTag::EN, &sample(), "style.css");

    assert!(
        markup.contains("<nav epub:type=\"page-list\" role=\"doc-pagelist\" hidden=\"hidden\">")
    );
    assert!(markup.contains("<li><a href=\"text/c0001.xhtml#page3\">iv</a></li>"));
}

/// A nav with nothing in it is not emitted at all: an empty `<nav epub:type="page-list">` with
/// no `<ol>` is invalid, and an empty `<ol>` is invalid too.
#[test]
fn an_empty_nav_is_absent_rather_than_empty() {
    let bare = Emitted {
        page_list: Vec::new(),
        landmarks: Vec::new(),
        ..sample()
    };
    let markup = nav("A Short Novel", &LangTag::EN, &bare, "style.css");

    assert!(!markup.contains("page-list"));
    assert!(!markup.contains("landmarks"));
    assert!(!markup.contains("<ol>\n</ol>"));
}
