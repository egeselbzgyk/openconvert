//! An anchor inside an anchor: a footnote backlink written inside a note reference.

use oc_epub::xhtml::frag;

fn main() {
    // The closure of `link` is handed an `El<NoAnchor>`, which has no `link`…
    let _ = frag(|text| text.link("#outer", |inner| inner.link("#inner", |deepest| deepest)));
    // …and no `noteref`.
    let _ = frag(|text| text.link("#outer", |inner| inner.noteref("fn1", "1")));
}
