//! `cargo xtask mutations` — write the mutated fixtures to `corpus/fixtures/mutations/`.
//!
//! A mutation is a fixture rewritten so that one thing about the *file* changes and nothing
//! about its *content* does. The recipes live in `oc-testkit`; this task applies them to the
//! fixtures they belong to and writes the results, so that what a metamorphic test ingests is
//! a file a reviewer can open, and a change to a recipe shows up as a diff rather than as a
//! quietly different test.
//!
//! The catalogue is data ([`catalogue`]), not a sequence of statements, because Phase 7 row
//! 7.6 asks that *every* recipe be shown to reproduce its committed mutant. A list a test can
//! walk is the only way that question has one answer.
//!
//! **Two of the recipes are not reproducible, and say so.** AES-128 draws a fresh
//! initialisation vector for every string and stream it encrypts, so encrypting the same file
//! twice gives two different files — different lengths, even. That is the algorithm working.
//! A randomised mutant is therefore written once and then left alone ([`Reproducibility`]),
//! because rewriting it on every run would replace a regression artefact with noise and put an
//! unreviewable diff in front of whoever ran the task.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use oc_testkit::mutate::{self, MutateError};

const OUT_DIR: &str = "corpus/fixtures/mutations";
const HANDMADE_DIR: &str = "corpus/fixtures/handmade";
const TYPST_DIR: &str = "corpus/fixtures/typst";

/// How far test 1.6's CropBox moves, in points. The same offset the test applies in process:
/// the committed file and the runtime mutation have to describe the same page.
pub const CROP_SHIFT_PT: (f32, f32) = (50.0, 50.0);

/// The passwords the encrypted fixtures use. Committed in the open on purpose: a fixture
/// password is a test input, not a secret, and a test that cannot say what password it used
/// is a test nobody can reproduce.
pub const OWNER_PASSWORD: &str = "owner";
pub const USER_PASSWORD: &str = "secret";

/// Where a recipe's input comes from.
///
/// A handmade fixture is committed and read from disk. A Typst fixture is not — it is compiled
/// into the git-ignored `target/fixtures/`, and its *mutation* is what gets committed — so it
/// is compiled here, in process, rather than read from a directory an earlier task may not
/// have filled.
#[derive(Clone, Copy, Debug)]
pub enum Parent {
    Handmade(&'static str),
    Typst { stem: &'static str, tagged: bool },
}

/// Whether applying a recipe twice gives the same bytes twice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reproducibility {
    /// The recipe is a pure function of its input. The committed mutant must match it exactly.
    Deterministic,
    /// The recipe draws randomness — an AES initialisation vector — so no two runs agree.
    /// The committed mutant is one valid output of it and is not regenerated.
    Randomised,
}

/// What a recipe promises about the characters on page 1 of its output.
///
/// This is the half of a mutation that matters. A recipe that changes the file and leaves the
/// content alone is a test of robustness; one that changes the content is a test of something
/// else, and saying which in the catalogue is what keeps the two from being confused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// The same characters, in the same order, as the parent.
    Preserves,
    /// Every character twice — the overdraw and OCR-sandwich cases the pipeline collapses.
    Duplicates,
    /// The characters can no longer be recovered at all: the file still draws the same ink,
    /// and nothing left in it says what that ink means.
    BreaksTextMapping,
    /// Not checkable by extracting page 1: the file will not open without a password.
    Unopenable,
}

/// One entry in the mutation catalogue.
pub struct Recipe {
    /// The file name the mutant is committed under.
    pub mutant: &'static str,
    pub parent: Parent,
    pub apply: fn(&[u8]) -> Result<Vec<u8>, MutateError>,
    pub reproducibility: Reproducibility,
    pub effect: Effect,
}

fn cropbox_offset(bytes: &[u8]) -> Result<Vec<u8>, MutateError> {
    mutate::cropbox_offset(bytes, CROP_SHIFT_PT.0, CROP_SHIFT_PT.1)
}

