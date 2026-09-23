//! The registry: every warning code the engine can emit, and the arguments each carries.
//!
//! **One list, in one place, and the reason it is not derived.** The codes themselves are
//! `pub const`s in the crate that raises them — `oc-structure` owns `W_TABLE_AS_IMAGE` because
//! `oc-structure` is what decides a table cannot be recovered — and `oc-core` cannot depend on any
//! of them without a cycle. So the registry is written by hand and `xtask ci-lint` holds it and the
//! tree in agreement: a `W_…` constant anywhere in the workspace that is not listed here fails the
//! lint, and so does a code listed here that nothing defines.
//!
//! **The arguments are declared, not discovered.** A template is checked against this list, so a
//! translation that writes `{page}` where the code carries `{pages}` fails a test rather than
//! rendering a literal brace to a user. That is the whole reason warnings are templates and not
//! model-phrased prose: a warning is a factual claim about what the software did, and the claim has
//! to survive translation intact (R10 §6.20).

/// One code, and the slots its template may fill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WarningSpec {
    /// The code, e.g. `W_TABLE_AS_IMAGE`.
    pub code: &'static str,
    /// The argument names the raising site sets, in the order a template would read them.
    pub args: &'static [&'static str],
}

/// Every warning code, sorted by code.
///
/// Sorted, and asserted to be: a reader looking for one should be able to find it, and a reviewer
/// should be able to see the whole set at once.
pub const CODES: &[WarningSpec] = &[
    WarningSpec {
        code: "W_BROKEN_TEXT_PAGES",
        args: &["count"],
    },
    WarningSpec {
        code: "W_CAPTION_AMBIGUOUS",
        args: &["caption", "reason", "best", "second"],
    },
    WarningSpec {
        code: "W_DUPLICATE_BLOCKS",
        args: &["fraction", "bound", "blocks", "worst"],
    },
    WarningSpec {
        code: "W_EPUB_INVALID",
        args: &["status", "fatal", "error", "ids"],
    },
    WarningSpec {
        code: "W_EPUB_LARGE",
        args: &[],
    },
    WarningSpec {
        code: "W_HEADINGS_OUT_OF_PAGE_ORDER",
        args: &[],
    },
    WarningSpec {
        code: "W_HEADING_COUNT_IMPLAUSIBLE",
        args: &["count", "min", "max"],
    },
    WarningSpec {
        code: "W_HEADING_LEVEL_SKIP",
        args: &["count", "first"],
    },
    WarningSpec {
        code: "W_IMAGE_ONLY_PAGES",
        args: &["count"],
    },
    WarningSpec {
        code: "W_LANG_FALLBACK",
        args: &[],
    },
    WarningSpec {
        code: "W_LANG_UNSTABLE",
        args: &[],
    },
    WarningSpec {
        code: "W_LIST_NUMBERING_GAP",
        args: &["numbers"],
    },
    WarningSpec {
        code: "W_LLM_BUDGET_EXHAUSTED",
        args: &["task", "calls"],
    },
    WarningSpec {
        code: "W_LLM_PREFIX_COLD",
        args: &["call", "task"],
    },
    WarningSpec {
        code: "W_LLM_TIME_EXHAUSTED",
        args: &["task", "share"],
    },
    WarningSpec {
        code: "W_LLM_UNAVAILABLE",
        args: &["reason"],
    },
    WarningSpec {
        code: "W_LOW_RETENTION",
        args: &["retention", "floor"],
    },
    WarningSpec {
        code: "W_NOTE_UNMATCHED",
        args: &["markers", "notes", "matched"],
    },
    WarningSpec {
        code: "W_NO_TEXT_EXTRACTED",
        args: &[],
    },
    WarningSpec {
        code: "W_OCR_ENGINE_MISSING",
        args: &["reason", "hint", "pages"],
    },
    WarningSpec {
        code: "W_OCR_FAILED",
        args: &["page", "reason"],
    },
    WarningSpec {
        code: "W_OCR_LANG_MISSING",
        args: &["lang", "hint"],
    },
    WarningSpec {
        code: "W_OCR_LOW_CONFIDENCE",
        args: &["page", "confidence", "floor"],
    },
    WarningSpec {
        code: "W_ORNAMENT_DROPPED",
        args: &["pages", "page_count"],
    },
    WarningSpec {
        code: "W_PAGE_BREAK_UNPLACED",
        args: &[],
    },
    WarningSpec {
        code: "W_REPAIR_FIRED",
        args: &["repair", "message_id", "location"],
    },
    WarningSpec {
        code: "W_REPAIR_OSCILLATION",
        args: &[],
    },
    WarningSpec {
        code: "W_SECTION_PAGES_NOT_MONOTONE",
        args: &["section", "starts"],
    },
    WarningSpec {
        code: "W_STYLE_INVENTORY_INVALID",
        args: &["clusters", "body_char_share"],
    },
    WarningSpec {
        code: "W_TABLE_AS_IMAGE",
        args: &["rows", "columns", "reason"],
    },
    WarningSpec {
        code: "W_UNMAPPED_VALIDATION_ID",
        args: &["message_id"],
    },
    WarningSpec {
        code: "W_VALIDATION_UNREPAIRABLE",
        args: &["message_id"],
    },
    WarningSpec {
        code: "W_XHTML_OVERSIZE",
        args: &[],
    },
    WarningSpec {
        code: "W_ZONES_OUT_OF_ORDER",
        args: &["zones"],
    },
];

/// The spec for one code, or `None` when the registry does not know it.
pub fn spec(code: &str) -> Option<&'static WarningSpec> {
    CODES.iter().find(|spec| spec.code == code)
}
