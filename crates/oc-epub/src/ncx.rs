//! `toc.ncx`: the EPUB 2 navigation map (R5 §A4).
//!
//! Formally superseded in EPUB 3.3 and emitted anyway. It costs a few hundred bytes, it is
//! derived from the same tree `nav.xhtml` is built from, and it is meaningfully better on older
//! e-ink firmware — several shipping readers still navigate by the NCX and show nothing at all
//! without one.
//!
//! Because both come from one tree, the two can only disagree through a bug, and test 5.8
//! (`nav_and_ncx_agree`) is what makes that a checkable claim rather than an argument.

use crate::content::NavPoint;
use crate::xhtml::escape;

/// Where the legacy navigation map lives inside the container.
pub const NCX_PATH: &str = "toc.ncx";

/// Serialise `toc.ncx`.
pub fn ncx(title: &str, identifier: &str, points: &[NavPoint]) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str(
        "<ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\">\n<head>\n",
    );
    out.push_str(&format!(
        "<meta name=\"dtb:uid\" content=\"{}\"/>\n",
        escape::attribute(identifier)
    ));
    // The three counters EPUB 2 required and nobody reads. Zero is the spec's own value for
    // "not applicable", and inventing a depth would be a number with no meaning behind it.
    out.push_str(&format!(
        "<meta name=\"dtb:depth\" content=\"{}\"/>\n\
         <meta name=\"dtb:totalPageCount\" content=\"0\"/>\n\
         <meta name=\"dtb:maxPageNumber\" content=\"0\"/>\n",
        depth(points)
    ));
    out.push_str("</head>\n");
    out.push_str(&format!(
        "<docTitle><text>{}</text></docTitle>\n",
        escape::text(title)
    ));

    out.push_str("<navMap>\n");
    let mut order = 0u32;
    out.push_str(&nav_points(points, &mut order));
    out.push_str("</navMap>\n</ncx>\n");
    out
}

/// One level of the navigation map.
///
/// `playOrder` counts across the whole document, depth first, which is what the format means by
/// it: the order a reader meets the headings, not their position among their siblings.
fn nav_points(points: &[NavPoint], order: &mut u32) -> String {
    let mut out = String::new();
    for point in points {
        *order = order.saturating_add(1);
        let id = *order;
        out.push_str(&format!(
            "<navPoint id=\"navpoint-{id}\" playOrder=\"{id}\">\n\
             <navLabel><text>{}</text></navLabel>\n\
             <content src=\"{}\"/>\n",
            escape::text(&point.title),
            escape::attribute(&point.href)
        ));
        out.push_str(&nav_points(&point.children, order));
        out.push_str("</navPoint>\n");
    }
    out
}

/// How deep the tree goes.
fn depth(points: &[NavPoint]) -> u32 {
    points
        .iter()
        .map(|point| 1 + depth(&point.children))
        .max()
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2)
// ---------------------------------------------------------------------------

#[cfg(test)]
fn sample() -> Vec<NavPoint> {
    vec![
        NavPoint {
            title: "Chapter One".to_owned(),
            href: "text/c0001.xhtml#sec0h".to_owned(),
            children: vec![NavPoint {
                title: "A section of it".to_owned(),
                href: "text/c0001.xhtml#sec1h".to_owned(),
                children: Vec::new(),
            }],
        },
        NavPoint {
            title: "Chapter Two".to_owned(),
            href: "text/c0002.xhtml#sec2h".to_owned(),
            children: Vec::new(),
        },
    ]
}

/// `playOrder` is the order a reader meets the headings, across the whole book and depth first
/// — not the position of a heading among its siblings. A reader that trusts it and finds two
/// entries claiming order 2 navigates to the wrong place.
#[test]
fn play_order_counts_across_the_whole_document_depth_first() {
    let markup = ncx("A Short Novel", "urn:uuid:0", &sample());

    let orders: Vec<&str> = markup
        .match_indices("playOrder=\"")
        .map(|(index, needle)| {
            let rest = &markup[index + needle.len()..];
            &rest[..rest.find('"').unwrap_or(0)]
        })
        .collect();
    assert_eq!(orders, vec!["1", "2", "3"]);
    assert!(markup.contains("<meta name=\"dtb:depth\" content=\"2\"/>"));
}

/// The NCX is derived from the same tree as the nav, so the labels and the order are the same
/// labels and the same order. Test 5.8 asserts that on every fixture; this asserts the
/// serialiser does not reorder on its own.
#[test]
fn the_navigation_map_keeps_the_trees_own_order() {
    let markup = ncx("A Short Novel", "urn:uuid:0", &sample());
    let labels: Vec<usize> = ["Chapter One", "A section of it", "Chapter Two"]
        .iter()
        .filter_map(|label| markup.find(label))
        .collect();
    assert_eq!(labels.len(), 3);
    assert!(labels.windows(2).all(|pair| pair[0] < pair[1]));
}
