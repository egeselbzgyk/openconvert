# OpenConvert IR sketch (shared reference for ARCHITECTURE.md, PIPELINE.md, IMPLEMENTATION_PLAN.md)

This is the authoritative sketch of the intermediate representation (`oc-model`). Documents may elaborate but must not contradict it. Rust-like pseudocode; all types are `serde`-serializable to canonical JSON. `ir_version = 1`.

```rust
// ---------- identity & geometry ----------
pub struct BlockId(pub [u8; 10]);           // base32(blake3(page_index ‖ bbox@1pt ‖ first 64 NFC chars))[..10] + collision suffix
pub struct RunId(pub u32);                  // per-document run index (stable within one extraction)
pub struct Rect { pub x0: f32, pub y0: f32, pub x1: f32, pub y1: f32 } // normalized page space: origin top-left, y down, points, after /Rotate and CropBox offset
pub struct PageRef { pub index: u32, pub label: Option<String> }        // label = printed page number if detected ("iv", "12")

// ---------- extraction layer (Stage 1 output) ----------
pub struct PdfDocInfo { pages: u32, producer: Option<String>, creator: Option<String>, encrypted: bool,
                        outline: Vec<OutlineEntry>, xmp: Option<XmpMeta>, info_dict: InfoDict, has_struct_tree: bool,
                        producer_family: ProducerFamily /* PdfTeX|InDesign|Word|Ghostscript|Scanner|Typst|WeasyPrint|Chromium|Unknown */ }
pub struct PageInfo { page: PageRef, media_box: Rect, crop_box: Rect, rotate: u16, class: PageClass, class_conf: f32,
                      image_area_ratio: f32, visible_chars: u32, invisible_chars: u32 }
pub enum PageClass { Text, BrokenText, OcrSandwich, ImageOnly, Mixed, Blank }
pub struct Glyph { ch: char, bbox: Rect, loose_bbox: Rect, origin: (f32,f32), font: FontId, size_pt: f32, weight: u16,
                   italic: bool, render_mode: u8, fill: [u8;4], generated: bool, hyphen_flag: bool, angle_deg: f32 }
pub struct FontInfo { id: FontId, name: String, family_key: String, serif: bool, fixed_pitch: bool, symbolic: bool, type3: bool, embedded: bool }
pub struct ImageRef { id: ImageId, page: PageRef, bbox: Rect, intrinsic_px: (u32,u32), has_smask: bool, is_inline: bool,
                      colorspace: String, effective_dpi: f32, kind: ImageKind /* Figure|FullPageBackground|Ornament|Strip|Unknown */ }
pub struct VectorRegion { id: VecId, page: PageRef, bbox: Rect, path_count: u32, is_rule: bool /* thin horizontal/vertical line */ }

// ---------- text layer (Stage 2/3 output) ----------
pub struct Run { id: RunId, page: PageRef, text: String /* NFC, ligatures expanded, no U+00AD */, bbox: Rect, baseline_y: f32,
                 font: FontId, size_pt: f32, weight: u16, italic: bool, superscript: bool, subscript: bool,
                 provenance: TextProvenance /* Pdf | OcrLayer | Ocr */, glyph_range: (u32,u32) }
pub struct Line { runs: Vec<RunId>, bbox: Rect, baseline_y: f32, ends_with_hyphen: bool, indent_pt: f32, right_gap_pt: f32 }
pub struct Block { id: BlockId, page: PageRef, bbox: Rect, lines: Vec<Line>, column: u8, kind_hint: BlockKindHint,
                   furniture: Option<FurnitureKind /* RunningHeader|RunningFooter|PageNumber */>, reading_index: u32 }

// ---------- semantic layer (Stage 4 output = the "document tree") ----------
pub struct Document { ir_version: u32, source_sha256: String, meta: Metadata, language: LangTag, sections: Vec<Section>,
                      notes: Vec<Note>, figures: Vec<Figure>, tables: Vec<Table>, page_breaks: Vec<PageBreak>,
                      ledger: Ledger, decisions: Vec<Decision>, warnings: Vec<Warning>, classification: DocClass, presets: PresetName }
pub struct Metadata { title: Option<String>, subtitle: Option<String>, authors: Vec<String>, translator: Option<String>,
                      publisher: Option<String>, date: Option<String>, identifier: String /* urn:uuid, stable per source_sha256 */,
                      language: LangTag, source: MetaSource /* Xmp|InfoDict|Llm|Heuristic|User */ }
pub struct Section { id: BlockId, role: SectionRole /* FrontMatter(kind) | Part | Chapter | Section | BackMatter(kind) */,
                     level: u8 /* 1..6 */, heading: Option<Heading>, content: Vec<Content>, children: Vec<Section>,
                     source_pages: (u32,u32), confidence: Confidence }
pub enum Content { Paragraph(Para), Heading(Heading), List(List), BlockQuote(Vec<Content>), Verse(Verse), Preformatted(Pre),
                   Figure(FigureId), Table(TableId), NoteRefAnchor(NoteId), PageBreak(PageBreakId), Rule, Epigraph(Vec<Content>) }
pub struct Para { id: BlockId, spans: Vec<Span>, first_line_indent: bool, drop_cap: bool, align: Align, lang: Option<LangTag>, confidence: Confidence }
pub struct Span { text: String, style: SpanStyle /* bold, italic, smallcaps, superscript, subscript, monospace */, noteref: Option<NoteId>, link: Option<LinkTarget> }
pub struct Heading { id: BlockId, level: u8, spans: Vec<Span>, numbering: Option<String>, style_cluster: ClusterId, confidence: Confidence }
pub struct List { id: BlockId, ordered: bool, start: Option<u32>, items: Vec<ListItem /* content + nested List */>, confidence: Confidence }
pub struct Verse { id: BlockId, stanzas: Vec<Vec<Vec<Span>>>, confidence: Confidence }
pub struct Note { id: NoteId, kind: NoteKind /* Footnote|Endnote */, marker: String, body: Vec<Content>, anchor: Option<BlockId>, page: PageRef, confidence: Confidence }
pub struct Figure { id: FigureId, image: ImageId, caption: Option<Vec<Span>>, alt: String, anchor: BlockId /* block before which it is placed */, confidence: Confidence }
pub struct Table { id: TableId, rows: Vec<Vec<Cell>>, header_rows: u8, caption: Option<Vec<Span>>, fallback_image: Option<ImageId>, confidence: Confidence }
pub struct PageBreak { id: PageBreakId, page: PageRef, before_block: BlockId }   // → epub:type="pagebreak" + page-list nav

// ---------- confidence, ledger, decisions ----------
pub struct Confidence { method: Method /* Deterministic|Llm|User */, score: Option<f32> /* None = predicate-based, v1 */,
                        signals: Vec<(SignalName, f32)>, escalated: bool, fallback_used: bool }
pub enum StageKind { Conserving, Budgeted }
pub enum Reason { SoftHyphen, LigatureExpand, GeneratedSpace, RunningHeader, RunningFooter, PageNumber, OverdrawDedup,
                  OcrLayerDuplicate, Dehyphenate, Ocr, DecorativeGlyph, Watermark, ClippedOffPage, HiddenText, UserOverride }
// 15 variants (closed). ClippedOffPage = geometrically absent (outside CropBox, or removed by a clipping path).
// HiddenText     = rendered but not visible: render mode 3 on a page that is NOT an OcrSandwich, or a fill
//                  colour within the delta-E tolerance of the local background. Both are owned by `ingest`.
// Ocr            = OCR-inserted text; region-scoped by I-6 (see below).
pub struct LedgerEntry { stage: StageName, reason: Reason, block: Option<BlockId>, page: PageRef, span: (u32,u32), text: String, added: bool }
pub struct Ledger { entries: Vec<LedgerEntry>, c_raw: CharHistogram, c_0: CharHistogram, per_stage_checks: Vec<StageCheck /* I-1..I-6 results */> }
pub struct Decision { stage: StageName, subject: BlockId, kind: DecisionKind, chosen: String, alternatives: Vec<String>,
                      method: Method, llm: Option<LlmTrace { model_id, prompt_version, input_sha256, output_sha256, cached: bool, ms: u32 }> }
pub struct Warning { code: WarningCode /* stable enum, localized via templates */, severity: Severity /* Info|Warn|Error */, args: Map, blocks: Vec<BlockId>, page: Option<PageRef> }

// ---------- overrides (user corrections; v1 supports metadata + TOC; block-level reserved) ----------
pub struct Overrides { ir_version: u32, source_sha256: String, metadata: Option<MetadataPatch>, toc: Option<Vec<TocPatch>>,
                       blocks: Vec<BlockOverride { id: BlockId, role: Option<...>, level: Option<u8>, merge_with_next: Option<bool> }> }
```

