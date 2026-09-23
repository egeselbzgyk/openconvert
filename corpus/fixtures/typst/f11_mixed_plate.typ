// A `mixed` page (D13.10): born-digital text above a scanned plate that no text overlaps.
//
// PHASE 13 row 13.14 reads it: OCR must touch only the plate — the one image region no text run
// covers — and leave the text above it exactly as extraction found it. The plate is the scan asset
// `f03` also uses, so it carries real pixels of text for a real Tesseract to read.
//
// The plate covers 128 mm x 150 mm of a 148 mm x 210 mm page, 61.8 % of it, which is over
// `pageclass.image_area_ratio_min` (0.60); the text is well over `pageclass.text_min_visible_chars`.
#set document(title: "A Mixed Page", author: ("O. Convert",), date: none)
#set page(width: 148mm, height: 210mm, margin: (x: 10mm, y: 10mm))
#set text(font: "Libertinus Serif", size: 10pt, lang: "en")
#set par(justify: true, leading: 0.65em)

#heading(level: 1)[Plate One]

The facing plate reproduces a page of the original edition, scanned as it was found. Its text is
pixels, and only OCR can read it.

#v(1fr)
#image("../assets/scan_page_01.png", width: 128mm, height: 150mm, fit: "stretch")
