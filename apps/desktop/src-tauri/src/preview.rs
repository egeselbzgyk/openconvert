//! The preview: the generated XHTML, rendered by the webview in a sandboxed frame (Phase 12 detail
//! 7, UI_UX §2.3).
//!
//! The finished EPUB is served, file by file, from a custom protocol (`ocpreview`) the app
//! registers — `ocpreview://localhost/<job>/<path in the EPUB>` — so the book is shown by the
//! same engine a reader's own OS provides and nothing is converted twice. The frame is
//! `sandbox`ed in the UI (no scripts, an opaque origin, no IPC); what this module serves carries
//! its own CSP that allows no script at all and nothing from outside the book. It is labelled
//! "approximate" and never used as a check (D7).
//!
//! The webview names the job and a path inside its EPUB; the EPUB itself is the one the queue
//! recorded for that job, and an entry name is served only if the archive has exactly that entry.

use std::io::Read;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::Reader;
use serde::Serialize;

use crate::engine::UiError;

/// One chapter in the navigation document, at its nesting level (1 = top).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Chapter {
    pub title: String,
    pub href: String,
    pub level: u32,
}

/// One entry of the page list: the printed page label and where it is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PageMark {
    pub label: String,
    pub href: String,
}

/// What the preview needs to navigate a book.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PreviewIndex {
    pub chapters: Vec<Chapter>,
    pub pages: Vec<PageMark>,
    /// The book's language, for the frame's `lang`.
    pub lang: String,
}

/// The name of the stylesheet the preview adds: `:target` highlighting, in system colours.
pub const PREVIEW_CSS: &str = "__oc_preview.css";

/// The stylesheet itself. System colour keywords only: the page keeps the book's own colours, and
/// the jumped-to block is marked the way the platform marks a find-in-page hit.
const PREVIEW_CSS_BODY: &str = ":target { background: Mark; color: MarkText; }\n";

/// The policy every served file carries: nothing but the book's own files, and no script.
pub const SERVED_CSP: &str =
    "default-src 'none'; img-src ocpreview: http://ocpreview.localhost data:; style-src ocpreview: http://ocpreview.localhost; font-src ocpreview: http://ocpreview.localhost";

/// Read the navigation document of `epub`.
pub fn index(epub: &Path) -> Result<PreviewIndex, UiError> {
    let nav = entry_bytes(epub, "nav.xhtml")?;
    let text = String::from_utf8_lossy(&nav);
    parse_nav(&text)
}

