//! Hostile PDFs, built byte by byte: each one declares something a cap exists to refuse (PHASE 14).
//!
//! Written by hand rather than through `lopdf` or `pdf-writer`, because the point of each file is a
//! lie those libraries would not tell — a `/Count` the tree does not hold, a `/Prev` that loops, a
//! `/Width` no decoder should believe. Every file is otherwise well formed, so what refuses it is
//! the cap and not a parse error.

use std::io::Write;

/// A PDF from numbered object bodies (`1 0 obj` is the first) and a trailer's extra entries.
/// Object 1 must be the catalogue. Offsets are exact; there is one classic xref section.
pub fn assemble(objects: &[Vec<u8>], trailer_extra: &str) -> Vec<u8> {
    let mut pdf = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in &offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R{trailer_extra} >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

/// A stream object's body: dictionary entries, then the data, with an honest `/Length`.
pub fn stream(entries: &str, data: &[u8]) -> Vec<u8> {
    let mut body = format!("<< {entries} /Length {} >>\nstream\n", data.len()).into_bytes();
    body.extend_from_slice(data);
    body.extend_from_slice(b"\nendstream");
    body
}

/// zlib at the fastest level: the hostile files are about what the data expands to, not how
/// small it is.
pub fn zlib(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    // Writing to a Vec cannot fail.
    let _ = encoder.write_all(data);
    encoder.finish().unwrap_or_default()
}

/// zlib of `size` zero bytes, compressed in chunks so the input is never held whole.
pub fn zlib_zeros(size: u64) -> Vec<u8> {
    let chunk = vec![0_u8; 1 << 20];
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    let mut written = 0_u64;
    while written < size {
        let take = usize::try_from((size - written).min(chunk.len() as u64)).unwrap_or(0);
        let _ = encoder.write_all(&chunk[..take]);
        written += take as u64;
    }
    encoder.finish().unwrap_or_default()
}

const PAGE: &str = "/Type /Page /Parent 2 0 R /MediaBox [0 0 612 792]";
const FONT: &str = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>";

/// One real page whose tree declares `count` pages (row 14.6).
pub fn declared_pages(count: u64) -> Vec<u8> {
    assemble(
        &[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            format!("<< /Type /Pages /Count {count} /Kids [3 0 R] >>").into_bytes(),
            format!("<< {PAGE} /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>")
                .into_bytes(),
            stream("", b"BT /F1 12 Tf 72 700 Td (One page.) Tj ET"),
            FONT.as_bytes().to_vec(),
        ],
        "",
    )
}

/// A page painting one image whose dictionary declares `width` × `height` pixels and carries a
/// handful of bytes (A14.1: refused from the dictionary).
pub fn pixel_claim(width: u64, height: u64) -> Vec<u8> {
    assemble(
        &[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_vec(),
            format!("<< {PAGE} /Contents 4 0 R /Resources << /XObject << /Im0 5 0 R >> >> >>")
                .into_bytes(),
            stream("", b"q 500 0 0 500 50 150 cm /Im0 Do Q"),
            stream(
                &format!(
                    "/Type /XObject /Subtype /Image /Width {width} /Height {height} \
                     /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode"
                ),
                &zlib(&[128_u8; 64]),
            ),
        ],
        "",
    )
}

/// `pages` pages, each drawing `glyphs` one-glyph `Tj` operators (row 14.9: "a 3-page PDF
/// declaring 40 million glyphs"). The content compresses about a thousand to one.
pub fn glyph_flood(pages: usize, glyphs: usize) -> Vec<u8> {
    let content = {
        let mut content = b"BT /F1 1 Tf 10 700 Td ".to_vec();
        content.reserve(glyphs * b"(a) Tj ".len());
        for _ in 0..glyphs {
            content.extend_from_slice(b"(a) Tj ");
        }
        content.extend_from_slice(b"ET");
        zlib(&content)
    };
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        Vec::new(),
        FONT.as_bytes().to_vec(),
    ];
    let mut kids = Vec::new();
    for _ in 0..pages {
        let page_number = objects.len() + 1;
        kids.push(format!("{page_number} 0 R"));
        objects.push(
            format!(
                "<< {PAGE} /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> >> >>",
                page_number + 1
            )
            .into_bytes(),
        );
        objects.push(stream("/Filter /FlateDecode", &content));
    }
    objects[1] = format!(
        "<< /Type /Pages /Count {pages} /Kids [{}] >>",
        kids.join(" ")
    )
    .into_bytes();
    assemble(&objects, "")
}

/// One page whose content stream is a zlib bomb of `expanded` zero bytes (A14.2).
pub fn decompression_bomb(bomb: &[u8]) -> Vec<u8> {
    assemble(
        &[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_vec(),
            format!("<< {PAGE} /Contents 4 0 R >>").into_bytes(),
            stream("/Filter /FlateDecode", bomb),
        ],
        "",
    )
}

/// A one-page file whose xref chain is `sections` sections deep, every update re-listing the
/// objects; with `self_loop`, the newest section's `/Prev` points at itself (rows 14.4, 14.5).
pub fn xref_chain(sections: usize, self_loop: bool) -> Vec<u8> {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>",
    ];
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (number, body) in bodies.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", number + 1).as_bytes());
    }
    let mut sections_at: Vec<usize> = Vec::new();
    for index in 0..sections.max(1) {
        let at = pdf.len();
        pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
        for offset in &offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        let last = index + 1 == sections.max(1);
        let prev = if last && self_loop {
            Some(at)
        } else {
            sections_at.last().copied()
        };
        let prev = prev
            .map(|prev| format!(" /Prev {prev}"))
            .unwrap_or_default();
        pdf.extend_from_slice(format!("trailer\n<< /Size 4 /Root 1 0 R{prev} >>\n").as_bytes());
        sections_at.push(at);
    }
    let last = sections_at.last().copied().unwrap_or_default();
    pdf.extend_from_slice(format!("startxref\n{last}\n%%EOF\n").as_bytes());
    pdf
}
