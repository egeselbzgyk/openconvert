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
use oc_model::doc::{Severity, Warning};
use oc_model::document::{Document, PresetName};
use oc_model::ids::BlockId;
use oc_model::lang::LangTag;
use oc_model::ledger::{Ledger, LedgerDelta};
use oc_pdf::classify::{classify_page, PageClass};
use oc_pdf::error::PdfError;
use oc_pdf::inspect::{PdfDoc, PdfOpen};
use oc_structure::meta::{InfoDict, MetaSources};
use oc_structure::stage::StructureInput;

use crate::document::DocumentInput;
use crate::pipeline::{
    body_runs, document_stage, epub_check, furniture_stage, layout_stage, structure_stage,
    text_stage, validate_repair_stage, DocumentError, EpubStageError, ValidateRepairError,
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

/// Per-stage wall-clock, in milliseconds, in stage order.
///
/// An `IndexMap` would do as well; a `Vec` of pairs is used because the report prints them in stage
/// order and a map would invite someone to sort them by name, which is not the order anybody wants
/// to read a pipeline in.
#[derive(Clone, Debug, Default)]
pub struct Timings(Vec<(&'static str, u64)>);

impl Timings {
    /// Time one stage.
    fn stage<T, E>(
        &mut self,
        name: &'static str,
        run: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        let started = std::time::Instant::now();
        let out = run();
        let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.0.push((name, elapsed));
        out
    }

    pub fn as_slice(&self) -> &[(&'static str, u64)] {
        &self.0
    }
}

/// What one conversion produced.
pub struct Conversion {
    pub document: Document,
    pub built: BuiltEpub,
    /// How many images extraction produced, for the Tier-1 parity check.
    pub extracted_images: u32,
    /// What Tier 1 found in the container that was written.
    pub tier1: oc_validate::Tier1Report,
    /// I-7, retention, heading sanity, duplicates, the Gopher statistics.
    pub structural: oc_validate::structural::StructuralReport,
    /// How the validate→repair loop ended, what fired, and what remains.
    pub repair: oc_validate::repair::RepairOutcome,
    /// Per-stage wall-clock, for the report.
    pub timings: Timings,
    /// The producer family, which is also the stratum the corpus is reported by (D18).
    ///
    /// Read from the file's own `/Producer` and `/Creator` rather than from `inspect`'s report:
    /// `convert` does not run `inspect`, and the family is a pure function of two strings.
    pub producer_family: oc_pdf::producer::ProducerFamily,
    /// How many pages of each class the document has (D13.10), by the class's report name.
    pub page_classes: BTreeMap<String, u32>,
    /// Every choice the deterministic evidence could not settle, with the evidence — written to
    /// the report whether or not a model was asked (PHASE 10 detail 1).
    pub escalations: Vec<oc_structure::escalate::EscalationRecord>,
    /// What the AI step did, when it ran. `None` with AI off — the v1 default.
    pub ai: Option<crate::ai::AiOutcome>,
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
    #[error(transparent)]
    ValidateRepair(#[from] ValidateRepairError),
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
    convert_with_ai(pdf, source_sha256, options, None, t)
}

/// Convert one open document, asking a model where the escalations say to when `ai` is given
/// (PHASE 10). `None` is the v1 default, `ai.enabled = false`, and is exactly [`convert`].
pub fn convert_with_ai(
    pdf: &dyn PdfDoc,
    source_sha256: &str,
    options: &ConvertOptions,
    ai: Option<&crate::ai::AiContext<'_>>,
    t: &Thresholds,
) -> Result<Conversion, ConvertError> {
    let prepared = prepare(pdf, source_sha256, options, t)?;
    convert_prepared(pdf, source_sha256, options, prepared, ai, t)
}

/// Everything from `structure` on, from what [`prepare`] produced — the seam a test uses to
/// change a book's evidence (its outline, its declared title) before `structure` reads it.
pub fn convert_prepared(
    pdf: &dyn PdfDoc,
    source_sha256: &str,
    options: &ConvertOptions,
    prepared: Prepared,
    ai: Option<&crate::ai::AiContext<'_>>,
    t: &Thresholds,
) -> Result<Conversion, ConvertError> {
    let Prepared {
        mut timings,
        mut totals,
        text,
        furniture,
        layout,
        images,
        language,
        doc_info,
        structure_input,
    } = prepared;
    let extracted_images = u32::try_from(images.len()).unwrap_or(u32::MAX);

    let mut structure = timings.stage("structure", || {
        structure_stage(&layout, &structure_input, &mut totals, t)
    })?;
    // Every escalation is recorded whether or not a model is ever asked: the first books
    // converted are the calibration corpus (PHASE 10 detail 1, RT A7.2).
    let escalations =
        oc_structure::escalate::Escalations::gather(&structure_input, &structure.output, t);

    // The AI step, when asked for: the admitted edits are applied by running `structure` again
    // with them, and that run is checked under the conservation law like the first.
    let ai = match ai {
        Some(context) => {
            let deterministic = structure.output;
            let (_, outcome) = timings.stage("ai", || {
                Ok::<_, ConvertError>(crate::ai::run(
                    context,
                    &structure_input,
                    deterministic,
                    &escalations,
                    t,
                ))
            })?;
            structure = crate::pipeline::structure_stage_with(
                &layout,
                &structure_input,
                &outcome.edits,
                &mut totals,
                t,
            )?;
            Some(outcome)
        }
        None => None,
    };

    finish(
        pdf,
        source_sha256,
        options,
        t,
        Finishing {
            timings,
            totals,
            text,
            furniture,
            layout,
            images,
            extracted_images,
            language,
            doc_info,
            structure,
            escalations,
            ai,
        },
    )
}

/// Everything a conversion knows when `structure` is about to run: the four stages before it,
/// checked, and the stage's input. The AI step and the tests that drive `structure` directly
/// start here.
pub struct Prepared {
    pub timings: Timings,
    pub totals: ReasonTotals,
    pub text: crate::pipeline::TextStage,
    pub furniture: crate::pipeline::FurnitureStage,
    pub layout: crate::pipeline::LayoutStage,
    pub images: Vec<oc_model::extract::ImageRef>,
    pub language: LangTag,
    pub doc_info: oc_pdf::inspect::DocMetadata,
    pub structure_input: StructureInput,
}

/// `ingest`, `text`, `furniture` and `layout`, each checked, and `structure`'s input.
pub fn prepare(
    pdf: &dyn PdfDoc,
    source_sha256: &str,
    options: &ConvertOptions,
    t: &Thresholds,
) -> Result<Prepared, ConvertError> {
    let mut timings = Timings::default();
    let input = timings.stage("ingest", || crate::input::page_inputs(pdf))?;
    let mut totals = ReasonTotals::default();

    let text = timings.stage("text", || text_stage(&input, &mut totals, t))?;
    // Language detection is Phase 2's and is not wired into the stage driver yet, so the
    // configured tag is the one that is used; `LangTag::UND` is what a book gets when nobody
    // said. Guessing English would tell a screen reader to pronounce a German book in English.
    let language = options.language.clone().unwrap_or(LangTag::UND);

    let furniture = timings.stage("furniture", || {
        furniture_stage(&text, language.clone(), &mut totals, t)
    })?;
    let layout = timings.stage("layout", || layout_stage(&text, &furniture, &mut totals, t))?;

    let images = document_images(&text);
    let hashes = image_hashes(pdf, &images, t);
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
    Ok(Prepared {
        timings,
        totals,
        text,
        furniture,
        layout,
        images,
        language,
        doc_info,
        structure_input,
    })
}

/// Everything `finish` needs: what `prepare` produced, and `structure`'s settled output.
struct Finishing {
    timings: Timings,
    totals: ReasonTotals,
    text: crate::pipeline::TextStage,
    furniture: crate::pipeline::FurnitureStage,
    layout: crate::pipeline::LayoutStage,
    images: Vec<oc_model::extract::ImageRef>,
    extracted_images: u32,
    language: LangTag,
    doc_info: oc_pdf::inspect::DocMetadata,
    structure: crate::pipeline::StructureStage,
    escalations: oc_structure::escalate::Escalations,
    ai: Option<crate::ai::AiOutcome>,
}

/// `document`, `epub`, `validate` and `repair`, from a settled `structure`.
fn finish(
    pdf: &dyn PdfDoc,
    source_sha256: &str,
    options: &ConvertOptions,
    t: &Thresholds,
    finishing: Finishing,
) -> Result<Conversion, ConvertError> {
    let Finishing {
        mut timings,
        mut totals,
        text,
        furniture,
        layout,
        images,
        extracted_images,
        language,
        doc_info,
        structure,
        escalations,
        ai,
    } = finishing;

    let producer_family = oc_pdf::producer::producer_family(
        doc_info.producer.as_deref(),
        doc_info.creator.as_deref(),
    );

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

    let document = timings.stage("document", || {
        document_stage(
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
        )
    })?;

    let sources = decode_images(pdf, &images);

    // `epub`, `validate` and `repair` are one call, because the loop owns the emission: each of its
    // iterations is one regeneration plus one validation pass, and a caller that emitted once for
    // the conservation check and again for the loop would encode every image in the book twice.
    let loop_result = timings.stage("epub+validate+repair", || {
        validate_repair_stage(
            &document.document,
            &sources,
            &options.epub,
            oc_validate::Expectations {
                images: Some(extracted_images),
            },
            pdf.page_count(),
            &mut totals,
            t,
        )
    })?;

    let epub = epub_check(&loop_result.document, loop_result.built, &mut totals)?;

    // The ledger the document was assembled with, plus every check made after it. `document`'s own
    // check is one of them: it was computed by `document_stage` and never recorded, so the ledger
    // named seven stages where the pipeline had checked eight. The check itself always ran — a
    // violation returns `DocumentError` — but the record is the evidence, and a stage missing from
    // it is a stage nobody can show was checked.
    let mut settled = loop_result.document;
    settled.ledger = document.document.ledger.clone();
    settled
        .ledger
        .push_stage(&document.delta, document.check.clone());
    settled.ledger.push_stage(&epub.delta, epub.check.clone());
    for check in loop_result.checks {
        settled.ledger.push_stage(&LedgerDelta::default(), check);
    }
    // What the AI step decided and warned about: every escalated choice it settled, refused or
    // never asked, and the reasons — recorded on the document, which is what the report reads.
    if let Some(outcome) = &ai {
        settled.decisions.extend(outcome.decisions.iter().cloned());
        settled.warnings.extend(outcome.warnings.iter().cloned());
    }
    settled
        .warnings
        .extend(loop_result.structural.warnings.iter().cloned());
    settled
        .warnings
        .extend(loop_result.outcome.warnings.clone());
    settled.warnings.extend(
        epub.built
            .warnings
            .iter()
            .map(|code| Warning::new(code, Severity::Warn)),
    );

    Ok(Conversion {
        document: settled,
        built: epub.built,
        extracted_images,
        tier1: loop_result.tier1,
        structural: loop_result.structural,
        repair: loop_result.outcome,
        timings,
        producer_family,
        page_classes: class_histogram(&classes),
        escalations: escalations.records(),
        ai,
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

/// How many pages fall into each class, by the name the report prints (D13.10).
fn class_histogram(classes: &[PageClass]) -> BTreeMap<String, u32> {
    let mut histogram: BTreeMap<String, u32> = BTreeMap::new();
    for class in classes {
        let name = serde_json::to_value(class)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "unknown".to_owned());
        *histogram.entry(name).or_default() += 1;
    }
    histogram
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
pub fn image_hashes(
    pdf: &dyn PdfDoc,
    images: &[oc_model::extract::ImageRef],
    t: &Thresholds,
) -> Vec<Option<u64>> {
    // Only the images the ornament rule will compare. A full-page scan cannot be an ornament,
    // and decoding one at full resolution to produce a hash nobody reads was most of the time
    // an image-only book spent in conversion.
    images
        .iter()
        .map(|image| {
            oc_structure::images::needs_hash(image, t)
                .then(|| oc_pdf::images::perceptual_hash(&decode_one(pdf, images, image)))
        })
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
