"""Standard Ebooks XHTML to ground truth.

TEST_CORPUS §5.1. SE markup is CC0 and says what its parts are: `<h1>`–`<h6>` at real levels,
`epub:type` telling a chapter from front matter, `<a epub:type="noteref">` paired by id with an
`<li epub:type="endnote">`, `<figure>` with `<figcaption>`. That is a ground-truth factory at
no authoring cost, and the source — never the PDF rendered from it — is what a score is taken
against, so the measurement is "did the pipeline recover the structure" rather than "did it
agree with a renderer".

Heading levels are normalised to depth rather than kept as the tag number. SE writes the
chapter title as `<h2>` inside a `<section>` because the book's `<h1>` is its title page, and a
converted EPUB's own outline starts at the chapter. Comparing `h2` against `h1` would score a
correct conversion as wrong.
"""

from __future__ import annotations

from collections.abc import Iterable, Sequence
from pathlib import Path

from lxml import etree

from oc_eval.ground_truth.schema import Figure, Footnote, GroundTruth, Heading

XHTML = "http://www.w3.org/1999/xhtml"
EPUB = "http://www.idpf.org/2007/ops"
NS = {"x": XHTML, "epub": EPUB}

HEADING_TAGS = tuple(f"h{level}" for level in range(1, 7))

# `epub:type` values that mark an element as a note or its reference.
NOTEREF_TYPE = "noteref"
ENDNOTE_TYPE = "endnote"

# The backlink inside an endnote is navigation, not the note's text.
BACKLINK_TYPE = "backlink"


class NoContent(ValueError):
    """The file parsed but held nothing a ground truth is made of."""


def ground_truth(paths: Sequence[Path], *, title: str) -> GroundTruth:
    """Read one book's XHTML files, in spine order, into a single ground truth."""
    headings: list[Heading] = []
    paragraphs: list[str] = []
    figures: list[Figure] = []
    noterefs: list[tuple[str, str, int]] = []
    notes: dict[str, str] = {}
    languages: list[str] = []

    for path in paths:
        root = _parse(path.read_bytes())
        _collect_language(root, languages)
        _collect(root, headings, paragraphs, figures, noterefs, notes)

    footnotes = tuple(
        Footnote(marker_id, note_id, notes[note_id], referring)
        for marker_id, note_id, referring in noterefs
        if note_id in notes
    )

    return GroundTruth(
        title=title,
        headings=tuple(headings),
        paragraphs=tuple(paragraphs),
        footnotes=footnotes,
        figures=tuple(figures),
        languages=tuple(dict.fromkeys(languages)),
    )


def parse_fragment(markup: str, *, base: str) -> GroundTruth:
    """A ground truth from a fragment of markup. For tests and for one-off inspection."""
    wrapped = f'<html xmlns="{XHTML}" xmlns:epub="{EPUB}"><body>{markup}</body></html>'
    root = _parse(wrapped.encode("utf-8"))

    headings: list[Heading] = []
    paragraphs: list[str] = []
    figures: list[Figure] = []
    noterefs: list[tuple[str, str, int]] = []
    notes: dict[str, str] = {}
    _collect(root, headings, paragraphs, figures, noterefs, notes)

    return GroundTruth(
        title=base,
        headings=tuple(headings),
        paragraphs=tuple(paragraphs),
        footnotes=tuple(
            Footnote(marker, note, notes[note], where)
            for marker, note, where in noterefs
            if note in notes
        ),
        figures=tuple(figures),
    )


def _parse(payload: bytes) -> etree._Element:
    # `resolve_entities=False` and `no_network=True`: an XHTML file is data, and an external
    # entity in one is a way to read the machine that parses it.
    parser = etree.XMLParser(resolve_entities=False, no_network=True, load_dtd=False)
    try:
        return etree.fromstring(payload, parser=parser)
    except etree.XMLSyntaxError as failure:
        raise NoContent(str(failure)) from failure


