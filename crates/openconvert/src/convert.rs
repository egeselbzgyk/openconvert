//! One conversion, end to end: PDF bytes in, container bytes out.
//!
//! Every stage, in order, each checked under the conservation law as it finishes. This is the
//! one code path — the CLI drives it, the tests drive it, and the desktop app drives the CLI
//! (D13.1). A second path that tests took and users did not would be a second set of bugs.

use std::collections::BTreeMap;

use oc_core::cancel::Cancel;
use oc_core::ledger_check::{ConservationError, ReasonTotals};
use oc_core::progress::{Progress, StagePhase};
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
    /// The text of the user's `overrides.json`, when the job named one (ARCHITECTURE §4.7). Read
    /// against the book's digest inside the conversion; a file that does not apply is reported by
    /// name and the book is converted without it.
    pub overrides: Option<String>,
}

/// Per-stage wall-clock, in milliseconds, in stage order.
///
/// An `IndexMap` would do as well; a `Vec` of pairs is used because the report prints them in stage
/// order and a map would invite someone to sort them by name, which is not the order anybody wants
/// to read a pipeline in.
#[derive(Clone, Debug, Default)]
pub struct Timings(Vec<(&'static str, u64)>);

impl Timings {
    /// Run one stage as a user sees it: refuse to start it once cancelled, report its edges, and
    /// time it (D13.2's `stage{name, phase, elapsed_ms}`).
    fn observed<T, E>(
        &mut self,
        observe: Observe<'_>,
        name: &'static str,
        run: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, ConvertError>
    where
        ConvertError: From<E>,
    {
        observe.check()?;
        observe.progress.stage(name, StagePhase::Begin);
        let started = std::time::Instant::now();
        let out = run();
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.0.push((name, elapsed_ms));
        observe.progress.stage(name, StagePhase::End { elapsed_ms });
        Ok(out?)
    }

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
    /// Stopped on request. Not a failure: the answer the user asked for (§2.4, exit 3).
    #[error("the conversion was cancelled")]
    Cancelled,
}

/// Who is watching one conversion, and whether they have asked it to stop (D13.2).
///
/// A pair of references, because the two cross thread boundaries differently: progress is
/// reported *from* this thread, cancellation is set *into* it from another (ARCHITECTURE §8.3).
#[derive(Clone, Copy)]
pub struct Observe<'a> {
    pub progress: &'a dyn Progress,
    pub cancel: &'a Cancel,
}

impl Observe<'_> {
    /// Stop here if a cancel has been asked for. Called at every stage boundary and inside every
    /// per-page and per-image loop, which is what keeps `done{cancelled}` inside two seconds.
    fn check(&self) -> Result<(), ConvertError> {
        if self.cancel.is_cancelled() {
            return Err(ConvertError::Cancelled);
        }
        Ok(())
    }
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
    let silent = oc_core::progress::Silent;
    let never = Cancel::new();
    convert_observed(
        pdf,
        source_sha256,
        options,
        t,
        Observe {
            progress: &silent,
            cancel: &never,
        },
    )
}

/// [`convert`], reporting every stage's edges and every page to `observe.progress`, and
/// stopping with [`ConvertError::Cancelled`] once `observe.cancel` is set.
///
/// The stage names are the twelve of IR_SKETCH, emitted only for stages this driver runs: a
/// supervisor that shows "Reconstructing" is shown it because `layout` began, not because a timer
/// said it probably had (UI_UX §2.2).
pub fn convert_observed(
    pdf: &dyn PdfDoc,
    source_sha256: &str,
    options: &ConvertOptions,
    t: &Thresholds,
    observe: Observe<'_>,
) -> Result<Conversion, ConvertError> {
    let mut timings = Timings::default();
    let input = timings.observed(observe, "ingest", || read_pages(pdf, observe))?;
    let mut totals = ReasonTotals::default();

    let text = timings.observed(observe, "text", || text_stage(&input, &mut totals, t))?;
    // Language detection is Phase 2's and is not wired into the stage driver yet, so the
    // configured tag is the one that is used; `LangTag::UND` is what a book gets when nobody
    // said. Guessing English would tell a screen reader to pronounce a German book in English.
    let language = options.language.clone().unwrap_or(LangTag::UND);

    let furniture = timings.observed(observe, "furniture", || {
        furniture_stage(&text, language.clone(), &mut totals, t)
    })?;
    let layout = timings.observed(observe, "layout", || {
        layout_stage(&text, &furniture, &mut totals, t)
    })?;

    let images = document_images(&text);
    let extracted_images = u32::try_from(images.len()).unwrap_or(u32::MAX);
    // The ornament rule's hashes are `structure`'s evidence, so they are reported as its work.
    observe.check()?;
    observe.progress.stage("structure", StagePhase::Begin);
    let structure_started = std::time::Instant::now();
    let hashes = hash_images(pdf, &images, t, observe)?;
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
    let structure = timings.stage("structure", || {
        structure_stage(&layout, &structure_input, &mut totals, t)
    })?;
    observe.progress.stage(
        "structure",
        StagePhase::End {
            elapsed_ms: u64::try_from(structure_started.elapsed().as_millis()).unwrap_or(u64::MAX),
        },
    );

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

    let (overrides, refused) = crate::overrides::load(options.overrides.as_deref(), source_sha256);
    let document = timings.observed(observe, "document", || {
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
            overrides.as_ref(),
            &mut totals,
            t,
        )
    })?;

    // The images are decoded for `epub`, and reported as its work: the first thing a user sees of
    // "Building" on an illustrated book is this loop.
    observe.check()?;
    let sources = decode_images(pdf, &images, observe)?;

    // `epub`, `validate` and `repair` are one call, because the loop owns the emission: each of its
    // iterations is one regeneration plus one validation pass, and a caller that emitted once for
    // the conservation check and again for the loop would encode every image in the book twice.
    // The host reports each of the three as it runs them, so the stage events stay separate.
    observe.check()?;
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
            observe.progress,
        )
    })?;

    let epub = epub_check(&loop_result.document, loop_result.built, &mut totals)?;

    // The ledger the document was assembled with, plus every check made after it. `document`'s own
    // check is one of them: it was computed by `document_stage` and never recorded, so the ledger
    // named seven stages where the pipeline had checked eight. The check itself always ran — a
    // violation returns `DocumentError` — but the record is the evidence, and a stage missing from
    // it is a stage nobody can show was checked.
    //
    // `document`'s delta — the user's corrections, when there are any — is already in its ledger
    // (`document_stage` put it there for the loop's I-7), so only its check is added here.
    let mut settled = loop_result.document;
    settled.ledger = document.document.ledger.clone();
    settled.ledger.per_stage_checks.push(document.check.clone());
    settled.ledger.push_stage(&epub.delta, epub.check.clone());
    for check in loop_result.checks {
        settled.ledger.push_stage(&LedgerDelta::default(), check);
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
    // Corrections the job named but this engine would not apply, by name (ARCHITECTURE §4.6).
    settled.warnings.extend(refused);

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

/// Every page, read a page at a time with a progress report and a cancel check between pages.
fn read_pages(
    pdf: &dyn PdfDoc,
    observe: Observe<'_>,
) -> Result<Vec<crate::pipeline::PageInput>, ConvertError> {
    let total = pdf.page_count();
    let mut pages = Vec::with_capacity(usize::try_from(total).unwrap_or_default());
    for index in 0..total {
        observe.check()?;
        pages.push(crate::input::page_input(pdf, index)?);
        observe.progress.advance("ingest", index, total);
    }
    Ok(pages)
}

/// [`image_hashes`], one image at a time, with a cancel check between images.
fn hash_images(
    pdf: &dyn PdfDoc,
    images: &[oc_model::extract::ImageRef],
    t: &Thresholds,
    observe: Observe<'_>,
) -> Result<Vec<Option<u64>>, ConvertError> {
    let mut hashes = Vec::with_capacity(images.len());
    for image in images {
        observe.check()?;
        hashes.push(
            oc_structure::images::needs_hash(image, t)
                .then(|| oc_pdf::images::perceptual_hash(&decode_one(pdf, images, image))),
        );
    }
    Ok(hashes)
}

/// Every image, decoded to RGBA for the encoder, with a cancel check between images.
fn decode_images(
    pdf: &dyn PdfDoc,
    images: &[oc_model::extract::ImageRef],
    observe: Observe<'_>,
) -> Result<Vec<SourceImage>, ConvertError> {
    let mut sources = Vec::with_capacity(images.len());
    for image in images {
        observe.check()?;
        let decoded = decode_one(pdf, images, image);
        sources.push(SourceImage {
            id: image.id,
            width: decoded.width,
            height: decoded.height,
            rgba: decoded.rgba,
        });
    }
    Ok(sources)
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
