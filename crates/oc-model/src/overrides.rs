//! User corrections: `overrides.json` (ARCHITECTURE §4.7, IR_SKETCH "overrides").
//!
//! A correction is keyed by what it corrects — the book, by `source_sha256`, and a heading, by
//! its [`BlockId`] — and by the IR version whose ids it names. Block ids are derived from content
//! and geometry (D13.3), so a derivation change renumbers every block and an old file would land
//! its corrections on the wrong headings. The file therefore carries `ir_version` as its first
//! key, and [`Overrides::parse`] **refuses** a file of any other version rather than applying
//! what it can of it (Phase 12 detail 8, row 12.11): a correction applied to the wrong heading is
//! worse than none, and a correction silently dropped is a bug report nobody can file.
//!
//! v1 applies `metadata` and `toc`. `blocks` is in the schema from day one so that the post-v1
//! block-level correction UI is a UI change and not an IR change (D16); a v1 engine refuses a file
//! that uses it, for the same reason it refuses a stale one.

use serde::{Deserialize, Serialize};

use crate::ids::BlockId;

/// The whole file. `ir_version` is declared first so that it is the first key written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overrides {
    pub ir_version: u32,
    /// The lowercase hex SHA-256 of the PDF these corrections are for.
    pub source_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MetadataPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toc: Option<Vec<TocPatch>>,
    /// Reserved for the post-v1 block-level correction UI (D16). Always empty in v1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<BlockOverride>,
}

/// Metadata the user set. A field left `None` keeps what the pipeline found.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The whole list, in order: an author removed in the editor is absent here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,
    /// A BCP-47 tag, e.g. `de` or `tr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// One heading of the table of contents, renamed or moved to another level.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TocPatch {
    /// The heading's block id, as the report lists it.
    pub heading: BlockId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// 1..=6, as XHTML has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
}

/// A block-level correction (D16, post-v1). Parsed so that a v1 engine can name what it refuses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockOverride {
    pub id: BlockId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_with_next: Option<bool>,
}

/// Why a file of corrections was refused. Each says what to do about it.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OverridesError {
    /// Made against another IR: its block ids may name other headings in this version.
    #[error(
        "these corrections were made for IR version {file}; this converter uses IR version \
         {engine}, whose block ids may name different headings, so they were not applied — \
         enter them again"
    )]
    StaleIrVersion { file: u32, engine: u32 },
    /// Made for another PDF.
    #[error("these corrections were made for another PDF (SHA-256 {file}), not this one")]
    OtherSource { file: String },
    /// Uses the block-level corrections this version does not apply (D16).
    #[error("these corrections include block-level changes, which this version does not apply")]
    BlocksReserved,
    /// Not a corrections file this version can read.
    #[error("the corrections file cannot be read: {0}")]
    Unreadable(String),
}

impl Overrides {
    /// Corrections for the book whose digest is `source_sha256`, made against this IR version.
    pub fn new(source_sha256: impl Into<String>) -> Self {
        Self {
            ir_version: crate::IR_VERSION,
            source_sha256: source_sha256.into(),
            metadata: None,
            toc: None,
            blocks: Vec::new(),
        }
    }

