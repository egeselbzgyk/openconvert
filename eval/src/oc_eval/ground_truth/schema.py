"""What a document's true structure is, and how it becomes assertions.

TEST_CORPUS §5.1's shape: headings, paragraphs, footnote pairs, figures. One type, whatever
the source — Standard Ebooks XHTML, a tagged PDF's struct tree, or arXiv LaTeX — so that the
scorer has one thing to compare against and adding a fourth source does not touch it.

The assertions are in the vocabulary `oc_testkit::assertions` reads and the committed
`.assert.json` fixtures are written in. That is deliberate: a generated expectation and a
hand-written one should be checkable by the same runner, and a vocabulary that exists twice is
a vocabulary that disagrees with itself the first time somebody extends one copy.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

# The block kinds `block_count` counts. `oc_testkit::assertions` looks them up by name in the
# converted document's own `block_counts` map.
BLOCK_PARAGRAPH = "paragraph"
BLOCK_FIGURE = "figure"

# How much of a paragraph a `text_present` assertion quotes. Long enough to be unique in a
# book, short enough that one dehyphenation or one curly quote does not make it a false alarm.
QUOTE_WORDS = 8


@dataclass(frozen=True)
class Heading:
    level: int
    text: str
    id: str | None = None


@dataclass(frozen=True)
class Footnote:
    """A noteref and the note it resolves to — D6's bijection, as the source states it."""

    marker_id: str
    note_id: str
    body: str
    referring_paragraph: int | None = None


@dataclass(frozen=True)
class Figure:
    alt: str
    src: str
    caption: str | None = None


@dataclass(frozen=True)
class GroundTruth:
    title: str
    headings: tuple[Heading, ...] = ()
    paragraphs: tuple[str, ...] = ()
    footnotes: tuple[Footnote, ...] = ()
    figures: tuple[Figure, ...] = ()
    languages: tuple[str, ...] = ()


def to_json(truth: GroundTruth) -> dict[str, Any]:
    return {
        "title": truth.title,
        "headings": [{"level": h.level, "text": h.text, "id": h.id} for h in truth.headings],
        "paragraphs": list(truth.paragraphs),
        "footnotes": [
            {
                "marker_id": n.marker_id,
                "note_id": n.note_id,
                "body": n.body,
                "referring_paragraph": n.referring_paragraph,
            }
            for n in truth.footnotes
        ],
        "figures": [{"alt": f.alt, "src": f.src, "caption": f.caption} for f in truth.figures],
        "languages": list(truth.languages),
    }


def from_json(payload: dict[str, Any]) -> GroundTruth:
    return GroundTruth(
        title=str(payload.get("title", "")),
        headings=tuple(
            Heading(int(h["level"]), str(h["text"]), h.get("id"))
            for h in payload.get("headings", [])
        ),
        paragraphs=tuple(str(p) for p in payload.get("paragraphs", [])),
        footnotes=tuple(
            Footnote(
                str(n["marker_id"]),
                str(n["note_id"]),
                str(n.get("body", "")),
                n.get("referring_paragraph"),
            )
            for n in payload.get("footnotes", [])
        ),
        figures=tuple(
            Figure(str(f.get("alt", "")), str(f.get("src", "")), f.get("caption"))
            for f in payload.get("figures", [])
        ),
        languages=tuple(str(tag) for tag in payload.get("languages", [])),
    )


def to_assertions(truth: GroundTruth) -> list[dict[str, Any]]:
    """The ground truth restated as things that must be true of the converted book.

    Only what the source actually says. A ground truth that guesses scores a wrong answer,
    which is worse than one that is short — so a document with no figures asserts zero images
    and a document whose notes do not pair asserts nothing about the bijection at all.
    """
    assertions: list[dict[str, Any]] = []

    # A heading whose text the source does not carry — a tagged PDF's `/H1` points at marked
    # content, not at characters — is a heading this ground truth counts and cannot quote.
    # Asserting on an empty string would score every document as wrong.
    named = [heading for heading in truth.headings if heading.text]
    if named and len(named) == len(truth.headings):
        assertions.append(
            {
                "kind": "heading_tree",
                "headings": [[h.level, h.text] for h in named],
                "note": "the heading tree the source markup states",
            }
        )
    for heading in named:
        assertions.append({"kind": "heading_level", "text": heading.text, "level": heading.level})

    for paragraph in truth.paragraphs:
        quote = _quote(paragraph)
        if quote:
            assertions.append({"kind": "text_present", "text": quote})

    if len(truth.paragraphs) >= 2:
        first, second = _quote(truth.paragraphs[0]), _quote(truth.paragraphs[1])
        if first and second and first != second:
            assertions.append(
                {
                    "kind": "text_order",
                    "first": first,
                    "then": second,
                    "note": "reading order, as the source has it",
                }
            )

    assertions.append(
        {
            "kind": "block_count",
            "of": BLOCK_PARAGRAPH,
            "min": len(truth.paragraphs),
            "max": len(truth.paragraphs),
        }
    )
    if truth.figures:
        assertions.append(
            {
                "kind": "block_count",
                "of": BLOCK_FIGURE,
                "min": len(truth.figures),
                "max": len(truth.figures),
            }
        )
    assertions.append({"kind": "image_count", "equals": len(truth.figures)})

    if truth.footnotes and all(note.body for note in truth.footnotes):
        assertions.append(
            {
                "kind": "note_bijection",
                "holds": True,
                "note": f"{len(truth.footnotes)} noteref/endnote pairs in the source",
            }
        )

    for tag in truth.languages:
        quote = _quote(truth.paragraphs[0]) if truth.paragraphs else ""
        if quote:
            assertions.append({"kind": "lang_tag", "text": quote, "lang": tag})

    return assertions


def _quote(paragraph: str) -> str:
    """The first few words of a paragraph, as a `text_present` needle.

    Whole paragraphs are not used: dehyphenation, ligature expansion and quote folding all
    change characters the pipeline is *supposed* to change (D13.4's `N`), so an assertion on
    a whole paragraph fails for being right.
    """
    words = paragraph.split()
    return " ".join(words[:QUOTE_WORDS])
