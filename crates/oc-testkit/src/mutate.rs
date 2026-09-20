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
    /// The mutation found nothing to change, which means the fixture is not the one the test
    /// thinks it is. Silently returning the input would make the test pass for the wrong
    /// reason, which is the failure a mutation test exists to prevent.
    #[error("nothing to mutate: the document has no {what}")]
    Absent { what: &'static str },
    #[error("the document could not be encrypted: {0}")]
    Encrypt(String),
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

/// The key that maps a font's character codes back to Unicode.
const TO_UNICODE: &[u8] = b"ToUnicode";

/// Delete `/ToUnicode` from every object that carries one.
///
/// **Changes:** whether anything can say what the characters *are*. **Does not change:** a
/// single mark on the page — the glyphs are selected by code and drawn from the embedded
/// font either way, so a reader sees exactly the same ink.
///
/// That gap is the most common reason a real PDF is unconvertible (R2 §B.8), and it is why
/// `broken_text` is a page class rather than an error: the honest answer is to route the page
/// to OCR, not to emit whatever the code points happened to decode to.
///
/// Every object is visited rather than only the fonts reachable from a page's resources: a
/// font referenced from a Form XObject, an annotation appearance or a pattern is a font too,
/// and one surviving `/ToUnicode` would leave part of the document decodable.
pub fn strip_tounicode(bytes: &[u8]) -> Result<Vec<u8>, MutateError> {
    let mut document = load(bytes)?;
    let mut removed = 0usize;
    for object in document.objects.values_mut() {
        let dictionary = match object {
            Object::Dictionary(dictionary) => Some(dictionary),
            Object::Stream(stream) => Some(&mut stream.dict),
            _ => None,
        };
        if let Some(dictionary) = dictionary {
            if dictionary.remove(TO_UNICODE).is_some() {
                removed += 1;
            }
        }
    }
    if removed == 0 {
        return Err(MutateError::Absent {
            what: "/ToUnicode to strip",
        });
    }
    save(document)
}

/// Delete the structure tree from the catalogue.
///
/// **Changes:** whether the file says what its parts *are* — headings, lists, figures,
/// reading order — in machine-readable form. **Does not change:** a single mark on the page.
///
/// D18's reason for keeping the tagged bucket at the real world's ~12.6 %: a pipeline tuned on
/// tagged input has been tuned on the 12.6 %. Stripping the tree from a document that has one
/// is how the other 87.4 % is simulated from a file whose true structure is known.
pub fn strip_structtree(bytes: &[u8]) -> Result<Vec<u8>, MutateError> {
    let mut document = load(bytes)?;

    let catalogue = document
        .catalog_mut()
        .map_err(|error| MutateError::Parse(error.to_string()))?;
    let had_tree = catalogue.remove(b"StructTreeRoot").is_some();
    catalogue.remove(b"MarkInfo");

    if !had_tree {
        return Err(MutateError::Absent {
            what: "/StructTreeRoot to strip",
        });
    }

    // `/StructParents` on a page points into the tree that is now gone. A dangling key is
    // worse than no key: a reader that trusts it looks up a number in nothing.
    for id in page_ids(&document) {
        page_dictionary_mut(&mut document, id)?.remove(b"StructParents");
    }

    // The marked-content operators in the streams are left alone on purpose. A real untagged
    // PDF from a producer that once tagged it looks exactly like this — `/P <</MCID 0>> BDC`
    // with nothing to resolve the MCID against — and that is the file the pipeline must
    // survive.
    save(document)
}

/// Draw every page's content a second time, exactly on top of itself.
///
/// **Changes:** how many times each glyph is painted, and therefore `C_raw`, which doubles.
/// **Does not change:** what a reader sees — the second pass lands on the first — or which
/// characters the file contains once the overdraw is collapsed.
///
/// This is the mutation the `OverdrawDedup` reason exists for. A producer that draws a page
/// twice to fake bold, or a tool that merged two layers, leaves exactly this, and a converter
/// that does not collapse it emits every word twice.
pub fn double_draw(bytes: &[u8]) -> Result<Vec<u8>, MutateError> {
    rewrite_content(bytes, |content| {
        let original = content.operations.clone();
        content.operations.extend(original);
    })
}

/// Add an invisible copy of every page's text, in render mode 3.
///
/// **Changes:** how many text-showing operations the page has, and therefore `C_raw`.
/// **Does not change:** one pixel of what is drawn — render mode 3 paints nothing.
///
/// This is the shape of an OCR sandwich (D13.10): a scanned page carries an image and an
/// invisible text layer the OCR engine produced, and a converter that reads both emits the
/// page twice. Doing it to a born-digital fixture makes the duplicate exactly detectable — the
/// visible and invisible runs carry the same characters at the same positions — which is what
/// `OcrLayerDuplicate` has to recognise.
pub fn ocr_sandwich(bytes: &[u8]) -> Result<Vec<u8>, MutateError> {
    rewrite_content(bytes, |content| {
        let mut invisible = Vec::with_capacity(content.operations.len() + 1);
        for operation in &content.operations {
            invisible.push(operation.clone());
            if operation.operator == "BT" {
                invisible.push(lopdf::content::Operation::new(
                    "Tr",
                    vec![Object::Integer(INVISIBLE_RENDER_MODE)],
                ));
            }
        }
        content.operations.extend(invisible);
    })
}

/// Move every text-positioning operator sideways by a small, fixed-per-operator amount.
///
/// **Changes:** where each run starts, by a fraction of a point. **Does not change:** which
/// characters are drawn, or in what order.
///
/// Word segmentation is a decision about gaps (R2 §B.3), and a gap that is 2.00 pt in one book
/// and 1.97 pt in the next must not change the answer. The offsets come from a fixed sequence
/// rather than a random source, because a mutation a test cannot reproduce is a mutation
/// nobody can debug.
pub fn jitter_spacing(bytes: &[u8]) -> Result<Vec<u8>, MutateError> {
    rewrite_content(bytes, |content| {
        let mut step = 0u32;
        for operation in &mut content.operations {
            if !matches!(operation.operator.as_str(), "Td" | "TD") {
                continue;
            }
            let Some(first) = operation.operands.first() else {
                continue;
            };
            let Some(x) = number(first) else {
                continue;
            };
            operation.operands[0] = Object::Real(x + jitter(step));
            step += 1;
        }
    })
}

/// Point `startxref` at nothing, so a reader has to rebuild the cross-reference table.
///
/// **Changes:** whether the file can be read by following its own index. **Does not change:** a
/// single object — every one of them is still there, at the offset it was always at.
///
/// Real files arrive like this constantly: a truncated download, a naive concatenation, a tool
/// that appended an incremental update and miscounted. Every serious reader reconstructs the
/// table by scanning for `obj` markers, and D13.2's point is that we let it rather than refuse
/// the file.
pub fn damage_xref(bytes: &[u8]) -> Result<Vec<u8>, MutateError> {
    let marker = b"startxref";
    let start = bytes
        .windows(marker.len())
        .rposition(|window| window == marker)
        .ok_or(MutateError::Absent {
            what: "startxref to damage",
        })?;

    let mut damaged = bytes[..start].to_vec();
    damaged.extend_from_slice(b"startxref\n");
    damaged.extend_from_slice(BOGUS_XREF_OFFSET.as_bytes());
    damaged.extend_from_slice(b"\n%%EOF\n");
    Ok(damaged)
}

/// The offset a damaged `startxref` points at: past the end of any fixture, so a reader that
/// trusts it finds nothing and a reader that rebuilds finds everything.
const BOGUS_XREF_OFFSET: &str = "999999999";

/// PDF text render mode 3: fill nothing, stroke nothing, clip nothing.
const INVISIBLE_RENDER_MODE: i64 = 3;

/// The jitter applied to the nth text-positioning operator, in points.
///
/// A fixed sequence, not a random one, and deliberately far smaller than the smallest gap that
/// separates two words at any realistic size: the mutation is a perturbation the pipeline
/// should shrug off, not a different layout.
fn jitter(step: u32) -> f32 {
    /// Peak displacement. A thirtieth of a point is below the resolution at which any
    /// typographic distinction is made, and well below `words.gap_resolution_pt`.
    const AMPLITUDE_PT: f32 = 0.03;
    /// How many operators before the sequence repeats. Coprime with nothing in particular —
    /// it just has to be short enough that a two-line fixture sees more than one value.
    const PERIOD: u32 = 7;

    let phase = f32::from(u16::try_from(step % PERIOD).unwrap_or(0)) / PERIOD as f32;
    AMPLITUDE_PT * (2.0 * phase - 1.0)
}

/// Decode every page's content stream, hand it to `edit`, and write it back.
fn rewrite_content(
    bytes: &[u8],
    mut edit: impl FnMut(&mut lopdf::content::Content),
) -> Result<Vec<u8>, MutateError> {
    let mut document = load(bytes)?;
    let pages = page_ids(&document);
    if pages.is_empty() {
        return Err(MutateError::Absent { what: "pages" });
    }

    for (index, id) in pages.iter().enumerate() {
        let mut content = document
            .get_and_decode_page_content(*id)
            .map_err(|error| MutateError::Parse(error.to_string()))?;
        if content.operations.is_empty() {
            return Err(MutateError::Missing {
                page: u32::try_from(index).unwrap_or(u32::MAX),
                attribute: "content stream",
            });
        }
        edit(&mut content);
        let encoded = content
            .encode()
            .map_err(|error| MutateError::Write(error.to_string()))?;
        document
            .change_page_content(*id, encoded)
            .map_err(|error| MutateError::Write(error.to_string()))?;
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

/// How a fixture is encrypted.
#[derive(Clone, Copy, Debug)]
pub struct EncryptOptions<'a> {
    /// The password that grants full rights. Never empty in a fixture: an owner password is
    /// what makes the permission flags mean anything.
    pub owner_password: &'a str,
    /// The password a reader is asked for. Empty means "no password needed", which is by far
    /// the commonest form of encrypted PDF in the wild — the file is encrypted to carry
    /// permission flags, not to keep anyone out.
    pub user_password: &'a str,
    /// Whether the permission flags allow printing. The flag every test of D13.11 turns off,
    /// because it is the one a converter is most often expected to obey and must not.
    pub allow_printing: bool,
}

/// Encrypt a document with AES-128 (security handler V4, revision 4).
///
/// **Changes:** how the file's strings and streams are stored, and what permissions it
/// declares. **Does not change:** a single character of its content, which is the whole point
/// of D13.11 — an encrypted book is a book, and the flags inside it are a request to a viewer
/// rather than a lock on the text.
///
/// AES-128 rather than the 40-bit RC4 of PDF 1.4 or the AES-256 of PDF 2.0 because it is what
/// the overwhelming majority of encrypted PDFs in circulation actually use, and what test 1.12
/// names.
pub fn encrypt(bytes: &[u8], options: EncryptOptions<'_>) -> Result<Vec<u8>, MutateError> {
    use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
    use lopdf::encryption::{EncryptionState, EncryptionVersion, Permissions};
    use std::collections::BTreeMap;
    use std::sync::Arc;

    /// The conventional name for the standard crypt filter; readers look for exactly this.
    const STANDARD_FILTER: &[u8] = b"StdCF";

    let mut document = load(bytes)?;

    let mut permissions = Permissions::all();
    if !options.allow_printing {
        permissions.remove(Permissions::PRINTABLE);
        permissions.remove(Permissions::PRINTABLE_IN_HIGH_QUALITY);
    }

    let mut crypt_filters: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
    crypt_filters.insert(STANDARD_FILTER.to_vec(), Arc::new(Aes128CryptFilter));

    let state = EncryptionState::try_from(EncryptionVersion::V4 {
        document: &document,
        encrypt_metadata: true,
        crypt_filters,
        stream_filter: STANDARD_FILTER.to_vec(),
        string_filter: STANDARD_FILTER.to_vec(),
        owner_password: options.owner_password,
        user_password: options.user_password,
        permissions,
    })
    .map_err(|error| MutateError::Encrypt(error.to_string()))?;

    document
        .encrypt(&state)
        .map_err(|error| MutateError::Encrypt(error.to_string()))?;
    save(document)
}
