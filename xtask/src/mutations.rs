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
const COMPILED_DIR: &str = "target/fixtures";

/// How far test 1.6's CropBox moves, in points. The same offset the test applies in process:
/// the committed file and the runtime mutation have to describe the same page.
const CROP_SHIFT_PT: (f32, f32) = (50.0, 50.0);

pub fn run(workspace_root: &Path) -> Result<()> {
    let out = workspace_root.join(OUT_DIR);
    std::fs::create_dir_all(&out).with_context(|| format!("cannot create {}", out.display()))?;

    let h01 = read(
        &workspace_root.join(HANDMADE_DIR).join("h01_two_glyphs.pdf"),
        "handmade-fixtures",
    )?;
    let mutated = oc_testkit::mutate::cropbox_offset(&h01, CROP_SHIFT_PT.0, CROP_SHIFT_PT.1)
        .context("cropbox_offset failed on h01")?;
    write(&out.join("h01__cropbox_offset.pdf"), &mutated)?;

    // `f01` itself is not committed — it is compiled into the ignored `target/fixtures/`, and
    // test 0.20 pins that compiling it twice gives the same bytes. Its mutation *is*
    // committed, because a regression artefact has to be the same file for everyone.
    let f01 = read(
        &workspace_root
            .join(COMPILED_DIR)
            .join("f01_prose_single_column.pdf"),
        "fixtures",
    )?;
    let stripped =
        oc_testkit::mutate::strip_tounicode(&f01).context("strip_tounicode failed on f01")?;
    write(&out.join("f01__strip_tounicode.pdf"), &stripped)?;

    Ok(())
}

fn read(path: &Path, producing_task: &str) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| {
        format!(
            "cannot read {}; run `cargo run -p xtask -- {producing_task}` first",
            path.display()
        )
    })
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
