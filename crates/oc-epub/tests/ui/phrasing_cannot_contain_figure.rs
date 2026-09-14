//! A figure inside a paragraph: the violation D5 names first, and acceptance criterion A5.5.

use oc_epub::xhtml::{frag, ImgRef};

fn main() {
    let Some(image) = ImgRef::new("images/i1.png", "A map of the estuary") else {
        return;
    };
    // `figure` lives on `FlowContext`, and `Phrasing` is not one.
    let _ = frag(|text| text.figure(&image, None));
}