Canonical JSON rules: keys sorted; `f32` geometry printed with 2 decimals *at serialization only* (in-memory full precision); no NaN/Inf (serialization error); strings NFC; arrays in document order; `ir_version` first key. Structural digest (for corpus snapshots): counts per `Content` variant, heading tree shape `(level, text[..40])` list, ledger totals per `Reason`, per-page class histogram, first/last 200 chars per section.

Stage names (also the `--dump-stage` names): `inspect`, `ingest`, `text`, `furniture`, `layout`, `paragraphs`, `structure`, `document`, `epub`, `validate`, `repair`, `report`.

Stage kinds: `ingest` Budgeted{GeneratedSpace, ClippedOffPage, HiddenText, OverdrawDedup, OcrLayerDuplicate, Ocr} — `Ocr` is owned by `ingest`, which runs OCR after page classification whenever an engine is available; `text` Budgeted{SoftHyphen, LigatureExpand}; `furniture` Budgeted{RunningHeader, RunningFooter, PageNumber, Watermark, DecorativeGlyph}; `layout` Conserving; `paragraphs` Budgeted{Dehyphenate}; `structure` Conserving; `document` Conserving; `epub` Conserving; `repair` Conserving (repairs are structural) except `UserOverride`.

Invariant I-6 is **region-scoped**: an `Ocr` ledger entry is Added-only and permitted on any page region whose bbox contains no PDF text runs — the whole page on an `image-only` page, each uncovered image region on a `mixed` page. Every such region carries `provenance = Ocr` and is excluded from the source-retention denominator.