fn encrypt_empty_user(bytes: &[u8]) -> Result<Vec<u8>, MutateError> {
    mutate::encrypt(
        bytes,
        mutate::EncryptOptions {
            owner_password: OWNER_PASSWORD,
            user_password: "",
            allow_printing: true,
        },
    )
}

fn encrypt_password(bytes: &[u8]) -> Result<Vec<u8>, MutateError> {
    mutate::encrypt(
        bytes,
        mutate::EncryptOptions {
            owner_password: OWNER_PASSWORD,
            user_password: USER_PASSWORD,
            allow_printing: true,
        },
    )
}

fn encrypt_no_print(bytes: &[u8]) -> Result<Vec<u8>, MutateError> {
    mutate::encrypt(
        bytes,
        mutate::EncryptOptions {
            owner_password: OWNER_PASSWORD,
            user_password: "",
            allow_printing: false,
        },
    )
}

/// The mutation catalogue, keyed to the failure taxonomy (IMPLEMENTATION_PLAN PHASE 7 §4).
pub fn catalogue() -> Vec<Recipe> {
    use Effect::{BreaksTextMapping, Duplicates, Preserves, Unopenable};
    use Parent::{Handmade, Typst};
    use Reproducibility::{Deterministic, Randomised};

    const H01: Parent = Handmade("h01_two_glyphs.pdf");
    const F01: Parent = Typst {
        stem: "f01_prose_single_column",
        tagged: false,
    };
    const F01_TAGGED: Parent = Typst {
        stem: "f01_prose_single_column",
        tagged: true,
    };

    vec![
        Recipe {
            mutant: "h01__cropbox_offset.pdf",
            parent: H01,
            apply: cropbox_offset,
            reproducibility: Deterministic,
            effect: Preserves,
        },
        Recipe {
            mutant: "f01__strip_tounicode.pdf",
            parent: F01,
            apply: mutate::strip_tounicode,
            reproducibility: Deterministic,
            effect: BreaksTextMapping,
        },
        // The struct tree can only be stripped from a file that has one, so this is the only
        // recipe whose parent is the tagged variant.
        Recipe {
            mutant: "f01__strip_structtree.pdf",
            parent: F01_TAGGED,
            apply: mutate::strip_structtree,
            reproducibility: Deterministic,
            effect: Preserves,
        },
        Recipe {
            mutant: "f01__double_draw.pdf",
            parent: F01,
            apply: mutate::double_draw,
            reproducibility: Deterministic,
            effect: Duplicates,
        },
        Recipe {
            mutant: "f01__ocr_sandwich.pdf",
            parent: F01,
            apply: mutate::ocr_sandwich,
            reproducibility: Deterministic,
            effect: Duplicates,
        },
        Recipe {
            mutant: "f01__jitter_spacing.pdf",
            parent: F01,
            apply: mutate::jitter_spacing,
            reproducibility: Deterministic,
            effect: Preserves,
        },
        Recipe {
            mutant: "f01__damage_xref.pdf",
            parent: F01,
            apply: mutate::damage_xref,
            reproducibility: Deterministic,
            effect: Preserves,
        },
        // The three encrypted forms Phase 1 detail 7 has to tell apart. All are AES-128,
        // which is what the great majority of encrypted PDFs in circulation use.
        Recipe {
            // The commonest kind by far: encrypted to carry permission flags, open to anyone.
            mutant: "h01__encrypted_empty_user.pdf",
            parent: H01,
            apply: encrypt_empty_user,
            reproducibility: Randomised,
            effect: Preserves,
        },
        Recipe {
            mutant: "h01__encrypted_password.pdf",
            parent: H01,
            apply: encrypt_password,
            reproducibility: Randomised,
            effect: Unopenable,
        },
        Recipe {
            // Opens without a password and forbids printing: the case D13.11 is about, where
            // the flag must be recorded and must not be obeyed.
            mutant: "h01__encrypted_no_print.pdf",
            parent: H01,
            apply: encrypt_no_print,
            reproducibility: Randomised,
            effect: Preserves,
        },
    ]
}

