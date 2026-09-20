"""A tagged PDF's own structure tree as ground truth.

TEST_CORPUS §5.4. A PDF/UA-conformant file embeds a `/StructTreeRoot` whose `/S` names each
part — `/H1`, `/P`, `/Figure`, `/Note` — and whose `/K` gives the order. That is ground truth
with no external source document at all, and 24 of the frozen holdout's documents carry one.

**It states structure, not text.** The tree points at marked content by MCID; recovering the
characters behind an MCID means walking the content stream and matching `BDC`/`EMC` pairs,
which is the extractor's job and not a ground truth's. So a heading found here has a level and
no text, and `schema.to_assertions` knows not to invent one — the assertions this source can
justify are the counts and the image parity, which are exactly the checks §5.3 says
page-level and structural ground truth is *for*.

Tagging quality in the wild is inconsistent (§5.4), so a tree that claims nothing is reported
as claiming nothing rather than as a document with no structure.
"""

from __future__ import annotations

from collections import Counter
from pathlib import Path
from typing import Any

import pikepdf

from oc_eval.ground_truth.schema import Figure, GroundTruth, Heading

# `/S` values, as PDF 32000-1 §14.8.4 names them.
HEADING_TYPES = {f"/H{level}": level for level in range(1, 7)}
# `/H` is a heading of unspecified level. Treated as level 1: it is the only level a file that
# uses `/H` ever means, and guessing deeper would invent a hierarchy the file does not state.
UNNUMBERED_HEADING = "/H"
PARAGRAPH_TYPES = {"/P"}
FIGURE_TYPES = {"/Figure"}
NOTE_TYPES = {"/Note", "/FENote"}

# A structure tree is a tree over a document, so its depth is bounded by its nesting. The cap
# is here because `/K` in a file anyone can write may point back up it.
MAX_DEPTH = 64


class NoStructTree(ValueError):
    """The document carries no `/StructTreeRoot`, so it has nothing to say about itself."""


def ground_truth(path: Path, *, title: str | None = None) -> GroundTruth:
    with pikepdf.open(path) as pdf:
        root = pdf.Root.get("/StructTreeRoot")
        if root is None:
            raise NoStructTree(f"{path.name} has no /StructTreeRoot")

        elements: list[tuple[int, str, str]] = []
        _walk(root.get("/K"), 0, elements)

        headings = tuple(
            Heading(level=level, text=text, id=None)
            for level, text in (
                (_heading_level(kind), text) for _, kind, text in elements if _is_heading(kind)
            )
            if level is not None
        )
        paragraphs = tuple(text for _, kind, text in elements if kind in PARAGRAPH_TYPES and text)
        figures = tuple(
            Figure(alt=text, src="", caption=None)
            for _, kind, text in elements
            if kind in FIGURE_TYPES
        )

        # A `/P` with no recoverable text is still a paragraph the tree counted. The count is
        # what this source can attest, so it is carried as an empty string rather than lost.
        paragraph_count = sum(1 for _, kind, _ in elements if kind in PARAGRAPH_TYPES)
        if len(paragraphs) < paragraph_count:
            paragraphs = paragraphs + ("",) * (paragraph_count - len(paragraphs))

        return GroundTruth(
            title=title or path.stem,
            headings=headings,
            paragraphs=paragraphs,
            footnotes=(),
            figures=figures,
            languages=_language(pdf),
        )


def structure_counts(path: Path) -> dict[str, int]:
    """How many of each `/S` the tree holds. The shape of a document, before any scoring."""
    with pikepdf.open(path) as pdf:
        root = pdf.Root.get("/StructTreeRoot")
        if root is None:
            raise NoStructTree(f"{path.name} has no /StructTreeRoot")
        elements: list[tuple[int, str, str]] = []
        _walk(root.get("/K"), 0, elements)
        return dict(sorted(Counter(kind for _, kind, _ in elements).items()))


def _walk(node: Any, depth: int, out: list[tuple[int, str, str]]) -> None:
    if depth > MAX_DEPTH:
        return
    if isinstance(node, pikepdf.Array):
        for item in node:
            _walk(item, depth, out)
        return
    if not isinstance(node, pikepdf.Dictionary):
        # An integer here is an MCID: a leaf pointing into a content stream, which is the
        # extractor's territory and not a structure element.
        return

    kind = str(node.get("/S", "")) if "/S" in node else ""
    child_depth = depth
    if kind:
        out.append((depth, kind, _text_of(node)))
        child_depth = depth + 1
    if "/K" in node:
        _walk(node.get("/K"), child_depth, out)


def _text_of(node: pikepdf.Dictionary) -> str:
    """`/ActualText` or `/Alt` if the producer wrote one. Most do not."""
    for key in ("/ActualText", "/Alt"):
        value = node.get(key)
        if value is not None:
            return " ".join(str(value).split())
    return ""


def _is_heading(kind: str) -> bool:
    return kind in HEADING_TYPES or kind == UNNUMBERED_HEADING


def _heading_level(kind: str) -> int | None:
    if kind == UNNUMBERED_HEADING:
        return 1
    return HEADING_TYPES.get(kind)


def _language(pdf: pikepdf.Pdf) -> tuple[str, ...]:
    value = pdf.Root.get("/Lang")
    if value is None:
        return ()
    tag = str(value).split("-")[0].lower()
    return (tag,) if tag else ()


def note_count(path: Path) -> int:
    """How many `/Note` elements the tree holds — the footnote count, where it is tagged."""
    counts = structure_counts(path)
    return sum(counts.get(kind, 0) for kind in NOTE_TYPES)