def _collect_language(root: etree._Element, languages: list[str]) -> None:
    tag = root.get("{http://www.w3.org/XML/1998/namespace}lang") or root.get("lang")
    if tag:
        languages.append(tag.split("-")[0].lower())


def _collect(
    root: etree._Element,
    headings: list[Heading],
    paragraphs: list[str],
    figures: list[Figure],
    noterefs: list[tuple[str, str, int]],
    notes: dict[str, str],
) -> None:
    for element in root.iter():
        local = etree.QName(element).localname if isinstance(element.tag, str) else ""

        if local in HEADING_TAGS:
            headings.append(
                Heading(level=_depth(element), text=_text_of(element), id=element.get("id"))
            )
        elif local == "figure":
            figures.append(_figure(element))
        elif local == "li" and _epub_type(element) == ENDNOTE_TYPE:
            ident = element.get("id")
            if ident:
                notes[ident] = _text_of(element, skip_epub_type=BACKLINK_TYPE)
        elif local == "p" and not _inside_note(element) and not _inside_figure(element):
            text = _text_of(element)
            if text:
                index = len(paragraphs)
                paragraphs.append(text)
                for ref in element.iter():
                    if _epub_type(ref) == NOTEREF_TYPE:
                        marker = ref.get("id") or ""
                        target = (ref.get("href") or "").rpartition("#")[2]
                        if marker and target:
                            noterefs.append((marker, target, index))


def _figure(element: etree._Element) -> Figure:
    alt = ""
    src = ""
    caption: str | None = None
    for child in element.iter():
        local = etree.QName(child).localname if isinstance(child.tag, str) else ""
        if local == "img":
            alt = child.get("alt") or ""
            src = child.get("src") or ""
        elif local == "figcaption":
            caption = _text_of(child) or None
    return Figure(alt=alt, src=src, caption=caption)


def _depth(element: etree._Element) -> int:
    """How deep a heading sits in the `<section>` nesting, counting from one.

    SE nests a subchapter's `<section>` inside its chapter's, so the nesting states the level
    the tag number only approximates.
    """
    depth = 0
    node = element.getparent()
    while node is not None:
        local = etree.QName(node).localname if isinstance(node.tag, str) else ""
        if local == "section":
            depth += 1
        node = node.getparent()
    return max(depth, 1)


def _epub_type(element: etree._Element) -> str:
    """The first `epub:type` token. SE writes several — `ordinal z3998:roman` — and the
    first is the structural one."""
    value = element.get(f"{{{EPUB}}}type")
    return value.split()[0] if value else ""


def _inside_note(element: etree._Element) -> bool:
    return _has_ancestor(element, lambda node: _epub_type(node) == ENDNOTE_TYPE)


def _inside_figure(element: etree._Element) -> bool:
    return _has_ancestor(
        element,
        lambda node: (etree.QName(node).localname if isinstance(node.tag, str) else "") == "figure",
    )


def _has_ancestor(element: etree._Element, matches: object) -> bool:
    node = element.getparent()
    while node is not None:
        if matches(node):  # type: ignore[operator]
            return True
        node = node.getparent()
    return False


def _text_of(element: etree._Element, *, skip_epub_type: str | None = None) -> str:
    """An element's text, whitespace collapsed, with navigation links left out.

    SE markup is tab-indented and line-wrapped, so the source's whitespace is layout and never
    content. The backlink at the end of an endnote is navigation for the same reason.
    """
    parts: Iterable[str] = (piece for piece in _walk_text(element, skip_epub_type) if piece)
    return " ".join("".join(parts).split())


def _walk_text(element: etree._Element, skip_epub_type: str | None) -> Iterable[str]:
    if skip_epub_type and _epub_type(element) == skip_epub_type:
        yield element.tail or ""
        return
    yield element.text or ""
    for child in element:
        if isinstance(child.tag, str):
            yield from _walk_text(child, skip_epub_type)
        else:
            yield child.tail or ""
    yield element.tail or ""