    /// Read `text` as the corrections for the book whose digest is `source_sha256`.
    ///
    /// The version is read **before** the shape: a file from another IR may have another shape,
    /// and "made for IR 3" is the message that helps, where "unknown field" is not.
    pub fn parse(text: &str, source_sha256: &str) -> Result<Self, OverridesError> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| OverridesError::Unreadable(e.to_string()))?;
        let version = value
            .get("ir_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| OverridesError::Unreadable("no ir_version".to_owned()))?;
        if version != u64::from(crate::IR_VERSION) {
            return Err(OverridesError::StaleIrVersion {
                file: u32::try_from(version).unwrap_or(u32::MAX),
                engine: crate::IR_VERSION,
            });
        }
        let overrides: Overrides =
            serde_json::from_value(value).map_err(|e| OverridesError::Unreadable(e.to_string()))?;
        if !overrides.source_sha256.eq_ignore_ascii_case(source_sha256) {
            return Err(OverridesError::OtherSource {
                file: overrides.source_sha256,
            });
        }
        if !overrides.blocks.is_empty() {
            return Err(OverridesError::BlocksReserved);
        }
        Ok(overrides)
    }

    /// The file's text: pretty JSON, `ir_version` first.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Whether the file corrects nothing.
    pub fn is_empty(&self) -> bool {
        self.metadata.is_none()
            && self.toc.as_ref().is_none_or(Vec::is_empty)
            && self.blocks.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 12.11 of the Phase 12 table.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn heading() -> BlockId {
        BlockId::derive(
            3,
            crate::geom::Rect {
                x0: 72.0,
                y0: 90.0,
                x1: 300.0,
                y1: 110.0,
            },
            "Chapter One",
        )
    }

    /// 12.11 — a stale `overrides.json` is refused with a clear message, not silently ignored.
    #[test]
    fn overrides_with_wrong_ir_version_are_refused() {
        let older = crate::IR_VERSION - 1;
        let stale = format!(
            r#"{{"ir_version": {older}, "source_sha256": "{SHA}", "metadata": {{"title": "Moby-Dick"}}}}"#
        );
        let refused = Overrides::parse(&stale, SHA).expect_err("a stale file is refused");
        assert_eq!(
            refused,
            OverridesError::StaleIrVersion {
                file: older,
                engine: crate::IR_VERSION
            }
        );
        let message = refused.to_string();
        assert!(
            message.contains(&format!("IR version {older}"))
                && message.contains(&format!("IR version {}", crate::IR_VERSION)),
            "the message names both versions: {message}"
        );
        assert!(message.contains("not applied"), "and says what happened");

        // A file from a later IR, in a shape this one does not know, is refused for its version,
        // not for its shape.
        let newer = crate::IR_VERSION + 1;
        let unknown_shape = format!(
            r#"{{"ir_version": {newer}, "source_sha256": "{SHA}", "chapters": [{{"id": 7}}]}}"#
        );
        assert_eq!(
            Overrides::parse(&unknown_shape, SHA),
            Err(OverridesError::StaleIrVersion {
                file: newer,
                engine: crate::IR_VERSION
            })
        );

        // No version at all is not "the current version".
        assert!(matches!(
            Overrides::parse(&format!(r#"{{"source_sha256": "{SHA}"}}"#), SHA),
            Err(OverridesError::Unreadable(_))
        ));
    }

    #[test]
    fn corrections_for_another_book_or_for_blocks_are_refused_by_name() {
        let mut other = Overrides::new("f".repeat(SHA.len()));
        other.metadata = Some(MetadataPatch {
            title: Some("Other".to_owned()),
            ..MetadataPatch::default()
        });
        let text = other.to_json().expect("serialises");
        assert!(matches!(
            Overrides::parse(&text, SHA),
            Err(OverridesError::OtherSource { .. })
        ));

        let mut blocks = Overrides::new(SHA);
        blocks.blocks.push(BlockOverride {
            id: heading(),
            role: Some("heading".to_owned()),
            level: None,
            merge_with_next: None,
        });
        let text = blocks.to_json().expect("serialises");
        assert_eq!(
            Overrides::parse(&text, SHA),
            Err(OverridesError::BlocksReserved)
        );

        let typo = format!(
            r#"{{"ir_version": {}, "source_sha256": "{SHA}", "metdata": {{}}}}"#,
            crate::IR_VERSION
        );
        assert!(
            matches!(
                Overrides::parse(&typo, SHA),
                Err(OverridesError::Unreadable(_))
            ),
            "a misspelt key is not an empty correction"
        );
    }

    #[test]
    fn a_saved_file_reads_back_as_written_with_its_version_first() {
        let mut overrides = Overrides::new(SHA);
        overrides.metadata = Some(MetadataPatch {
            title: Some("The Wheels of Chance".to_owned()),
            authors: Some(vec!["H. G. Wells".to_owned()]),
            language: Some("en".to_owned()),
        });
        overrides.toc = Some(vec![TocPatch {
            heading: heading(),
            title: Some("I. The Hero of This Story".to_owned()),
            level: Some(2),
        }]);
        let text = overrides.to_json().expect("serialises");
        assert!(
            text.trim_start().starts_with("{\n  \"ir_version\""),
            "ir_version is the first key: {text}"
        );
        assert_eq!(Overrides::parse(&text, SHA), Ok(overrides));
    }
}
