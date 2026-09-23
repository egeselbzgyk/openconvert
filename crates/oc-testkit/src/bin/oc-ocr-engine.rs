//! A minimal engine for the OCR teardown test (PHASE 13 row 13.19).
//!
//! It starts one OCR call through `oc_core::ocr::invoke`, exactly as the pipeline does, against the
//! `tesseract` named on its command line — a fake that sleeps — and then ends the way it is told:
//!
//! - `wait`: blocks on the call until a signal arrives — only the signal handler can tear the child
//!   down;
//! - `panic`: waits for a line on stdin (sent once the test has seen the child running) and panics
//!   — only the panic hook can tear the child down.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use image::GrayImage;
use oc_core::ocr::discover::{DiscoverySource, TesseractInfo, Version};
use oc_core::ocr::invoke::{OcrEngine, OcrRequest, Tesseract};
use oc_core::ocr::lang::LangSpec;
use oc_core::ocr::OcrScope;
use oc_model::geom::Rect;

fn main() {
    // As the real engine does: its children get the parent-death signal (PHASE 14).
    oc_core::sidecar::orphan::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mode, program, workdir) = (
        args[0].clone(),
        PathBuf::from(&args[1]),
        PathBuf::from(&args[2]),
    );
    let info = TesseractInfo {
        path: program,
        version: Version {
            major: 5,
            minor: 3,
            patch: 4,
        },
        langs: BTreeSet::from(["eng".to_owned()]),
        source: DiscoverySource::ConfigPath,
    };
    let engine = Tesseract::new(info, &workdir, Duration::from_secs(600));
    let request = OcrRequest {
        raster: GrayImage::new(8, 8),
        dpi: 300,
        psm: OcrScope::FullPage.psm(),
        langs: LangSpec::single("eng"),
        region_pt: Rect {
            x0: 0.0,
            y0: 0.0,
            x1: 1.92,
            y1: 1.92,
        },
        page_index: 0,
    };

    let worker = std::thread::spawn(move || engine.recognize(&request));
    match mode.as_str() {
        "wait" => {
            let _ = worker.join();
        }
        "panic" => {
            let mut go = String::new();
            std::io::stdin().read_line(&mut go).expect("the go line");
            panic!("forced panic with a live tesseract");
        }
        other => panic!("unknown mode {other}"),
    }
    oc_core::sidecar::supervise::settle();
}
