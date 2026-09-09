//! `cargo xtask mutations` — write the mutated fixtures to `corpus/fixtures/mutations/`.
//!
//! A mutation is a fixture rewritten so that one thing about the *file* changes and nothing
//! about its *content* does. The recipes live in `oc-testkit`; this task applies them to the
//! committed fixtures and writes the results, so that what a metamorphic test ingests is a
//! file a reviewer can open, and a change to a recipe shows up as a diff rather than as a
//! quietly different test.

use std::path::Path;

use anyhow::{Context, Result};

const OUT_DIR: &str = "corpus/fixtures/mutations";
const HANDMADE_DIR: &str = "corpus/fixtures/handmade";

/// How far test 1.6's CropBox moves, in points. The same offset the test applies in process:
/// the committed file and the runtime mutation have to describe the same page.
const CROP_SHIFT_PT: (f32, f32) = (50.0, 50.0);

pub fn run(workspace_root: &Path) -> Result<()> {
    let out = workspace_root.join(OUT_DIR);
    std::fs::create_dir_all(&out).with_context(|| format!("cannot create {}", out.display()))?;

    let source = workspace_root.join(HANDMADE_DIR).join("h01_two_glyphs.pdf");
    let bytes = std::fs::read(&source).with_context(|| {
        format!(
            "cannot read {}; run `cargo run -p xtask -- handmade-fixtures` first",
            source.display()
        )
    })?;

    let mutated = oc_testkit::mutate::cropbox_offset(&bytes, CROP_SHIFT_PT.0, CROP_SHIFT_PT.1)
        .context("cropbox_offset failed on h01")?;
    write(&out.join("h01__cropbox_offset.pdf"), &mutated)?;

    Ok(())
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let unchanged = std::fs::read(path).is_ok_and(|existing| existing == bytes);
    std::fs::write(path, bytes).with_context(|| format!("cannot write {}", path.display()))?;
    println!(
        "{} ({} bytes){}",
        path.display(),
        bytes.len(),
        if unchanged { "" } else { " CHANGED" }
    );
    Ok(())
}
