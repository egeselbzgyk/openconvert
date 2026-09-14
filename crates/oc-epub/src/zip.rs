//! The OCF container: a zip whose bytes are fully determined by its contents (D5, D13.8).
//!
//! Two requirements, and they are unrelated to each other.
//!
//! **PKG-007.** OCF requires the first entry to be named `mimetype`, to be stored
//! uncompressed, and to carry no extra field — so that a reader can identify the file by
//! reading bytes 30..38 without parsing the archive at all. Getting it wrong is the classic
//! hand-rolled-zip mistake (R5 §B6), which is why the test for it was written before this
//! file existed.
//!
//! **Byte identity.** `--no-ai` output is byte-identical on the same OS and version, and CI
//! asserts it is identical *across* Linux, macOS and Windows (D13.8, test 5.5). A zip writer
//! defeats that in three ways if left to itself: a timestamp, the host-system byte in the
//! version-made-by field, and the unix-permission extra fields. All three are pinned here —
//! every timestamp is 1980-01-01T00:00:00, no entry carries permissions, and entries are
//! written in a fixed order rather than in whatever order a map iterated.
//!
//! The `zip` crate's API for this exact surface churns across majors (RT B12), so the version
//! is pinned exactly and `golden_epub_bytes_f01` diffs a whole EPUB against committed bytes.

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

/// The entry OCF requires first, stored, with no extra field.
pub const MIMETYPE_PATH: &str = "mimetype";

/// The media type of an EPUB publication.
pub const EPUB_MEDIA_TYPE: &str = "application/epub+zip";

/// Where the OCF container descriptor lives.
pub const CONTAINER_PATH: &str = "META-INF/container.xml";

/// The offset at which a reader finds the first entry's name: 30 bytes of local file header.
pub const MIMETYPE_NAME_OFFSET: usize = 30;

/// One file in the container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZipEntry {
    /// The path inside the container, with `/` separators and no leading slash.
    pub path: String,
    pub data: Vec<u8>,
}

impl ZipEntry {
    pub fn new(path: impl Into<String>, data: impl Into<Vec<u8>>) -> Self {
        Self {
            path: path.into(),
            data: data.into(),
        }
    }
}

/// Why the container could not be written.
#[derive(Debug, thiserror::Error)]
pub enum ZipError {
    #[error("the container has no `{MIMETYPE_PATH}` entry")]
    NoMimetype,
    #[error("two entries collide under a case-insensitive filesystem: {0} and {1}")]
    CaseCollision(String, String),
    #[error("writing the container failed: {0}")]
    Write(#[from] zip::result::ZipError),
    /// Writing into an in-memory cursor. Kept as its own variant rather than folded into
    /// `Write`, because an I/O failure here means the machine ran out of memory and that is a
    /// different conversation from a malformed archive.
    #[error("writing the container failed: {0}")]
    Io(#[from] std::io::Error),
}

/// Serialise the container.
///
/// The `mimetype` entry goes first and stored; every other entry is deflated and they are
/// written in ascending path order. Order is not cosmetic: it is half of what makes two builds
/// of one book the same bytes, and a zip whose central directory lists entries in a different
/// order every run cannot be diffed against a golden file.
pub fn write_deterministic_zip(entries: &[ZipEntry]) -> Result<Vec<u8>, ZipError> {
    let mimetype = entries
        .iter()
        .find(|entry| entry.path == MIMETYPE_PATH)
        .ok_or(ZipError::NoMimetype)?;

    let mut rest: Vec<&ZipEntry> = entries
        .iter()
        .filter(|entry| entry.path != MIMETYPE_PATH)
        .collect();
    rest.sort_by(|a, b| a.path.cmp(&b.path));

    // A container that holds `OEBPS/Ch1.xhtml` and `OEBPS/ch1.xhtml` unpacks to one file on
    // Windows and macOS and to two on Linux, so one of the two books silently loses a chapter.
    // Checked here rather than only in the validator because this is where the names are all
    // in one place (D6).
    for pair in rest.windows(2) {
        let (a, b) = (&pair[0].path, &pair[1].path);
        if a.to_lowercase() == b.to_lowercase() {
            return Err(ZipError::CaseCollision(a.clone(), b.clone()));
        }
    }

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));

    writer.start_file(MIMETYPE_PATH, options(CompressionMethod::Stored))?;
    writer.write_all(&mimetype.data)?;

    for entry in rest {
        writer.start_file(&entry.path, options(CompressionMethod::Deflated))?;
        writer.write_all(&entry.data)?;
    }

    Ok(writer.finish()?.into_inner())
}

/// The options every entry is written with, differing only in the compression method.
///
/// `last_modified_time` is the earliest instant the MS-DOS timestamp format can express —
/// 1980-01-01T00:00:00 — because there is no "no timestamp" to choose, and no permissions are
/// set, because a mode bit is where the writing platform leaks into the bytes.
fn options(method: CompressionMethod) -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(method)
        .last_modified_time(epoch())
        .large_file(false)
}

