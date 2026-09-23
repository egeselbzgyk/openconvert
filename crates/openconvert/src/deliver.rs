//! Putting a finished book at the user's path — or nothing at all (D13.2, PHASE 14 row 14.19).
//!
//! The EPUB is written to `<output>.oc-tmp-<token>` in the destination directory and renamed over
//! the destination on success. A conversion that fails, is cancelled, or runs into a cap never gets
//! here, so the destination never holds a partial file; and the rename is on one filesystem by
//! construction, because the temporary sits beside the destination rather than in `/tmp`.

use std::path::Path;

/// Write `bytes` beside `output` and rename them over it.
pub fn write_atomically(output: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let directory = output.parent().unwrap_or_else(|| Path::new("."));
    // Enough entropy that two conversions of one book into one directory do not collide, and
    // no more: this is a temporary name, not a secret.
    let token = format!(
        "{:x}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default(),
        std::process::id()
    );
    let temporary = directory.join(format!(
        "{}.oc-tmp-{token}",
        output
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "out.epub".to_owned())
    ));

    std::fs::write(&temporary, bytes)?;
    match std::fs::rename(&temporary, output) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            Err(error)
        }
    }
}
