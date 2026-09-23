//! An in-process OCR engine for the pipeline tests (PHASE 13): it reads nothing, returns a scripted
//! page of words laid out inside whatever region it is asked about, and records every request.
//!
//! In-process rather than a fake `tesseract` binary, so the routing, merging and ledger tests run
//! on every platform. The process itself — argv, deadline, teardown — is `oc-testkit`'s
//! `ocr_invoke` tests and, here, the Unix-only `hung_tesseract_is_killed_at_deadline`.

use std::collections::BTreeSet;
use std::sync::Mutex;

use oc_core::ocr::invoke::{OcrEngine, OcrError, OcrRequest};
use oc_core::ocr::{OcrWord, Psm};
use oc_model::geom::Rect;

/// What one request asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct Asked {
    pub page: u32,
    pub psm: Psm,
    pub region: Rect,
    pub langs: String,
    pub dpi: u32,
    pub raster: (u32, u32),
}

/// A scripted engine.
pub struct ScriptedEngine {
    /// The lines every region "contains", as text.
    pub lines: Vec<String>,
    /// Every word's confidence.
    pub conf: f32,
    /// When set, every call fails with this error instead.
    pub fail: Option<OcrError>,
    langs: BTreeSet<String>,
    asked: Mutex<Vec<Asked>>,
}

impl ScriptedEngine {
    pub fn new(lines: &[&str]) -> Self {
        Self {
            lines: lines.iter().map(|line| (*line).to_owned()).collect(),
            conf: 0.93,
            fail: None,
            langs: ["deu", "eng", "osd", "tur"].map(str::to_owned).into(),
            asked: Mutex::new(Vec::new()),
        }
    }

    pub fn with_conf(mut self, conf: f32) -> Self {
        self.conf = conf;
        self
    }

    pub fn failing(mut self, error: OcrError) -> Self {
        self.fail = Some(error);
        self
    }

    pub fn asked(&self) -> Vec<Asked> {
        self.asked.lock().expect("the log").clone()
    }

    /// Every word the engine returns, joined as the merge joins them.
    pub fn text(&self) -> String {
        self.lines.join(" ")
    }
}

impl OcrEngine for ScriptedEngine {
    fn capability(&self) -> String {
        "ocr:scripted-1.0.0".to_owned()
    }

    fn langs(&self) -> &BTreeSet<String> {
        &self.langs
    }

    fn recognize(&self, request: &OcrRequest) -> Result<Vec<OcrWord>, OcrError> {
        self.asked.lock().expect("the log").push(Asked {
            page: request.page_index,
            psm: request.psm,
            region: request.region_pt,
            langs: request.langs.arg(),
            dpi: request.dpi,
            raster: (request.raster.width(), request.raster.height()),
        });
        if let Some(error) = &self.fail {
            return Err(error.clone());
        }
        // Lines 14 pt apart from the region's top-left, words 5 pt a character, scaled down if the
        // region is small, so every word is inside the region it was read from.
        let region = request.region_pt;
        let width = region.x1 - region.x0;
        let height = region.y1 - region.y0;
        let longest = self
            .lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(1) as f32;
        let scale = (width / (longest * 5.0 + 20.0))
            .min(height / (self.lines.len() as f32 * 14.0 + 20.0))
            .min(1.0);
        let mut words = Vec::new();
        for (index, line) in self.lines.iter().enumerate() {
            let y0 = region.y0 + (10.0 + index as f32 * 14.0) * scale;
            let mut x = region.x0 + 10.0 * scale;
            for word in line.split_whitespace() {
                let w = word.chars().count() as f32 * 5.0 * scale;
                words.push(OcrWord {
                    text: word.to_owned(),
                    bbox: Rect {
                        x0: x,
                        y0,
                        x1: x + w,
                        y1: y0 + 10.0 * scale,
                    },
                    conf: self.conf,
                    block: 1,
                    par: 1,
                    line: u32::try_from(index + 1).unwrap_or(u32::MAX),
                });
                x += w + 5.0 * scale;
            }
        }
        Ok(words)
    }
}
