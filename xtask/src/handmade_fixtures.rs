//! `cargo xtask handmade-fixtures` — write the hand-made PDFs to `corpus/fixtures/handmade/`.
//!
//! The builders live in `oc-testkit`; this task only writes their output. The files are
//! committed because a fixture has to be the same bytes for everyone, and because a builder
//! change should show up as a reviewable diff rather than as a silently different test.

use std::path::Path;

use anyhow::{Context, Result};

const OUT_DIR: &str = "corpus/fixtures/handmade";

pub fn run(workspace_root: &Path) -> Result<()> {
    let out = workspace_root.join(OUT_DIR);
    std::fs::create_dir_all(&out).with_context(|| format!("cannot create {}", out.display()))?;

    for (name, bytes) in oc_testkit::handmade::all() {
        let path = out.join(format!("{name}.pdf"));
        let unchanged = std::fs::read(&path).is_ok_and(|existing| existing == bytes);
        std::fs::write(&path, &bytes)
            .with_context(|| format!("cannot write {}", path.display()))?;
        println!(
            "{} ({} bytes){}",
            path.display(),
            bytes.len(),
            if unchanged { "" } else { " CHANGED" }
        );
    }
    Ok(())
}
