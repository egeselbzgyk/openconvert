//! Driving a fixture the whole way to a [`Document`], the way a conversion does.
//!
//! Phase 5's tests are about what comes *out* of the pipeline, so nearly all of them need the
//! same nine stages run over a fixture first. Doing that once here keeps each test to its own
//! subject, and — more usefully — means every test is looking at a document assembled by the
//! same code path the CLI uses, rather than at a hand-built one that cannot go wrong in the
//! ways a real book does.

#![allow(dead_code)]

use std::collections::BTreeMap;

use oc_core::ledger_check::ReasonTotals;
use oc_core::thresholds::T;
use oc_model::document::{Document, PresetName};
use oc_model::ids::BlockId;
use oc_model::lang::LangTag;
use oc_model::ledger::Ledger;
use oc_pdf::classify::{classify_page, PageClass};
use oc_pdf::inspect::PdfOpen;
use oc_pdf::pdfium::PdfiumBackend;
use oc_structure::meta::{InfoDict, MetaSources};
use oc_structure::stage::StructureInput;
use openconvert::document::DocumentInput;
use openconvert::pipeline::{
    body_runs, document_stage, furniture_stage, layout_stage, structure_stage, text_stage,
    LayoutStage, StructureStage, TextStage,
};
use openconvert::structure_input::{block_views, document_images};

/// Every stage's output for one fixture, kept so a test can look behind the document.
pub struct Converted {
    pub document: Document,
    pub text: TextStage,
    pub layout: LayoutStage,
    pub structure: StructureStage,
}

/// The path of a built Typst fixture, by its stem.
pub fn fixture(stem: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/fixtures")
        .join(format!("{stem}.pdf"))
}

/// Run every stage over a fixture and assemble the document.
pub fn convert(stem: &str) -> Converted {
    convert_as(stem, LangTag::EN)
}

/// The same, for a fixture that is not in English.
pub fn convert_as(stem: &str, lang: LangTag) -> Converted {
    let path = fixture(stem);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing fixture {}: {error}; run `cargo run -p xtask -- fixtures`",
            path.display()
        )
    });
    let sha256 = sha256_hex(&bytes);

    let backend = PdfiumBackend::bind().expect("PDFium is vendored");
    let pdf = backend.open(&bytes, None).expect("the fixture opens");
    let input = openconvert::input::page_inputs(pdf.as_ref()).expect("every page extracts");

    let mut totals = ReasonTotals::default();
    let text = text_stage(&input, &mut totals, &T).expect("text conserves");
    let furniture =
        furniture_stage(&text, lang.clone(), &mut totals, &T).expect("furniture stays in budget");
    let layout = layout_stage(&text, &furniture, &mut totals, &T).expect("layout conserves");

    let images = document_images(&text);
    let hashes = images
        .iter()
        .map(|image| {
            let local = images
                .iter()
                .filter(|other| other.page.index == image.page.index)
                .position(|other| other.id == image.id)
                .unwrap_or_default();
            pdf.image_bytes(
                image.page.index,
                oc_model::extract::ImageId(u32::try_from(local).unwrap_or_default()),
            )
            .map(|decoded| oc_pdf::images::perceptual_hash(&decoded))
            .unwrap_or_default()
        })
        .collect();
    let vectors = (0..pdf.page_count())
        .filter_map(|page| pdf.page_vectors(page).ok())
        .flatten()
        .collect();
    let doc_info = pdf.doc_info();

    let structure_input = StructureInput {
        blocks: block_views(&text, &layout),
        runs: body_runs(&text, &furniture),
        fonts: text.fonts.clone(),
        images,
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
            filename: format!("{stem}.pdf"),
            source_sha256: sha256.clone(),
            language: lang.clone(),
        },
        lang: lang.clone(),
    };
    let structure =
        structure_stage(&layout, &structure_input, &mut totals, &T).expect("structure conserves");

    let classes: Vec<PageClass> = (0..pdf.page_count())
        .map(|page| {
            let chars = pdf.page_char_stats(page).unwrap_or_default();
            let images = pdf.page_image_stats(page).unwrap_or_default();
            classify_page(&chars, &images, None, &T).0
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
            source_sha256: &sha256,
            structure: &structure.output,
            labels: &furniture.labels,
            classes: &classes,
            landscape: &landscape,
            column_counts: &column_counts,
            block_pages: &block_pages,
            language: lang,
            preset: PresetName::Auto,
            ledger,
        },
        &mut totals,
        &T,
    )
    .expect("document conserves and closes");

    Converted {
        document: document.document,
        text,
        layout,
        structure,
    }
}

/// The lowercase hex SHA-256 of the input bytes, as `dc:identifier` is minted from.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
