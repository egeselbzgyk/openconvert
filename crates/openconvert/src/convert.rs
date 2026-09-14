//! One conversion, end to end: PDF bytes in, container bytes out.
//!
//! Every stage, in order, each checked under the conservation law as it finishes. This is the
//! one code path — the CLI drives it, the tests drive it, and the desktop app drives the CLI
//! (D13.1). A second path that tests took and users did not would be a second set of bugs.

use std::collections::BTreeMap;

use oc_core::ledger_check::{ConservationError, ReasonTotals};
use oc_core::thresholds::Thresholds;
use oc_epub::images::SourceImage;
use oc_epub::{BuiltEpub, EpubOptions};
use oc_model::document::{Document, PresetName};
use oc_model::ids::BlockId;
use oc_model::lang::LangTag;
use oc_model::ledger::Ledger;
use oc_pdf::classify::{classify_page, PageClass};
use oc_pdf::error::PdfError;
use oc_pdf::inspect::{PdfDoc, PdfOpen};
use oc_structure::meta::{InfoDict, MetaSources};
use oc_structure::stage::StructureInput;

use crate::document::DocumentInput;
use crate::pipeline::{
    body_runs, document_stage, epub_stage, furniture_stage, layout_stage, structure_stage,
    text_stage, DocumentError, EpubStageError,
};
use crate::structure_input::{block_views, document_images};

/// What the caller chose.
pub struct ConvertOptions {
    /// The file's own name, for the last-resort title (PIPELINE §8.8).
    pub filename: String,
    /// `dc:language`, forced. `None` means the detected one.
    pub language: Option<LangTag>,
    /// The preset before [`PresetName::resolve`] has run.
    pub preset: PresetName,
    pub epub: EpubOptions,
}

/// What one conversion produced.
pub struct Conversion {
    pub document: Document,
    pub built: BuiltEpub,
    /// How many images extraction produced, for the Tier-1 parity check.
    pub extracted_images: u32,
}

