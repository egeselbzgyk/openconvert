//! What `structure` settled, saved for "Fix and rebuild" (ratified R-15, A12.4b).
//!
//! A metadata or TOC correction cannot change a glyph, so the run that applies one has no reason
//! to read the PDF's text again. A full run with a cache directory saves its [`Upstream`] — the
//! structured book plus the few per-page facts `document` reads, and the ledger through
//! `structure` — as one JSON file named by the book's digest; a run with the user's corrections
//! that finds a save made by this engine for these bytes resumes at `document` from it.
//!
//! **A cache miss is never an error.** A save that is missing, damaged, from another engine or IR
//! version, or for another forced language reads as nothing, and the run converts from page one:
//! the book is the same book either way, only slower.
//!
//! **What is on disk is the book's text.** The directory is the app's own (its data directory's
//! `cache/`), cleared with the rest of the cache, and nothing is written unless the caller names
//! it: the CLI writes nothing unless `OC_CACHE_DIR` is set.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use oc_core::stages::{self, StageDecl};
use oc_model::doc::{Figure, Metadata, Note, Section, Severity, Table, Warning};
use oc_model::extract::{CharHistogram, ImageId, ImageRef, PageRef};
use oc_model::geom::Rect;
use oc_model::ids::BlockId;
use oc_model::lang::LangTag;
use oc_model::ledger::{Ledger, LedgerEntry, Reason, StageCheck, StageKind};
use oc_pdf::classify::PageClass;
use oc_pdf::producer::ProducerFamily;
use oc_structure::escalate::EscalationRecord;
use serde::{Deserialize, Serialize};

use crate::convert::{ConvertOptions, Upstream};
use crate::document::Structured;

/// What a save must match to be resumed from: the bytes, the engine that read them, the IR its
/// ids are in, and the options the stages up to `structure` read — the forced language and OCR's
/// settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheKey {
    pub engine_version: String,
    pub ir_version: u32,
    pub source_sha256: String,
    /// The forced `dc:language`, which `furniture` reads; `None` when none was forced.
    pub language: Option<String>,
    /// `ingest`'s OCR settings (PHASE 13): the mode, the re-OCR policy, the explicit languages and
    /// the engine found — `never|never||` for a run that reads no pixels.
    pub ocr: String,
}

impl CacheKey {
    pub fn for_run(source_sha256: &str, options: &ConvertOptions) -> Self {
        Self {
            engine_version: env!("CARGO_PKG_VERSION").to_owned(),
            ir_version: oc_model::IR_VERSION,
            source_sha256: source_sha256.to_owned(),
            language: options.language.as_ref().map(|tag| tag.as_str().to_owned()),
            ocr: format!(
                "{}|{}|{}|{}",
                options.ocr.mode.as_str(),
                options.ocr.re_ocr.as_str(),
                options
                    .ocr
                    .langs
                    .as_ref()
                    .map(oc_core::ocr::lang::LangSpec::arg)
                    .unwrap_or_default(),
                options
                    .ocr
                    .engine
                    .as_ref()
                    .map(|engine| engine.capability())
                    .unwrap_or_default(),
            ),
        }
    }
}

/// The saves, one file per book under `<root>/structure/`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureCache {
    dir: PathBuf,
}

impl StructureCache {
    pub fn new(root: &Path) -> Self {
        Self {
            dir: root.join("structure"),
        }
    }

    /// Where the save for `key` lives. `None` for a digest that is not lowercase hex, which is
    /// the only thing that ever becomes part of the name.
    pub fn path(&self, key: &CacheKey) -> Option<PathBuf> {
        let digest = &key.source_sha256;
        let hex = !digest.is_empty()
            && digest
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
        hex.then(|| self.dir.join(format!("{digest}.json")))
    }

    /// The save for `key`, if there is one this run can resume from.
    pub fn read(&self, key: &CacheKey) -> Option<Upstream> {
        let text = std::fs::read_to_string(self.path(key)?).ok()?;
        let saved: Saved = serde_json::from_str(&text).ok()?;
        if saved.key != *key {
            return None;
        }
        saved.upstream.restore()
    }

    /// Save `upstream` for `key`, replacing any earlier save of the same book whole: written beside
    /// and renamed over, so a reader never sees half a file.
    pub fn write(&self, key: &CacheKey, upstream: &Upstream) -> std::io::Result<()> {
        let path = self
            .path(key)
            .ok_or_else(|| std::io::Error::other("the digest is not hex"))?;
        std::fs::create_dir_all(&self.dir)?;
        let text = serde_json::to_vec(&Saved {
            key: key.clone(),
            upstream: SavedUpstream::of(upstream),
        })
        .map_err(std::io::Error::other)?;
        let partial = path.with_extension(format!("json.{}.partial", std::process::id()));
        std::fs::write(&partial, text)?;
        std::fs::rename(&partial, &path).inspect_err(|_| {
            let _ = std::fs::remove_file(&partial);
        })
    }
}

#[derive(Serialize, Deserialize)]
struct Saved {
    key: CacheKey,
    upstream: SavedUpstream,
}

/// [`Upstream`] as it is written. The IR types are written as they are; the three that carry a
/// `&'static str` — a warning's code, a ledger entry's and a check's stage — are written as text
/// and read back against the registries they came from, so a save naming a code or a stage this
/// engine does not have is not resumed from.
#[derive(Serialize, Deserialize)]
struct SavedUpstream {
    sections: Vec<Section>,
    notes: Vec<Note>,
    figures: Vec<Figure>,
    tables: Vec<Table>,
    metadata: Metadata,
    warnings: Vec<SavedWarning>,
    labels: Vec<Option<String>>,
    classes: Vec<PageClass>,
    landscape: Vec<bool>,
    column_counts: Vec<usize>,
    block_pages: BTreeMap<BlockId, u32>,
    images: Vec<ImageRef>,
    slots: Vec<ImageId>,
    extracted_images: u32,
    #[serde(default)]
    ornaments_dropped: u32,
    language: LangTag,
    producer_family: ProducerFamily,
    escalations: Vec<EscalationRecord>,
    ledger: SavedLedger,
}

