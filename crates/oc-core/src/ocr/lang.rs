//! Which language data a page is read with (PHASE 13 detail 5).
//!
//! **One language by default.** Tesseract reads a stacked `deu+eng` more slowly and, on text that
//! is really one language, less accurately than `deu` alone, so a second language is stacked only
//! when per-block detection says it holds at least `ocr.second_lang_block_share` of the blocks.
//!
//! **Never a silent substitution.** When the traineddata for the chosen language is not installed
//! the page is read with `eng` — the one pack every Tesseract install carries — and
//! `W_OCR_LANG_MISSING` names the pack that was missing and how to install it. A German book read
//! with English data comes out recognisably worse, and a reader who is not told why will blame the
//! book.

use std::collections::BTreeSet;

use oc_model::doc::{Severity, Warning};
use oc_model::lang::LangTag;

pub use super::Os;

/// Traineddata named for a language this pipeline does not map, or for no verdict at all.
pub const FALLBACK: &str = "eng";

/// The selected traineddata is not installed, and the page was read with `eng` instead.
pub const W_OCR_LANG_MISSING: &str = "W_OCR_LANG_MISSING";

/// The separator Tesseract's `-l` takes between stacked languages.
const STACK: char = '+';

/// A `+`-joined list of traineddata names, as Tesseract's `-l` takes it. Never empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LangSpec(Vec<String>);

impl LangSpec {
    /// One language.
    pub fn single(code: &str) -> Self {
        Self(vec![code.to_owned()])
    }

    /// Parse `deu`, `deu+eng`. Every part must be a plain traineddata name — ASCII letters, digits
    /// and `_` — because the spec reaches a command line, and a name that could be a path or carry
    /// a shell metacharacter is refused rather than escaped.
    pub fn parse(spec: &str) -> Option<Self> {
        let parts: Vec<String> = spec.split(STACK).map(str::to_owned).collect();
        let plain = |part: &String| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        };
        parts.iter().all(plain).then_some(Self(parts))
    }

    /// The traineddata names, primary first.
    pub fn codes(&self) -> &[String] {
        &self.0
    }

    /// The value of Tesseract's `-l`.
    pub fn arg(&self) -> String {
        self.0.join(&STACK.to_string())
    }
}

/// The traineddata a language tag maps to: `de → deu`, `tr → tur`, `en → eng`, anything else
/// `eng` (detail 5). Keyed on the primary subtag, so `de-AT` is German.
pub fn tesseract_code(tag: &LangTag) -> &'static str {
    match tag.primary() {
        "de" => "deu",
        "tr" => "tur",
        _ => FALLBACK,
    }
}

/// The language data for a document whose language is `doc_lang` (`None` when nothing said).
pub fn select_langs(
    doc_lang: Option<&LangTag>,
    installed: &BTreeSet<String>,
) -> (LangSpec, Vec<Warning>) {
    let wanted = doc_lang.map_or(FALLBACK, tesseract_code);
    select_explicit(&LangSpec::single(wanted), installed)
}

/// An explicit spec (`--ocr-lang`), reduced to what is installed. A spec none of whose languages is
/// installed becomes `eng`; every missing language is one warning.
pub fn select_explicit(spec: &LangSpec, installed: &BTreeSet<String>) -> (LangSpec, Vec<Warning>) {
    let mut kept = Vec::new();
    let mut warnings = Vec::new();
    for code in spec.codes() {
        if installed.contains(code) {
            if !kept.contains(code) {
                kept.push(code.clone());
            }
        } else {
            warnings.push(missing(code));
        }
    }
    if kept.is_empty() {
        kept.push(FALLBACK.to_owned());
    }
    (LangSpec(kept), warnings)
}

/// Stack a second language onto `spec` when it holds at least `share_min` of `block_langs` and
/// its traineddata is installed. The most frequent other language is the candidate; ties go to
/// the traineddata name that sorts first, so the choice is deterministic.
pub fn stack_second(
    spec: LangSpec,
    block_langs: &[LangTag],
    installed: &BTreeSet<String>,
    share_min: f64,
) -> LangSpec {
    if block_langs.is_empty() {
        return spec;
    }
    let mut counts: std::collections::BTreeMap<&'static str, usize> =
        std::collections::BTreeMap::new();
    for tag in block_langs {
        let code = tesseract_code(tag);
        if !spec.codes().iter().any(|have| have == code) {
            *counts.entry(code).or_default() += 1;
        }
    }
    let Some((code, count)) = counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0)))
    else {
        return spec;
    };
    let share = count as f64 / block_langs.len() as f64;
    if share < share_min || !installed.contains(code) {
        return spec;
    }
    let mut codes = spec.0;
    codes.push(code.to_owned());
    LangSpec(codes)
}

/// How to install the traineddata `code` on `os`: a command where there is one, and the
/// installer's own option where there is not. The app never opens a URL itself (D13.9).
pub fn install_hint(os: Os, code: &str) -> String {
    match os {
        Os::Linux => format!("sudo apt install tesseract-ocr-{code}"),
        Os::MacOs => "brew install tesseract-lang".to_owned(),
        Os::Windows => format!(
            "re-run the UB-Mannheim Tesseract installer and select the \"{code}\" \
             traineddata under Additional language data"
        ),
    }
}

fn missing(code: &str) -> Warning {
    Warning::new(W_OCR_LANG_MISSING, Severity::Warn)
        .with_arg("lang", code)
        .with_arg("hint", install_hint(Os::current(), code))
}