/// Read or compile a recipe's input.
pub fn parent_bytes(workspace_root: &Path, parent: Parent) -> Result<Vec<u8>> {
    match parent {
        Parent::Handmade(name) => {
            let path = workspace_root.join(HANDMADE_DIR).join(name);
            std::fs::read(&path).with_context(|| {
                format!(
                    "cannot read {}; run `cargo run -p xtask -- handmade-fixtures` first",
                    path.display()
                )
            })
        }
        Parent::Typst { stem, tagged } => {
            let source = workspace_root.join(TYPST_DIR).join(format!("{stem}.typ"));
            crate::fixtures::compile_fixture(workspace_root, &source, tagged)
                .with_context(|| format!("cannot compile {}", source.display()))
        }
    }
}

pub fn mutant_path(workspace_root: &Path, recipe: &Recipe) -> PathBuf {
    workspace_root.join(OUT_DIR).join(recipe.mutant)
}

pub fn run(workspace_root: &Path) -> Result<()> {
    let out = workspace_root.join(OUT_DIR);
    std::fs::create_dir_all(&out).with_context(|| format!("cannot create {}", out.display()))?;

    for recipe in catalogue() {
        let path = mutant_path(workspace_root, &recipe);

        if recipe.reproducibility == Reproducibility::Randomised && path.exists() {
            println!("{} (kept: randomised)", path.display());
            continue;
        }

        let parent = parent_bytes(workspace_root, recipe.parent)?;
        let bytes = (recipe.apply)(&parent)
            .with_context(|| format!("recipe for {} failed", recipe.mutant))?;
        write(&path, &bytes)?;
    }

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

#[cfg(test)]
mod tests {
    use super::{catalogue, mutant_path, parent_bytes, Effect, Reproducibility};
    use crate::fixtures::workspace_root_for_test;

    /// PHASE 7 row 7.6: each recipe applied to its parent reproduces the committed mutant
    /// byte-for-byte.
    ///
    /// For the two-thirds of the catalogue that is a pure function of its input, that is the
    /// assertion as written. For the encrypted three it cannot be — AES-128 draws a fresh
    /// initialisation vector per string and stream — so what is asserted instead is the
    /// property that makes byte-equality impossible: two applications of the same recipe to
    /// the same parent differ. A recipe that quietly became deterministic would be a broken
    /// cipher, and this notices.
    #[test]
    fn every_mutation_recipe_replays() {
        let root = workspace_root_for_test();

        for recipe in catalogue() {
            let parent = parent_bytes(&root, recipe.parent)
                .unwrap_or_else(|error| panic!("{}: {error:?}", recipe.mutant));
            let produced = (recipe.apply)(&parent)
                .unwrap_or_else(|error| panic!("{}: {error:?}", recipe.mutant));
            let path = mutant_path(&root, &recipe);
            let committed = std::fs::read(&path).unwrap_or_else(|error| {
                panic!(
                    "{} is missing ({error}); run `cargo run -p xtask -- mutations`",
                    path.display()
                )
            });

            assert!(
                committed.starts_with(b"%PDF-"),
                "{} is not a PDF",
                recipe.mutant
            );

            match recipe.reproducibility {
                Reproducibility::Deterministic => assert_eq!(
                    produced,
                    committed,
                    "{} has drifted from its recipe: the committed file is {} bytes and the \
                     recipe now produces {}. Run `cargo run -p xtask -- mutations` and review \
                     the diff",
                    recipe.mutant,
                    committed.len(),
                    produced.len(),
                ),
                Reproducibility::Randomised => {
                    let again = (recipe.apply)(&parent)
                        .unwrap_or_else(|error| panic!("{}: {error:?}", recipe.mutant));
                    assert_ne!(
                        produced, again,
                        "{} is declared randomised but produced the same bytes twice",
                        recipe.mutant
                    );
                }
            }
        }
    }

    /// Every recipe does to the page's characters exactly what its `effect` says.
    ///
    /// A mutation nobody can ingest is not a fixture, it is a broken file, and the way that
    /// goes unnoticed is that the recipe is only ever checked against its own output. This
    /// opens each mutant with the extractor the pipeline uses and compares it with its parent.
    #[test]
    fn every_mutation_does_what_its_effect_promises() {
        use oc_pdf::inspect::PdfOpen as _;

        let root = workspace_root_for_test();
        let backend = oc_pdf::pdfium::PdfiumBackend::bind().expect("PDFium is vendored");

        let page_text = |bytes: &[u8]| -> Option<String> {
            let document = backend.open(bytes, None).ok()?;
            let page = document.page_glyphs(0).ok()?;
            Some(page.glyphs.iter().map(|glyph| glyph.ch).collect())
        };

        for recipe in catalogue() {
            let parent = parent_bytes(&root, recipe.parent)
                .unwrap_or_else(|error| panic!("{}: {error:?}", recipe.mutant));
            let committed = std::fs::read(mutant_path(&root, &recipe))
                .unwrap_or_else(|error| panic!("{}: {error}", recipe.mutant));

            let before = page_text(&parent)
                .unwrap_or_else(|| panic!("{}: the parent does not open", recipe.mutant));

            match recipe.effect {
                Effect::Preserves => {
                    let after = page_text(&committed).unwrap_or_else(|| {
                        panic!(
                            "{} does not open; a mutant has to stay readable",
                            recipe.mutant
                        )
                    });
                    assert_eq!(after, before, "{} changed the page's text", recipe.mutant);
                }
                Effect::Duplicates => {
                    let after = page_text(&committed).unwrap_or_else(|| {
                        panic!(
                            "{} does not open; a mutant has to stay readable",
                            recipe.mutant
                        )
                    });
                    assert_eq!(
                        after.chars().count(),
                        before.chars().count() * 2,
                        "{} should carry every character twice",
                        recipe.mutant
                    );
                    assert!(
                        after.starts_with(&before),
                        "{}'s first copy should be the original text",
                        recipe.mutant
                    );
                }
                Effect::BreaksTextMapping => {
                    let after = page_text(&committed).unwrap_or_else(|| {
                        panic!(
                            "{} does not open; a mutant has to stay readable",
                            recipe.mutant
                        )
                    });
                    assert_ne!(
                        after, before,
                        "{} still decodes to the same text, so nothing was broken",
                        recipe.mutant
                    );
                }
                Effect::Unopenable => assert!(
                    page_text(&committed).is_none(),
                    "{} opened without the password it is supposed to need",
                    recipe.mutant
                ),
            }
        }
    }

    /// A catalogue with two entries under one name would silently overwrite one of them.
    #[test]
    fn every_mutant_in_the_catalogue_has_its_own_name() {
        let mut names: Vec<&str> = catalogue().iter().map(|recipe| recipe.mutant).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "two recipes share a mutant name");
    }

    /// Every committed mutant is one the catalogue knows how to make again.
    ///
    /// A file left behind by a recipe that was renamed or deleted is a fixture nothing
    /// produces, and the next person to read it has no way to find out what it is.
    #[test]
    fn no_committed_mutant_is_orphaned() {
        let root = workspace_root_for_test();
        let known: Vec<&str> = catalogue().iter().map(|recipe| recipe.mutant).collect();

        let dir = root.join(super::OUT_DIR);
        for entry in std::fs::read_dir(&dir).expect("the mutations directory exists") {
            let name = entry.expect("a readable directory entry").file_name();
            let name = name.to_string_lossy().into_owned();
            if !name.ends_with(".pdf") {
                continue;
            }
            assert!(
                known.contains(&name.as_str()),
                "{name} is committed but no recipe in the catalogue produces it"
            );
        }
    }
}