/// Why a conversion failed.
#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    #[error(transparent)]
    Pdf(#[from] PdfError),
    #[error(transparent)]
    Conservation(#[from] ConservationError),
    #[error(transparent)]
    Document(#[from] DocumentError),
    #[error(transparent)]
    Epub(#[from] EpubStageError),
}

/// Convert one open document.
///
/// The backend is taken as a `&dyn PdfDoc` rather than a path, so that the whole pipeline is
/// testable without a file and so that the one place that opens a file — and therefore the one
/// place that has to get the password and the limits right — is the CLI.
pub fn convert(
    pdf: &dyn PdfDoc,
    source_sha256: &str,
    options: &ConvertOptions,
    t: &Thresholds,
) -> Result<Conversion, ConvertError> {
    let input = crate::input::page_inputs(pdf)?;
    let mut totals = ReasonTotals::default();

    let text = text_stage(&input, &mut totals, t)?;
    // Language detection is Phase 2's and is not wired into the stage driver yet, so the
    // configured tag is the one that is used; `LangTag::UND` is what a book gets when nobody
    // said. Guessing English would tell a screen reader to pronounce a German book in English.
    let language = options.language.clone().unwrap_or(LangTag::UND);

    let furniture = furniture_stage(&text, language.clone(), &mut totals, t)?;
    let layout = layout_stage(&text, &furniture, &mut totals, t)?;

    let images = document_images(&text);
    let extracted_images = u32::try_from(images.len()).unwrap_or(u32::MAX);
    let hashes = image_hashes(pdf, &images);
    let vectors = (0..pdf.page_count())
        .filter_map(|page| pdf.page_vectors(page).ok())
        .flatten()
        .collect();
    let doc_info = pdf.doc_info();

    let structure_input = StructureInput {
        blocks: block_views(&text, &layout),
        runs: body_runs(&text, &furniture),
        fonts: text.fonts.clone(),
        images: images.clone(),
        image_hashes: hashes,
        vectors,
        outline: pdf.outline(),
        labels: furniture.labels.clone(),
        drop_caps: layout.drop_caps.iter().flatten().cloned().collect(),
        page_count: pdf.page_count(),
        meta: MetaSources {
            xmp: pdf.xmp(),
            info: InfoDict {
                title: doc_info.title.clone(),
                author: doc_info.author.clone(),
            },
            filename: options.filename.clone(),
            source_sha256: source_sha256.to_owned(),
            language: language.clone(),
        },
        lang: language.clone(),
    };
    let structure = structure_stage(&layout, &structure_input, &mut totals, t)?;

    let classes: Vec<PageClass> = (0..pdf.page_count())
        .map(|page| {
            let chars = pdf.page_char_stats(page).unwrap_or_default();
            let images = pdf.page_image_stats(page).unwrap_or_default();
            classify_page(&chars, &images, None, t).0
        })
        .collect();
    let landscape: Vec<bool> = layout
        .pages
        .iter()
        .map(|page| page.width_pt > page.height_pt)
        .collect();
    let column_counts: Vec<usize> = layout
        .columns
        .iter()
        .map(oc_layout::columns::ColumnLayout::count)
        .collect();
    let block_pages: BTreeMap<BlockId, u32> = layout
        .blocks
        .iter()
        .flatten()
        .map(|block| (block.id, block.page.index))
        .collect();

    let mut ledger = Ledger {
        c_raw: text.c_0.clone(),
        c_0: text.c_0.clone(),
        ..Ledger::default()
    };
    ledger.push_stage(&text.delta, text.check.clone());
    ledger.push_stage(&furniture.delta, furniture.check.clone());
    ledger.push_stage(&layout.delta, layout.check.clone());
    ledger.push_stage(&structure.delta, structure.check.clone());

    let document = document_stage(
        &structure,
        DocumentInput {
            source_sha256,
            structure: &structure.output,
            labels: &furniture.labels,
            classes: &classes,
            landscape: &landscape,
            column_counts: &column_counts,
            block_pages: &block_pages,
            images: &images,
            language,
            preset: options.preset,
            ledger,
        },
        &mut totals,
        t,
    )?;

    let sources = decode_images(pdf, &images);
    let epub = epub_stage(&document.document, &sources, &options.epub, &mut totals)?;

    let mut document = document.document;
    document.ledger.push_stage(&epub.delta, epub.check.clone());

    Ok(Conversion {
        document,
        built: epub.built,
        extracted_images,
    })
}

/// Open a document and convert it.
pub fn convert_bytes(
    backend: &dyn PdfOpen,
    bytes: &[u8],
    password: Option<&str>,
    options: &ConvertOptions,
    t: &Thresholds,
) -> Result<Conversion, ConvertError> {
    let pdf = backend.open(bytes, password)?;
    convert(pdf.as_ref(), &sha256_hex(bytes), options, t)
}

/// The lowercase hex SHA-256 of the input bytes.
///
/// `dc:identifier` is minted from it, which is what makes a re-conversion of the same file the
/// same book to a reading system rather than a new one (R5 §A2).
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Every image, decoded to RGBA for the encoder.
fn decode_images(pdf: &dyn PdfDoc, images: &[oc_model::extract::ImageRef]) -> Vec<SourceImage> {
    images
        .iter()
        .map(|image| {
            let decoded = decode_one(pdf, images, image);
            SourceImage {
                id: image.id,
                width: decoded.width,
                height: decoded.height,
                rgba: decoded.rgba,
            }
        })
        .collect()
}

/// One image's perceptual hash per image, for the ornament rule.
fn image_hashes(pdf: &dyn PdfDoc, images: &[oc_model::extract::ImageRef]) -> Vec<u64> {
    images
        .iter()
        .map(|image| oc_pdf::images::perceptual_hash(&decode_one(pdf, images, image)))
        .collect()
}

/// Decode one image, translating the document-wide id back to the page-local one.
///
/// `ImageId` means two things and this is the boundary: the backend numbers images per *page*,
/// because `image_bytes` indexes that page's draw order, while a `Figure` names one picture in
/// the whole book. The page-local index is recoverable as the image's position among those
/// sharing its page, which document order preserves.
fn decode_one(
    pdf: &dyn PdfDoc,
    images: &[oc_model::extract::ImageRef],
    image: &oc_model::extract::ImageRef,
) -> oc_pdf::images::DecodedImage {
    let local = images
        .iter()
        .filter(|other| other.page.index == image.page.index)
        .position(|other| other.id == image.id)
        .unwrap_or_default();
    pdf.image_bytes(
        image.page.index,
        oc_model::extract::ImageId(u32::try_from(local).unwrap_or_default()),
    )
    .unwrap_or(oc_pdf::images::DecodedImage {
        // A one-pixel opaque black square, so that an image the backend could not decode
        // becomes a visibly wrong picture rather than a panic or a dropped figure. The
        // conversion report is where it is named.
        width: 1,
        height: 1,
        rgba: vec![0, 0, 0, 255],
    })
}
