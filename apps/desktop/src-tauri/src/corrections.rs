//! The user's corrections to a book, kept as its `overrides.json` (ARCHITECTURE §4.7).
//!
//! The editors send what changed since the book they are looking at; the file holds everything
//! the user has corrected in this book so far. The two differ after the first "Fix and rebuild":
//! a rebuild starts from the book as `structure` left it (A12.4b), so a second correction — the
//! TOC after the title — has to carry the first one with it, or the rebuild would quietly undo it.
//! [`merge`] is that rule, and the only place it lives.

use std::path::Path;

use oc_model::overrides::{MetadataPatch, Overrides, TocPatch};
use serde::Deserialize;

use crate::engine::UiError;

/// What an editor sends: the fields the user changed, and nothing they did not touch.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    #[serde(default)]
    pub metadata: Option<MetadataPatch>,
    #[serde(default)]
    pub toc: Vec<TocPatch>,
}

/// `patch` folded into `saved`: a field set in the patch replaces the saved one, a field the patch
/// leaves out keeps it, and a heading is matched by its id.
pub fn merge(mut saved: Overrides, patch: Patch) -> Overrides {
    if let Some(new) = patch.metadata {
        let old = saved.metadata.take().unwrap_or_default();
        saved.metadata = Some(MetadataPatch {
            title: new.title.or(old.title),
            authors: new.authors.or(old.authors),
            language: new.language.or(old.language),
        });
    }
    if !patch.toc.is_empty() {
        let mut toc = saved.toc.take().unwrap_or_default();
        for new in patch.toc {
            match toc.iter_mut().find(|old| old.heading == new.heading) {
                Some(old) => {
                    if new.title.is_some() {
                        old.title = new.title;
                    }
                    if new.level.is_some() {
                        old.level = new.level;
                    }
                }
                None => toc.push(new),
            }
        }
        saved.toc = Some(toc);
    }
    saved
}

/// Which book, and which IR, the report of a run is about: its input digest and its engine's
/// `ir_version`, the two things a corrections file is keyed by.
pub fn keyed_by(report: &serde_json::Value) -> Result<(String, u32), UiError> {
    let sha256 = report["input"]["sha256"]
        .as_str()
        .ok_or_else(|| UiError::Io("the report names no input digest".to_owned()))?;
    let ir_version = report["engine"]["ir_version"]
        .as_u64()
        .and_then(|version| u32::try_from(version).ok())
        .ok_or_else(|| UiError::Io("the report names no IR version".to_owned()))?;
    Ok((sha256.to_owned(), ir_version))
}

/// The book's corrections with `patch` added, written to `path` and returned.
///
/// `sha256` and `ir_version` come from the report of the run the user was looking at, so the file
/// names the same bytes and the same block ids that report did. A file already at `path` that this
/// version cannot read — made for another IR, say — is replaced: the user is entering the
/// corrections again, which is what the refusal asked them to do.
pub fn save(
    path: &Path,
    sha256: &str,
    ir_version: u32,
    patch: Patch,
) -> Result<Overrides, UiError> {
    let saved = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| Overrides::parse(&text, sha256).ok())
        .filter(|saved| saved.ir_version == ir_version)
        .unwrap_or_else(|| Overrides {
            ir_version,
            ..Overrides::new(sha256)
        });
    let merged = merge(saved, patch);
    let text = merged
        .to_json()
        .map_err(|error| UiError::Io(error.to_string()))?;
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, text)?;
    std::fs::rename(&partial, path)?;
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_model::ids::BlockId;

    const SHA: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn heading(n: u32) -> BlockId {
        BlockId::derive(
            n,
            oc_model::geom::Rect {
                x0: 0.0,
                y0: 0.0,
                x1: 10.0,
                y1: 10.0,
            },
            "Heading",
        )
    }

    #[test]
    fn a_second_correction_keeps_the_first() {
        let root = std::env::temp_dir().join(format!("oc-desktop-corr-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("made");
        let path = root.join(format!("{SHA}.json"));

        save(
            &path,
            SHA,
            oc_model::IR_VERSION,
            Patch {
                metadata: Some(MetadataPatch {
                    title: Some("The Title".to_owned()),
                    ..MetadataPatch::default()
                }),
                toc: vec![TocPatch {
                    heading: heading(1),
                    title: Some("One".to_owned()),
                    level: None,
                }],
            },
        )
        .expect("saved");
        let second = save(
            &path,
            SHA,
            oc_model::IR_VERSION,
            Patch {
                metadata: Some(MetadataPatch {
                    authors: Some(vec!["Ada Reader".to_owned()]),
                    ..MetadataPatch::default()
                }),
                toc: vec![
                    TocPatch {
                        heading: heading(1),
                        title: None,
                        level: Some(2),
                    },
                    TocPatch {
                        heading: heading(2),
                        title: Some("Two".to_owned()),
                        level: None,
                    },
                ],
            },
        )
        .expect("saved");

        let metadata = second.metadata.clone().expect("metadata");
        assert_eq!(
            metadata.title.as_deref(),
            Some("The Title"),
            "the first edit stays"
        );
        assert_eq!(metadata.authors, Some(vec!["Ada Reader".to_owned()]));
        assert_eq!(
            second.toc.clone().expect("toc"),
            vec![
                TocPatch {
                    heading: heading(1),
                    title: Some("One".to_owned()),
                    level: Some(2),
                },
                TocPatch {
                    heading: heading(2),
                    title: Some("Two".to_owned()),
                    level: None,
                },
            ]
        );
        let on_disk = std::fs::read_to_string(&path).expect("written");
        assert_eq!(Overrides::parse(&on_disk, SHA), Ok(second));

        // A file this version cannot read is replaced, not merged into.
        std::fs::write(&path, r#"{"ir_version": 0, "source_sha256": "x"}"#).expect("written");
        let fresh = save(&path, SHA, oc_model::IR_VERSION, Patch::default()).expect("saved");
        assert!(fresh.is_empty());
    }
}
