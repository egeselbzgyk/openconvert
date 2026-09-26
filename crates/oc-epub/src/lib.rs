#![forbid(unsafe_code)]
//! EPUB 3.3 generation: the typed XHTML builder, OPF/nav/NCX, CSS, splitting,
//! deterministic zip and image encoding (D5).
//!
//! [`build_epub`] is the whole of the `epub` stage. It takes a finished `Document` and the
//! images `ingest` decoded, and returns the bytes of an OCF container — nothing is written to
//! disk here, because the CLI writes to `<output>.oc-tmp-<rand>` and renames atomically on
//! success (D13.2), and a builder that opened files would have to be told about that.
//!
//! The stage is **Conserving** (PIPELINE §10): every character in the output is a character the
//! book contained. What the emitter needs to say for itself — alt text, a page label, an
//! accessibility summary — goes into an attribute or into package metadata, both of which are
//! outside `C` (ARCHITECTURE §5.2).

pub mod content;
pub mod css;
pub mod images;
pub mod nav;
pub mod ncx;
pub mod opf;
mod resample;
pub mod textcontent;
pub mod xhtml;
pub mod zip;

use std::collections::BTreeMap;

use oc_model::document::Document;
use oc_model::extract::ImageId;

use crate::content::{EmitOptions, Emitted};
use crate::images::{ImageOptions, SourceImage};
use crate::zip::ZipEntry;

/// The bytes of one EPUB.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpubBytes(pub Vec<u8>);

