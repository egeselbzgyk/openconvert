//! The structural half of image extraction, read from the file rather than from PDFium.
//!
//! PDFium places an image and counts its pixels, but its object API says nothing about two
//! things `ImageRef` has to carry: whether the image has a soft or stencil mask, and whether
//! it was written inline in the content stream. Both are properties of the file, so both are
//! read from the file with `lopdf`.
//!
//! The two sides are matched **by draw order**. PDFium enumerates page objects in
//! content-stream order, and the walk below records image draws in the same order, so the
//! *k*-th entry here belongs to the *k*-th PDFium image object. When the two disagree on how
//! many there are — a form XObject that PDFium flattens differently, a content stream `lopdf`
//! declines to parse — the flags are reported as `false` for every image on the page rather
//! than guessed. A missing mask makes the compositing path do slightly more work than it
//! needed to; a *wrongly asserted* mask would make it drop pixels.

use lopdf::{Document, Object, ObjectId};
use oc_core::limits::Limits;

use crate::error::PdfError;

/// What the file says about one image draw, in the order the page draws them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ImageFacts {
    pub(crate) has_smask: bool,
    pub(crate) is_inline: bool,
}

/// The image draws of one page, in content-stream order.
///
/// `Ok(None)` when the page's structure could not be read at all, which the caller reports as
/// a page whose image flags are unknown rather than as a page without masks. `Err` only when
/// a *limit* refused the page — a decompression bomb is not a parse failure to shrug at, and
/// degrading to "no masks here" would be reading a hostile stream and then ignoring it.
pub(crate) fn page_image_facts(
    document: &Document,
    index: u32,
    limits: &Limits,
) -> Result<Option<Vec<ImageFacts>>, PdfError> {
    let Some(page_id) = page_id(document, index) else {
        return Ok(None);
    };
    // The cap is applied here, on the read itself, rather than being checked afterwards on a
    // buffer that has already been allocated.
    let raw = crate::limits::read_page_content(document, page_id, limits)?;
    let Ok(content) = lopdf::content::Content::decode(&raw) else {
        return Ok(None);
    };
    let resources = xobject_resources(document, page_id);

    let mut facts = Vec::new();
    for operation in &content.operations {
        match operation.operator.as_str() {
            // `BI … ID … EI`: the image is written where it is drawn. `lopdf` hands the whole
            // thing back as one stream object, so its dictionary is readable directly.
            "BI" => facts.push(ImageFacts {
                has_smask: operation.operands.iter().any(declares_mask_inline),
                is_inline: true,
            }),
            // `/Name Do`: an XObject, which is an image only if it says so.
            "Do" => {
                let Some(name) = operation.operands.first().and_then(|o| o.as_name().ok()) else {
                    continue;
                };
                let Some(stream) = resources
                    .as_ref()
                    .and_then(|resources| resources.get(name).ok())
                    .and_then(|object| resolve(document, object))
                else {
                    continue;
                };
                let Ok(stream) = stream.as_stream() else {
                    continue;
                };
                if stream
                    .dict
                    .get(b"Subtype")
                    .ok()
                    .and_then(|s| s.as_name().ok())
                    != Some(SUBTYPE_IMAGE)
                {
                    continue;
                }
                facts.push(ImageFacts {
                    has_smask: declares_mask(&stream.dict),
                    is_inline: false,
                });
            }
            _ => {}
        }
    }
    Ok(Some(facts))
}

/// `/Subtype /Image`, the only XObject kind that is one.
const SUBTYPE_IMAGE: &[u8] = b"Image";

/// How far up the page tree `/Resources` is looked for; a page tree is shallow, and the bound
/// stops a `/Parent` chain that points at itself.
const MAX_PAGE_TREE_DEPTH: usize = 64;

/// Both ways a PDF says "part of this image is transparent".
///
/// `/SMask` is the modern alpha channel; `/Mask` is either a stencil mask or a colour-key
/// range. They mean different things to a compositor and the same thing here: the image is
/// not a plain rectangle of pixels.
fn declares_mask(dictionary: &lopdf::Dictionary) -> bool {
    dictionary.has(b"SMask") || dictionary.has(b"Mask")
}

/// The same question for an inline image, whose dictionary uses the abbreviated keys.
fn declares_mask_inline(operand: &Object) -> bool {
    let Ok(stream) = operand.as_stream() else {
        return false;
    };
    // `/IM` is the inline abbreviation for `/ImageMask`; `/SMask` has no abbreviation and is
    // not legal inline, but a file that writes one anyway is telling us something true.
    declares_mask(&stream.dict) || stream.dict.has(b"IM") || stream.dict.has(b"ImageMask")
}

/// The page object for a zero-based page index.
fn page_id(document: &Document, index: u32) -> Option<ObjectId> {
    document
        .get_pages()
        .into_values()
        .nth(usize::try_from(index).ok()?)
}

/// The page's `/Resources /XObject` dictionary, inherited from the page tree if the page does
/// not carry one itself.
fn xobject_resources(document: &Document, page: ObjectId) -> Option<lopdf::Dictionary> {
    let mut id = page;
    for _ in 0..MAX_PAGE_TREE_DEPTH {
        let dictionary = document.get_dictionary(id).ok()?;
        if let Some(xobjects) = dictionary
            .get(b"Resources")
            .ok()
            .and_then(|object| resolve(document, object))
            .and_then(|object| object.as_dict().ok())
            .and_then(|resources| resources.get(b"XObject").ok())
            .and_then(|object| resolve(document, object))
            .and_then(|object| object.as_dict().ok())
        {
            return Some(xobjects.clone());
        }
        id = dictionary
            .get(b"Parent")
            .ok()
            .and_then(|parent| match parent {
                Object::Reference(parent) => Some(*parent),
                _ => None,
            })?;
    }
    None
}

fn resolve<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Object> {
    match object {
        Object::Reference(id) => document.get_object(*id).ok(),
        other => Some(other),
    }
}
