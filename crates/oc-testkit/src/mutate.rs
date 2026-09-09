//! Mutations: a valid PDF rewritten into a differently-valid PDF that must extract the same.
//!
//! The plan's Phase 1 file list names these as Python recipes under
//! `eval/src/oc_eval/mutate/`. They are written in Rust here instead, because a metamorphic
//! test has to apply the mutation and compare *in one process*: a Python step in the middle
//! would make the Rust test suite depend on an interpreter, a virtualenv and a `pikepdf`
//! wheel to run at all, and `cargo nextest` is the gate. The Python recipes still have a
//! job — Phase 7 mutates a corpus of real books, where the mutation runs once and the output
//! is a file — but the ones a test needs live here, next to the fixtures they mutate.
//!
//! A mutation is only worth having if it changes the file and not the content. Each one
//! below states which is which.

use lopdf::{Dictionary, Document, Object, ObjectId};

/// How far up a page tree an inherited attribute is looked for.
///
/// A page tree is a balanced tree over the pages, so a real book's is a handful of levels
/// deep. The bound is here so a document whose `/Parent` chain points at itself — which is a
/// malformed file, not an impossible one — stops rather than spins.
const MAX_PAGE_TREE_DEPTH: usize = 64;

/// The four numbers of a PDF rectangle.
const RECT_LEN: usize = 4;

/// What can go wrong applying a mutation. A fixture that cannot be mutated is a broken
/// fixture, so these are worth naming rather than collapsing into one string.
#[derive(Debug, thiserror::Error)]
pub enum MutateError {
    #[error("the source PDF could not be parsed: {0}")]
    Parse(String),
    #[error("the mutated PDF could not be written: {0}")]
    Write(String),
    #[error("page {page} has no usable {attribute}")]
    Missing { page: u32, attribute: &'static str },
}

/// Set `/Rotate` on every page.
///
/// **Changes:** how a reader orients the page, and therefore the shape of the page box and
/// where every glyph lands in it. **Does not change:** which characters the content stream
/// draws, or the order it draws them in — the content stream is not touched at all. That is
/// the whole point of test 1.5.
pub fn rotate(bytes: &[u8], degrees: i32) -> Result<Vec<u8>, MutateError> {
    let mut document = load(bytes)?;
    for id in page_ids(&document) {
        let dictionary = page_dictionary_mut(&mut document, id)?;
        dictionary.set("Rotate", Object::Integer(i64::from(degrees)));
    }
    save(document)
}

/// Shift every page's CropBox by `(dx, dy)`, leaving the MediaBox alone.
///
/// **Changes:** which part of the page a reader shows, and therefore the origin every
/// extracted rect is measured from. **Does not change:** the content stream, so a glyph that
/// was inside the crop before the shift and is inside it after must survive with the same
/// character. Test 1.6 asserts exactly that, because the alternative — measuring from the
/// MediaBox — is a bug that looks correct on every fixture whose CropBox starts at the
/// origin (R1 §D.6 #1).
///
/// A page with no CropBox of its own inherits one, and failing that uses its MediaBox; the
/// shift is written onto the page itself either way, which is what a real producer does.
pub fn cropbox_offset(bytes: &[u8], dx: f32, dy: f32) -> Result<Vec<u8>, MutateError> {
    let mut document = load(bytes)?;
    for (number, id) in page_ids(&document).into_iter().enumerate() {
        let page = u32::try_from(number).unwrap_or(u32::MAX);
        let current = inherited_rect(&document, id, b"CropBox")
            .or_else(|| inherited_rect(&document, id, b"MediaBox"))
            .ok_or(MutateError::Missing {
                page,
                attribute: "CropBox or MediaBox",
            })?;
        let shifted = [
            current[0] + dx,
            current[1] + dy,
            current[2] + dx,
            current[3] + dy,
        ];
        let dictionary = page_dictionary_mut(&mut document, id)?;
        dictionary.set("CropBox", rect_object(shifted));
    }
    save(document)
}

// ---------------------------------------------------------------------------
// lopdf plumbing
// ---------------------------------------------------------------------------

fn load(bytes: &[u8]) -> Result<Document, MutateError> {
    Document::load_mem(bytes).map_err(|error| MutateError::Parse(error.to_string()))
}

fn save(mut document: Document) -> Result<Vec<u8>, MutateError> {
    let mut out = Vec::new();
    document
        .save_to(&mut out)
        .map_err(|error| MutateError::Write(error.to_string()))?;
    Ok(out)
}

/// The page objects, in page order.
fn page_ids(document: &Document) -> Vec<ObjectId> {
    document.get_pages().into_values().collect()
}

fn page_dictionary_mut(
    document: &mut Document,
    id: ObjectId,
) -> Result<&mut Dictionary, MutateError> {
    document
        .get_object_mut(id)
        .and_then(Object::as_dict_mut)
        .map_err(|error| MutateError::Parse(error.to_string()))
}

/// A rectangle attribute, looked up on the page and then up its `/Parent` chain, which is
/// where the page tree puts the boxes a producer declared once for the whole document.
fn inherited_rect(document: &Document, page: ObjectId, key: &[u8]) -> Option<[f32; RECT_LEN]> {
    let mut id = page;
    for _ in 0..MAX_PAGE_TREE_DEPTH {
        let dictionary = document.get_dictionary(id).ok()?;
        if let Some(rect) = dictionary.get(key).ok().and_then(|object| {
            let resolved = resolve(document, object)?;
            rect_from(document, resolved)
        }) {
            return Some(rect);
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

/// Follow one level of indirection, which is where a producer is free to put anything.
fn resolve<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Object> {
    match object {
        Object::Reference(id) => document.get_object(*id).ok(),
        other => Some(other),
    }
}

fn rect_from(document: &Document, object: &Object) -> Option<[f32; RECT_LEN]> {
    let array = object.as_array().ok()?;
    if array.len() != RECT_LEN {
        return None;
    }
    let mut rect = [0.0f32; RECT_LEN];
    for (slot, value) in rect.iter_mut().zip(array) {
        *slot = number(resolve(document, value)?)?;
    }
    Some(rect)
}

fn number(object: &Object) -> Option<f32> {
    match object {
        Object::Integer(value) => Some(*value as f32),
        Object::Real(value) => Some(*value),
        _ => None,
    }
}

fn rect_object(rect: [f32; RECT_LEN]) -> Object {
    Object::Array(rect.iter().copied().map(Object::Real).collect())
}