/// Parse `nav.xhtml`: the `toc` nav's links with their nesting, and the `page-list` nav's.
pub fn parse_nav(markup: &str) -> Result<PreviewIndex, UiError> {
    let mut reader = Reader::from_str(markup);
    let mut index = PreviewIndex::default();
    let mut section: Option<&'static str> = None;
    let mut depth: u32 = 0;
    let mut link: Option<(String, String)> = None;

    loop {
        let event = reader
            .read_event()
            .map_err(|error| UiError::Io(format!("nav.xhtml: {error}")))?;
        match event {
            Event::Start(tag) => {
                let name = tag.local_name().as_ref().to_owned();
                match name.as_str() {
                    "html" => {
                        for attribute in tag.attributes().flatten() {
                            if attribute.key.local_name().as_ref() == "lang" {
                                index.lang = attribute.value.to_string();
                            }
                        }
                    }
                    "nav" => {
                        section = tag.attributes().flatten().find_map(|attribute| {
                            (attribute.key.local_name().as_ref() == "type").then(|| match attribute
                                .value
                                .as_ref()
                            {
                                "toc" => "toc",
                                "page-list" => "page-list",
                                _ => "other",
                            })
                        });
                    }
                    "ol" if section.is_some() => depth += 1,
                    "a" if section.is_some() => {
                        let href = tag
                            .attributes()
                            .flatten()
                            .find(|attribute| attribute.key.as_ref() == "href")
                            .map(|attribute| attribute.value.to_string())
                            .unwrap_or_default();
                        link = Some((href, String::new()));
                    }
                    _ => {}
                }
            }
            Event::Text(text) => {
                if let Some((_, title)) = link.as_mut() {
                    title.push_str(text.as_ref());
                }
            }
            Event::GeneralRef(reference) => {
                // `&amp;` and friends inside a link title.
                if let Some((_, title)) = link.as_mut() {
                    if let Ok(Some(ch)) = reference.resolve_char_ref() {
                        title.push(ch);
                    } else {
                        title.push_str(match reference.as_ref() {
                            "amp" => "&",
                            "lt" => "<",
                            "gt" => ">",
                            "quot" => "\"",
                            "apos" => "'",
                            _ => "",
                        });
                    }
                }
            }
            Event::End(tag) => {
                let name = tag.local_name().as_ref().to_owned();
                match name.as_str() {
                    "nav" => {
                        section = None;
                        depth = 0;
                    }
                    "ol" if section.is_some() => depth = depth.saturating_sub(1),
                    "a" => {
                        if let Some((href, title)) = link.take() {
                            let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
                            match section {
                                Some("toc") => index.chapters.push(Chapter {
                                    title,
                                    href,
                                    level: depth,
                                }),
                                Some("page-list") => {
                                    index.pages.push(PageMark { label: title, href })
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(index)
}

/// One file of the EPUB, with its media type, ready to serve. The preview stylesheet is added to
/// every XHTML document's head.
pub fn serve(epub: &Path, name: &str) -> Result<(Vec<u8>, &'static str), UiError> {
    if name == PREVIEW_CSS {
        return Ok((PREVIEW_CSS_BODY.as_bytes().to_vec(), "text/css"));
    }
    let bytes = entry_bytes(epub, name)?;
    let media = media_type(name);
    if media == "application/xhtml+xml" {
        let depth = name.matches('/').count();
        let link = format!(
            "<link rel=\"stylesheet\" type=\"text/css\" href=\"{}{PREVIEW_CSS}\"/></head>",
            "../".repeat(depth)
        );
        let text = String::from_utf8_lossy(&bytes).replacen("</head>", &link, 1);
        return Ok((text.into_bytes(), media));
    }
    Ok((bytes, media))
}

/// An entry of the archive, by its exact name. Anything that could step outside the archive's own
/// namespace is refused before the archive is opened.
fn entry_bytes(epub: &Path, name: &str) -> Result<Vec<u8>, UiError> {
    let unsafe_name = name.is_empty()
        || name.starts_with('/')
        || name.contains('\\')
        || name.split('/').any(|part| part == ".." || part == ".");
    if unsafe_name {
        return Err(UiError::Io(format!("not an entry name: {name}")));
    }
    let file = std::fs::File::open(epub)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| UiError::Io(error.to_string()))?;
    let mut entry = archive
        .by_name(name)
        .map_err(|error| UiError::Io(format!("{name}: {error}")))?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// The media type of a file the preview may serve; anything else is served as opaque bytes.
fn media_type(name: &str) -> &'static str {
    let extension = name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "xhtml" | "html" => "application/xhtml+xml",
        "css" => "text/css",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    const NAV: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" xml:lang="de" lang="de">
<head><title>T</title></head><body>
<nav epub:type="toc" id="toc"><h1>Contents</h1><ol>
<li><a href="text/c0001.xhtml#h0">Vorwort</a></li>
<li><a href="text/c0002.xhtml#h1">Kapitel &amp; Eins</a><ol><li><a href="text/c0002.xhtml#h2">Ein Abschnitt</a></li></ol></li>
</ol></nav>
<nav epub:type="landmarks" hidden="hidden"><ol><li><a href="text/c0002.xhtml#h1">Kapitel</a></li></ol></nav>
<nav epub:type="page-list" hidden="hidden"><ol><li><a href="text/c0001.xhtml#page0">i</a></li><li><a href="text/c0002.xhtml#page1">1</a></li></ol></nav>
</body></html>"#;

    fn epub(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("book.epub");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).expect("created"));
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, body) in [
            ("nav.xhtml", NAV),
            (
                "text/c0001.xhtml",
                "<html><head><title>1</title></head><body><p id=\"page0\">i</p></body></html>",
            ),
            ("style.css", "p { margin: 0 }"),
        ] {
            zip.start_file(name, stored).expect("entry");
            zip.write_all(body.as_bytes()).expect("written");
        }
        zip.finish().expect("finished");
        path
    }

    #[test]
    fn the_nav_gives_chapters_with_levels_and_the_page_list() {
        let index = parse_nav(NAV).expect("parses");
        assert_eq!(index.lang, "de");
        assert_eq!(
            index
                .chapters
                .iter()
                .map(|c| (c.title.as_str(), c.level))
                .collect::<Vec<_>>(),
            [("Vorwort", 1), ("Kapitel & Eins", 1), ("Ein Abschnitt", 2)]
        );
        assert_eq!(index.pages.len(), 2, "landmarks are not pages");
        assert_eq!(index.pages[0].label, "i");
        assert_eq!(index.pages[1].href, "text/c0002.xhtml#page1");
    }

    #[test]
    fn only_the_archives_own_entries_are_served() {
        let dir = std::env::temp_dir().join(format!("oc-desktop-preview-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("made");
        let book = epub(&dir);

        let (chapter, media) = serve(&book, "text/c0001.xhtml").expect("served");
        assert_eq!(media, "application/xhtml+xml");
        let chapter = String::from_utf8(chapter).expect("UTF-8");
        assert!(
            chapter.contains("href=\"../__oc_preview.css\""),
            "{chapter}"
        );

        assert_eq!(serve(&book, "style.css").expect("served").1, "text/css");
        for refused in [
            "../book.epub",
            "/etc/passwd",
            "text/../nav.xhtml",
            "",
            "missing.xhtml",
        ] {
            assert!(
                serve(&book, refused).is_err(),
                "{refused:?} must not be served"
            );
        }
        assert_eq!(index(&book).expect("indexed").chapters.len(), 3);
    }
}