impl EpubBytes {
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Everything the emitter is configured by, from `thresholds.toml`.
pub struct EpubOptions {
    /// `xhtml.split_bytes`.
    pub split_bytes: usize,
    /// `images.max_longest_side_px`.
    pub max_longest_side_px: u32,
    /// `images.jpeg_quality`.
    pub jpeg_quality: u8,
    /// `epub.warn_total_bytes`.
    pub warn_total_bytes: u64,
    /// `dcterms:modified`, as `YYYY-MM-DDThh:mm:ssZ`.
    ///
    /// Passed in rather than read from a clock inside the emitter. It is the one field that
    /// differs between two builds of the same book, so a caller that wants byte-identical
    /// output — a test, a golden file, the cross-OS CI gate — has to be able to hold it still
    /// (D13.8).
    pub modified: String,
}

/// Why the EPUB could not be built.
#[derive(Debug, thiserror::Error)]
pub enum EpubError {
    #[error("the document contains a character XML cannot carry: {0}")]
    Markup(#[from] xhtml::IllegalChar),
    #[error(transparent)]
    Image(#[from] images::ImageError),
    #[error(transparent)]
    Container(#[from] zip::ZipError),
}

/// What the stage produced besides the bytes.
pub struct BuiltEpub {
    pub bytes: EpubBytes,
    pub emitted: Emitted,
    /// Codes for the report. `oc-epub` does not depend on the pipeline and has no `Warning` to
    /// build, so the codes travel and the caller attaches the page and the block.
    pub warnings: Vec<&'static str>,
}

/// The book is larger than `epub.warn_total_bytes`.
pub const W_EPUB_LARGE: &str = "W_EPUB_LARGE";

/// Build the container.
pub fn build_epub(
    document: &Document,
    sources: &[SourceImage],
    options: &EpubOptions,
) -> Result<BuiltEpub, EpubError> {
    let (mut encoded, mut paths) = images::encode(
        sources,
        &ImageOptions {
            max_longest_side_px: options.max_longest_side_px,
            jpeg_quality: options.jpeg_quality,
        },
    )?;
    // The cover is filed under its own name, `images/cover.*`: it is not one of the book's
    // images, and a reader of the container can tell which file it is.
    if let Some(cover) = document.cover {
        for image in encoded.iter_mut().filter(|image| image.id == cover) {
            let extension = image.path.rsplit('.').next().unwrap_or("jpg").to_owned();
            image.path = format!("images/cover.{extension}");
            paths.insert(cover, image.path.clone());
        }
    }

    let emitted = content::emit(
        document,
        &paths,
        &EmitOptions {
            split_bytes: options.split_bytes,
        },
    )?;

    let mut emitted = emitted;
    let title_for_cover = document.meta.title.clone().unwrap_or_default();
    let cover_path = document
        .cover
        .and_then(|id| paths.get(&id).map(|path| (id, path.clone())));
    if let Some((_, path)) = &cover_path {
        content::prepend_cover(&mut emitted, document, &title_for_cover, path)?;
    }

    // Only the images a document actually references reach the manifest. An item nothing
    // points at is EPUBCheck's `OPF-003` class, and an image dropped as an ornament or living
    // on a page no section covers is exactly that.
    // The cover is carried beside them: it is a rendering of the first page, not one of the
    // images the book's text uses, and `used_images` stays the count of those.
    let used: Vec<ImageId> = emitted.used_images.clone();
    let cover_id = cover_path.as_ref().map(|(id, _)| *id);
    let images: Vec<images::EncodedImage> = encoded
        .into_iter()
        .filter(|image| used.contains(&image.id) || Some(image.id) == cover_id)
        .collect();

    let title = document
        .meta
        .title
        .clone()
        .unwrap_or_else(|| "Untitled".to_owned());

    let nav_document = nav::nav(&title, &document.language, &emitted, css::STYLE_PATH);
    let ncx_document = ncx::ncx(&title, &document.meta.identifier, &emitted.toc);
    let package = opf::package(
        document,
        &opf::PackageInput {
            emitted: &emitted,
            images: &images,
            nav_path: nav::NAV_PATH,
            ncx_path: ncx::NCX_PATH,
            style_path: css::STYLE_PATH,
            modified: options.modified.clone(),
            cover: cover_path.as_ref().map(|(id, _)| *id),
        },
    );

    let mut entries = vec![
        ZipEntry::new(zip::MIMETYPE_PATH, zip::EPUB_MEDIA_TYPE.as_bytes().to_vec()),
        ZipEntry::new(
            zip::CONTAINER_PATH,
            zip::container_xml(opf::OPF_PATH).into_bytes(),
        ),
        ZipEntry::new(opf::OPF_PATH, package.into_bytes()),
        ZipEntry::new(nav::NAV_PATH, nav_document.into_bytes()),
        ZipEntry::new(ncx::NCX_PATH, ncx_document.into_bytes()),
        ZipEntry::new(css::STYLE_PATH, css::stylesheet().into_bytes()),
    ];
    for file in &emitted.files {
        entries.push(ZipEntry::new(
            file.path.clone(),
            file.markup.clone().into_bytes(),
        ));
    }
    for image in &images {
        entries.push(ZipEntry::new(image.path.clone(), image.bytes.clone()));
    }

    let bytes = EpubBytes(zip::write_deterministic_zip(&entries)?);

    let mut warnings = Vec::new();
    if bytes.len() as u64 > options.warn_total_bytes {
        warnings.push(W_EPUB_LARGE);
    }
    if emitted
        .files
        .iter()
        .any(|file| file.markup.len() > options.split_bytes)
    {
        warnings.push(content::W_XHTML_OVERSIZE);
    }

    Ok(BuiltEpub {
        bytes,
        emitted,
        warnings,
    })
}

/// Every file in a container, by path.
///
/// Reading the archive back rather than remembering what was put in it is deliberate: a Tier-1
/// check that read the emitter's intentions would be checking the emitter against itself, which
/// is precisely the failure mode D6 exists to avoid.
pub fn read_entries(
    epub: &EpubBytes,
) -> Result<BTreeMap<String, Vec<u8>>, ::zip::result::ZipError> {
    use std::io::Read;

    let mut archive = ::zip::ZipArchive::new(std::io::Cursor::new(epub.as_slice()))?;
    let mut out = BTreeMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        out.insert(name, bytes);
    }
    Ok(out)
}
