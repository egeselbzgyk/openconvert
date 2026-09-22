//! The directories the app owns, and the rules for paths that go into a job spec.
//!
//! The engine gets one argument, a path, and the argument is only as meaningful as the rule
//! about where that path may point (RT B15): **a job spec lives in the app's own job directory
//! and nowhere else.** [`is_inside`] is that rule, applied to canonical paths so that `..` and
//! symbolic links cannot walk a spec out of the directory.

use std::path::{Component, Path, PathBuf};

/// The app's own directories, under its data directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppDirs {
    /// Where job specs are written, one file per job. Nothing else writes here.
    pub jobs: PathBuf,
}

impl AppDirs {
    /// The directories under `data_dir`, created if missing.
    pub fn under(data_dir: &Path) -> std::io::Result<Self> {
        let jobs = data_dir.join("jobs");
        std::fs::create_dir_all(&jobs)?;
        Ok(Self { jobs })
    }
}

/// Whether `path` is inside `dir`, after both are made canonical.
///
/// A path that does not exist yet is judged by its canonical parent: a spec is checked after it
/// is written, but the rule should not depend on that.
pub fn is_inside(dir: &Path, path: &Path) -> bool {
    let Ok(dir) = dir.canonicalize() else {
        return false;
    };
    let resolved = match path.canonicalize() {
        Ok(resolved) => resolved,
        Err(_) => {
            // Refuse any `..` outright rather than reason about it without the filesystem.
            if path.components().any(|c| matches!(c, Component::ParentDir)) {
                return false;
            }
            match (path.parent().map(Path::canonicalize), path.file_name()) {
                (Some(Ok(parent)), Some(name)) => parent.join(name),
                _ => return false,
            }
        }
    };
    resolved.starts_with(&dir) && resolved != dir
}

/// `<stem>.epub` beside the input: where a dropped book is written unless Settings says otherwise.
pub fn default_output_for(input: &Path) -> PathBuf {
    input.with_extension("epub")
}

/// `desired`, or the first of `name (2).epub`, `name (3).epub`, … that does not exist.
///
/// The app never overwrites a file the user already has; it saves beside it and says so on the
/// output row (design decision, `result.html` §5). The engine enforces the same rule from the other
/// side: a job spec without `overwrite: true` refuses an existing output.
pub fn free_output_path(desired: &Path) -> PathBuf {
    if !desired.exists() {
        return desired.to_path_buf();
    }
    let stem = desired
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = desired
        .extension()
        .map(|extension| format!(".{}", extension.to_string_lossy()))
        .unwrap_or_default();
    let parent = desired.parent().unwrap_or_else(|| Path::new(""));
    // Counting starts at the second copy, as a person would name it.
    (2_u32..)
        .map(|n| parent.join(format!("{stem} ({n}){extension}")))
        .find(|candidate| !candidate.exists())
        .unwrap_or_else(|| desired.to_path_buf())
}

/// A dropped or picked set of paths, split into what is converted and what is not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Drop {
    /// Files that name a PDF, in the order they were dropped.
    pub pdfs: Vec<PathBuf>,
    /// Everything else, each of which the UI names ("… is not a PDF and will be skipped").
    pub skipped: Vec<PathBuf>,
}

/// Split a drop. **The whole drop is never rejected** because of one file in it (design
/// decision 9): PDFs are added and every other file is named.
///
/// Judged by extension, deliberately: the drop handler must not read the user's files, and a file
/// that calls itself a PDF and is not one fails in the engine with a message that says so.
pub fn partition_drop(paths: impl IntoIterator<Item = PathBuf>) -> Drop {
    let mut drop = Drop::default();
    for path in paths {
        let is_pdf = !path.is_dir()
            && path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"));
        if is_pdf {
            drop.pdfs.push(path);
        } else {
            drop.skipped.push(path);
        }
    }
    drop
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oc-desktop-fs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("made");
        dir
    }

    #[test]
    fn a_spec_must_be_inside_the_jobs_directory() {
        let root = scratch("inside");
        let dirs = AppDirs::under(&root).expect("made");
        let spec = dirs.jobs.join("job-1.json");
        std::fs::write(&spec, "{}").expect("written");

        assert!(is_inside(&dirs.jobs, &spec));
        assert!(is_inside(&dirs.jobs, &dirs.jobs.join("not-yet.json")));
        assert!(!is_inside(&dirs.jobs, &root.join("elsewhere.json")));
        assert!(!is_inside(&dirs.jobs, &dirs.jobs.join("../escape.json")));
        assert!(
            !is_inside(&dirs.jobs, &dirs.jobs),
            "the directory is not a spec"
        );
    }

    #[test]
    fn an_existing_output_is_never_overwritten() {
        let root = scratch("free");
        let desired = root.join("Book.epub");
        assert_eq!(free_output_path(&desired), desired);

        std::fs::write(&desired, "x").expect("written");
        assert_eq!(free_output_path(&desired), root.join("Book (2).epub"));
        std::fs::write(root.join("Book (2).epub"), "x").expect("written");
        assert_eq!(free_output_path(&desired), root.join("Book (3).epub"));
    }

    #[test]
    fn a_mixed_drop_keeps_the_pdfs_and_names_the_rest() {
        let drop = partition_drop([
            PathBuf::from("/a/one.pdf"),
            PathBuf::from("/a/notes.txt"),
            PathBuf::from("/a/TWO.PDF"),
            PathBuf::from("/a/picture.png"),
        ]);
        assert_eq!(
            drop.pdfs,
            [PathBuf::from("/a/one.pdf"), PathBuf::from("/a/TWO.PDF")]
        );
        assert_eq!(
            drop.skipped,
            [
                PathBuf::from("/a/notes.txt"),
                PathBuf::from("/a/picture.png")
            ]
        );
    }
}