/// 1980-01-01T00:00:00, the zero of the MS-DOS timestamp.
fn epoch() -> DateTime {
    DateTime::from_date_and_time(1980, 1, 1, 0, 0, 0).unwrap_or_default()
}

/// `META-INF/container.xml`, pointing at the package document.
pub fn container_xml(opf_path: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\n\
         <rootfiles>\n\
         <rootfile full-path=\"{}\" media-type=\"application/oebps-package+xml\"/>\n\
         </rootfiles>\n\
         </container>\n",
        crate::xhtml::escape::attribute(opf_path)
    )
}

// ---------------------------------------------------------------------------
// Tests (written first — IMPLEMENTATION_PLAN §0.2). Row 5.3.
// ---------------------------------------------------------------------------

#[cfg(test)]
fn sample() -> Vec<ZipEntry> {
    vec![
        ZipEntry::new("OEBPS/ch2.xhtml", b"<html>two</html>".to_vec()),
        ZipEntry::new(MIMETYPE_PATH, EPUB_MEDIA_TYPE.as_bytes().to_vec()),
        ZipEntry::new("OEBPS/ch1.xhtml", b"<html>one</html>".to_vec()),
        ZipEntry::new(
            CONTAINER_PATH,
            container_xml("OEBPS/content.opf").into_bytes(),
        ),
    ]
}

/// Row 5.3. The one thing a reader may do without parsing the archive is read bytes 30..38 and
/// find `mimetype` there, uncompressed. PKG-007 is what happens when it cannot.
#[test]
fn zip_mimetype_is_first_and_stored() {
    let bytes = write_deterministic_zip(&sample()).expect("the container writes");

    assert_eq!(
        &bytes[0..4],
        b"PK\x03\x04",
        "a local file header opens the file"
    );
    assert_eq!(
        &bytes[MIMETYPE_NAME_OFFSET..MIMETYPE_NAME_OFFSET + MIMETYPE_PATH.len()],
        MIMETYPE_PATH.as_bytes(),
        "the first entry is named `mimetype` at offset 30, so the extra field is empty"
    );

    // Compression method is the little-endian u16 at offset 8 of the local file header.
    let method = u16::from_le_bytes([bytes[8], bytes[9]]);
    assert_eq!(method, 0, "stored, not deflated");

    // And the media type itself follows the header directly, uncompressed.
    let start = MIMETYPE_NAME_OFFSET + MIMETYPE_PATH.len();
    assert_eq!(
        &bytes[start..start + EPUB_MEDIA_TYPE.len()],
        EPUB_MEDIA_TYPE.as_bytes()
    );

    // No extra field and no general-purpose bit 3, which is what a data descriptor would set.
    assert_eq!(
        u16::from_le_bytes([bytes[28], bytes[29]]),
        0,
        "no extra field"
    );
    assert_eq!(
        u16::from_le_bytes([bytes[6], bytes[7]]) & 0b1000,
        0,
        "no data descriptor"
    );
}

/// Byte identity is the property that makes a golden-byte test and a cross-OS CI gate
/// possible at all, and the thing most likely to break it is an entry order that depends on a
/// hash map's iteration.
#[test]
fn the_same_entries_in_any_order_produce_the_same_bytes() {
    let forwards = write_deterministic_zip(&sample()).expect("writes");
    let mut shuffled = sample();
    shuffled.reverse();
    let backwards = write_deterministic_zip(&shuffled).expect("writes");

    assert_eq!(forwards, backwards, "entry order is the path order, always");
    assert_eq!(
        forwards,
        write_deterministic_zip(&sample()).expect("writes"),
        "and two runs of the same input agree"
    );
}

/// The container is a real zip that a reader can open, listing exactly what was put in it and
/// in the order OCF requires.
#[test]
fn the_container_reads_back_as_the_entries_it_was_given() {
    let bytes = write_deterministic_zip(&sample()).expect("writes");
    let archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("the container opens");

    assert_eq!(
        archive.file_names().collect::<Vec<_>>(),
        vec![
            MIMETYPE_PATH,
            CONTAINER_PATH,
            "OEBPS/ch1.xhtml",
            "OEBPS/ch2.xhtml",
        ]
    );
}

/// Two names that differ only in case unpack to one file on Windows and macOS. One of the two
/// chapters then silently does not exist, which no reading system reports.
#[test]
fn entry_names_that_collide_case_insensitively_are_refused() {
    let mut entries = sample();
    entries.push(ZipEntry::new("OEBPS/CH1.xhtml", b"shadow".to_vec()));

    let refused = write_deterministic_zip(&entries).expect_err("the collision is refused");
    assert!(
        matches!(refused, ZipError::CaseCollision(_, _)),
        "{refused}"
    );
}