#[derive(Serialize, Deserialize)]
struct SavedWarning {
    code: String,
    severity: Severity,
    args: BTreeMap<String, String>,
    blocks: Vec<BlockId>,
    page: Option<PageRef>,
}

#[derive(Serialize, Deserialize)]
struct SavedLedger {
    entries: Vec<SavedEntry>,
    c_raw: CharHistogram,
    c_0: CharHistogram,
    per_stage_checks: Vec<SavedCheck>,
}

#[derive(Serialize, Deserialize)]
struct SavedEntry {
    stage: String,
    reason: Reason,
    page: u32,
    span: (u32, u32),
    text: String,
    added: bool,
    /// OCR's region (I-6); absent for every other reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    region: Option<Rect>,
}

#[derive(Serialize, Deserialize)]
struct SavedCheck {
    stage: String,
    kind: StageKind,
    removed_chars: u64,
    added_chars: u64,
    retention: f32,
}

/// Every stage that can appear in a ledger, by the name it is recorded under.
const STAGES: [StageDecl; 10] = [
    stages::INGEST,
    stages::TEXT,
    stages::FURNITURE,
    stages::LAYOUT,
    stages::PARAGRAPHS,
    stages::STRUCTURE,
    stages::DOCUMENT,
    stages::EPUB,
    stages::VALIDATE,
    stages::REPAIR,
];

fn stage_name(name: &str) -> Option<&'static str> {
    STAGES
        .iter()
        .find(|decl| decl.name == name)
        .map(|decl| decl.name)
}

impl SavedUpstream {
    fn of(upstream: &Upstream) -> Self {
        let structured = &upstream.structured;
        Self {
            sections: structured.sections.clone(),
            notes: structured.notes.clone(),
            figures: structured.figures.clone(),
            tables: structured.tables.clone(),
            metadata: structured.metadata.clone(),
            warnings: structured
                .warnings
                .iter()
                .map(|warning| SavedWarning {
                    code: warning.code.to_owned(),
                    severity: warning.severity,
                    args: warning.args.clone(),
                    blocks: warning.blocks.clone(),
                    page: warning.page.clone(),
                })
                .collect(),
            labels: upstream.labels.clone(),
            classes: upstream.classes.clone(),
            landscape: upstream.landscape.clone(),
            column_counts: upstream.column_counts.clone(),
            block_pages: upstream.block_pages.clone(),
            images: upstream.images.clone(),
            slots: upstream.slots.clone(),
            extracted_images: upstream.extracted_images,
            ornaments_dropped: upstream.ornaments_dropped,
            language: upstream.language.clone(),
            producer_family: upstream.producer_family,
            escalations: upstream.escalations.clone(),
            ledger: SavedLedger {
                entries: upstream
                    .ledger
                    .entries
                    .iter()
                    .map(|entry| SavedEntry {
                        stage: entry.stage.to_owned(),
                        reason: entry.reason,
                        page: entry.page,
                        span: entry.span,
                        text: entry.text.clone(),
                        added: entry.added,
                        region: entry.region,
                    })
                    .collect(),
                c_raw: upstream.ledger.c_raw.clone(),
                c_0: upstream.ledger.c_0.clone(),
                per_stage_checks: upstream
                    .ledger
                    .per_stage_checks
                    .iter()
                    .map(|check| SavedCheck {
                        stage: check.stage.to_owned(),
                        kind: check.kind,
                        removed_chars: check.removed_chars,
                        added_chars: check.added_chars,
                        retention: check.retention,
                    })
                    .collect(),
            },
        }
    }

    /// The [`Upstream`] this was written from, or `None` if it names a warning code or a stage
    /// this engine does not know.
    fn restore(self) -> Option<Upstream> {
        let warnings = self
            .warnings
            .into_iter()
            .map(|saved| {
                let code = oc_core::warnings::spec(&saved.code)?.code;
                Some(Warning {
                    code,
                    severity: saved.severity,
                    args: saved.args,
                    blocks: saved.blocks,
                    page: saved.page,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let entries = self
            .ledger
            .entries
            .into_iter()
            .map(|saved| {
                Some(LedgerEntry {
                    stage: stage_name(&saved.stage)?,
                    reason: saved.reason,
                    page: saved.page,
                    span: saved.span,
                    text: saved.text,
                    added: saved.added,
                    region: saved.region,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let per_stage_checks = self
            .ledger
            .per_stage_checks
            .into_iter()
            .map(|saved| {
                Some(StageCheck {
                    stage: stage_name(&saved.stage)?,
                    kind: saved.kind,
                    removed_chars: saved.removed_chars,
                    added_chars: saved.added_chars,
                    retention: saved.retention,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Upstream {
            structured: Structured {
                sections: self.sections,
                notes: self.notes,
                figures: self.figures,
                tables: self.tables,
                metadata: self.metadata,
                warnings,
            },
            labels: self.labels,
            classes: self.classes,
            landscape: self.landscape,
            column_counts: self.column_counts,
            block_pages: self.block_pages,
            images: self.images,
            slots: self.slots,
            extracted_images: self.extracted_images,
            ornaments_dropped: self.ornaments_dropped,
            language: self.language,
            producer_family: self.producer_family,
            escalations: self.escalations,
            ledger: Ledger {
                entries,
                c_raw: self.ledger.c_raw,
                c_0: self.ledger.c_0,
                per_stage_checks,
            },
        })
    }
}
