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
    /// The engine's cache (`OC_CACHE_DIR`): what each full run's `structure` settled, for "Fix and
    /// rebuild". It holds the text of the books converted, so it is the app's alone and cleared
    /// with [`AppDirs::clear_cache`].
    pub cache: PathBuf,
    /// The user's corrections, one `overrides.json` per book, named by its digest.
    pub overrides: PathBuf,
    /// Secrets that live as long as the app runs: the key of the app's own model server
    /// (`llm.rs`). Emptied at start, so nothing a crash left behind is reused.
    pub run: PathBuf,
}

impl AppDirs {
    /// The directories under `data_dir`, created if missing.
    pub fn under(data_dir: &Path) -> std::io::Result<Self> {
        let dirs = Self {
            jobs: data_dir.join("jobs"),
            cache: data_dir.join("cache"),
            overrides: data_dir.join("overrides"),
            run: data_dir.join("run"),
        };
        for dir in [&dirs.jobs, &dirs.cache, &dirs.overrides, &dirs.run] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(dirs)
    }

    /// Empty the engine's cache. The app does this at start: a rebuild is offered only on a row of
    /// this session's queue, so a save outliving the session would be the text of a book kept on
    /// disk for nothing (SECURITY §10).
    pub fn clear_cache(&self) -> std::io::Result<()> {
        match std::fs::remove_dir_all(&self.cache) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        std::fs::create_dir_all(&self.cache)
    }

    /// Empty the run directory. The app does this at start: a key a crashed session left there is
    /// for a server that no longer exists.
    pub fn clear_run(&self) -> std::io::Result<()> {
        match std::fs::remove_dir_all(&self.run) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        std::fs::create_dir_all(&self.run)
    }

    /// What the cache holds: its size on disk, and how many books have a saved `structure` in it
    /// — what Settings › Advanced says before offering to clear it (SECURITY §10).
    pub fn cache_usage(&self) -> CacheUsage {
        let books = std::fs::read_dir(self.cache.join("structure"))
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
                    .count()
            })
            .unwrap_or_default();
        CacheUsage {
            bytes: size_of_tree(&self.cache),
            books: u32::try_from(books).unwrap_or(u32::MAX),
        }
    }

    /// Where the corrections for the book whose digest is `sha256` are kept. `None` unless the
    /// digest is lowercase hex, which is the only thing that becomes part of the name.
    pub fn overrides_for(&self, sha256: &str) -> Option<PathBuf> {
        let hex = !sha256.is_empty()
            && sha256
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
        hex.then(|| self.overrides.join(format!("{sha256}.json")))
    }
}

/// The cache's size and contents, as Settings shows them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct CacheUsage {
    pub bytes: u64,
    pub books: u32,
}

/// The bytes of every file under `dir`, not following links.
fn size_of_tree(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => size_of_tree(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map(|meta| meta.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
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
    free_output_path_among(desired, |_| false)
}

/// [`free_output_path`], also treating as taken every path `reserved` says is — the outputs of
/// jobs already in the queue, which do not exist yet but will.
pub fn free_output_path_among(desired: &Path, reserved: impl Fn(&Path) -> bool) -> PathBuf {
    let taken = |path: &Path| path.exists() || reserved(path);
    if !taken(desired) {
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
        .find(|candidate| !taken(candidate))
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
    fn the_cache_is_cleared_and_corrections_are_kept_by_digest() {
        let root = scratch("cache");
        let dirs = AppDirs::under(&root).expect("made");
        std::fs::create_dir_all(dirs.cache.join("structure")).expect("made");
        std::fs::write(dirs.cache.join("structure/abc.json"), "{}").expect("written");
        std::fs::write(dirs.cache.join("structure/def.json"), "[1]").expect("written");
        assert_eq!(dirs.cache_usage(), CacheUsage { bytes: 5, books: 2 });
        dirs.clear_cache().expect("cleared");
        assert_eq!(dirs.cache_usage(), CacheUsage::default());
        assert!(dirs.cache.is_dir(), "the directory stays");
        assert_eq!(
            std::fs::read_dir(&dirs.cache).expect("reads").count(),
            0,
            "and nothing in it"
        );

        let sha = "0123456789abcdef".repeat(4);
        assert_eq!(
            dirs.overrides_for(&sha),
            Some(dirs.overrides.join(format!("{sha}.json")))
        );
        for bad in ["", "../x", "ABC", "12/34"] {
            assert_eq!(dirs.overrides_for(bad), None, "{bad:?} names no file");
        }
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
