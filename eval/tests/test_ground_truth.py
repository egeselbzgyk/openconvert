"""PHASE 7 row 7.7: Standard Ebooks XHTML is a ground-truth factory, and this is the mill.

TEST_CORPUS §5.1: SE markup is CC0 and semantically rich — real `<h1>`–`<h6>`, `epub:type`
distinguishing chapters from front matter, `<a epub:type="noteref">` paired with an endnote,
`<figure>`/`<figcaption>` — so the true structure of a document can be read straight out of
the source, rendered to PDF, and used to score what came back. The ground truth is the
*source*, never the intermediate PDF, so the score measures "did we recover the structure"
rather than "did we agree with a renderer".

The assertions the ground truth produces are in the same vocabulary the committed
`.assert.json` fixtures use and `oc_testkit::assertions` reads, because one runner checking
both is worth more than two that agree until they do not.
`test_every_generated_assertion_kind_is_one_the_engine_knows` holds those two in agreement by
reading the Rust enum rather than by keeping a second copy of the list.
"""

from __future__ import annotations

import json
import re
from pathlib import Path

import pytest

from oc_eval.ground_truth import from_xhtml, schema

REPO_ROOT = Path(__file__).resolve().parents[2]
SAMPLE = REPO_ROOT / "corpus" / "gt" / "se_sample"
ASSERTIONS_RS = REPO_ROOT / "crates" / "oc-testkit" / "src" / "assertions.rs"


def sample_ground_truth() -> schema.GroundTruth:
    return from_xhtml.ground_truth(
        [SAMPLE / "chapter-1.xhtml", SAMPLE / "endnotes.xhtml"], title="A Schedule of Rates"
    )


# --------------------------------------------------------------------------- row 7.7


def test_ground_truth_from_xhtml_roundtrips() -> None:
    """XHTML -> ground truth -> assertions, and each step keeps what the source said."""
    truth = sample_ground_truth()

    assert [(h.level, h.text) for h in truth.headings] == [
        (1, "I"),
        (2, "The Visitor"),
        (1, "Endnotes"),
    ]
    # Four in the chapter. The two inside the endnotes are the notes bodies, not prose.
    assert len(truth.paragraphs) == 4
    assert truth.paragraphs[0].startswith("The office was quiet at that hour")
    assert [(n.marker_id, n.note_id) for n in truth.footnotes] == [
        ("noteref-1", "note-1"),
        ("noteref-2", "note-2"),
    ]
    assert len(truth.figures) == 1
    assert truth.figures[0].alt.startswith("A ledger open on a desk")
    assert truth.figures[0].caption == "The ledger, as it stood on the evening of the fourth."

    round_tripped = schema.from_json(json.loads(json.dumps(schema.to_json(truth))))

    assert round_tripped == truth

    assertions = schema.to_assertions(truth)
    kinds = {a["kind"] for a in assertions}

    assert {"heading_tree", "text_present", "block_count", "image_count", "note_bijection"} <= kinds
    tree = next(a for a in assertions if a["kind"] == "heading_tree")
    assert tree["headings"] == [[1, "I"], [2, "The Visitor"], [1, "Endnotes"]]


def test_the_ground_truth_counts_blocks_the_way_the_assertion_vocabulary_does() -> None:
    truth = sample_ground_truth()

    counts = {a["of"]: a for a in schema.to_assertions(truth) if a["kind"] == "block_count"}

    assert counts["paragraph"]["min"] == counts["paragraph"]["max"] == len(truth.paragraphs)
    assert counts["figure"]["min"] == counts["figure"]["max"] == len(truth.figures)


def test_the_note_bijection_is_asserted_only_when_the_notes_actually_pair() -> None:
    """D6's bijection: every noteref resolves to a note and back. A dangling one is not it."""
    truth = sample_ground_truth()
    bijection = next(a for a in schema.to_assertions(truth) if a["kind"] == "note_bijection")

    assert bijection["holds"] is True

    broken = schema.GroundTruth(
        title=truth.title,
        headings=truth.headings,
        paragraphs=truth.paragraphs,
        footnotes=(schema.Footnote("noteref-9", "note-9", "", None),),
        figures=truth.figures,
        languages=truth.languages,
    )

    assert not any(a["kind"] == "note_bijection" for a in schema.to_assertions(broken))


def test_a_noteref_with_no_endnote_is_dropped_rather_than_invented() -> None:
    """A ground truth that guesses is worse than one that is short: it scores a wrong answer."""
    only_chapter = from_xhtml.ground_truth([SAMPLE / "chapter-1.xhtml"], title="Partial")

    assert only_chapter.footnotes == ()
    assert only_chapter.paragraphs, "the chapter's own paragraphs are still read"


def test_the_language_comes_from_the_document_rather_than_being_guessed() -> None:
    truth = sample_ground_truth()

    assert truth.languages == ("en",)
    assert any(a["kind"] == "lang_tag" and a["lang"] == "en" for a in schema.to_assertions(truth))


def test_a_figure_with_no_caption_still_yields_its_alt_text() -> None:
    truth = from_xhtml.parse_fragment(
        '<figure><img alt="A plain plate" src="x.jpg"/></figure>', base="t"
    )

    assert len(truth.figures) == 1
    assert truth.figures[0].caption is None
    assert truth.figures[0].alt == "A plain plate"


def test_whitespace_in_the_source_markup_never_reaches_the_ground_truth() -> None:
    """SE markup is tab-indented and line-wrapped; a paragraph is its words, not its layout."""
    truth = sample_ground_truth()

    for paragraph in truth.paragraphs:
        assert paragraph == " ".join(paragraph.split())
        assert not paragraph.startswith(" ")


# --------------------------------------------------------------------------- the vocabulary


def test_every_generated_assertion_kind_is_one_the_engine_knows() -> None:
    """The generated assertions and `oc_testkit::assertions` are one vocabulary, not two.

    Read out of the Rust source rather than duplicated here, for the same reason
    `xtask ci-lint` reads the warning registry out of the tree: a list written twice is a
    list that disagrees with itself the first time somebody adds to one copy.
    """
    source = ASSERTIONS_RS.read_text(encoding="utf-8")
    known = set(re.findall(r"^    ([A-Z][A-Za-z]+)\s*\{", source, flags=re.MULTILINE))
    snake = {re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower() for name in known}

    assert "text_present" in snake, "the Rust enum could not be read"

    produced = {a["kind"] for a in schema.to_assertions(sample_ground_truth())}

    assert produced <= snake, f"generated kinds the engine does not know: {produced - snake}"


def test_no_generated_assertion_carries_a_field_the_engine_would_reject() -> None:
    """`oc_testkit::assertions` is `deny_unknown_fields`, so a stray key is a hard error."""
    allowed = {
        "text_present": {"kind", "text", "note"},
        "heading_tree": {"kind", "headings", "note"},
        "heading_level": {"kind", "text", "level", "note"},
        "block_count": {"kind", "of", "min", "max"},
        "image_count": {"kind", "equals", "note"},
        "note_bijection": {"kind", "holds", "note"},
        "lang_tag": {"kind", "text", "lang", "note"},
        "text_order": {"kind", "first", "then", "note"},
    }

    for assertion in schema.to_assertions(sample_ground_truth()):
        kind = assertion["kind"]
        assert kind in allowed, f"no field list for {kind}"
        assert set(assertion) <= allowed[kind], (
            f"{kind} carries {set(assertion) - allowed[kind]}, which serde would reject"
        )


# --------------------------------------------------------------------------- tagged PDFs


def a_tagged_pdf() -> Path:
    """The Typst fixture compiled with its struct tree kept (D18's tagged bucket)."""
    import shutil
    import subprocess

    path = REPO_ROOT / "target" / "fixtures" / "f01_prose_single_column__tagged.pdf"
    if not path.exists():
        # Built rather than demanded: the tagged variant is git-ignored build output, and a
        # test that fails because an earlier task was not run is a test about the developer.
        cargo = shutil.which("cargo") or "cargo"
        subprocess.run(  # noqa: S603 - a fixed argv into this repository's own build tool
            [cargo, "run", "-q", "-p", "xtask", "--", "fixtures", "--keep-structtree"],
            cwd=REPO_ROOT,
            check=True,
        )
    return path


def test_a_tagged_pdf_states_its_own_structure() -> None:
    """TEST_CORPUS §5.4: a struct tree is ground truth with no external source document."""
    from oc_eval.ground_truth import from_structtree

    truth = from_structtree.ground_truth(a_tagged_pdf())

    assert [h.level for h in truth.headings] == [1]
    assert len(truth.paragraphs) >= 4
    assert truth.languages == ("en",)


def test_a_struct_tree_heading_carries_no_text_and_asserts_none() -> None:
    """The tree points at marked content, not at characters. Quoting it would invent text."""
    from oc_eval.ground_truth import from_structtree

    truth = from_structtree.ground_truth(a_tagged_pdf())
    assertions = schema.to_assertions(truth)

    assert all(h.text == "" for h in truth.headings)
    assert not any(a["kind"] in {"heading_tree", "heading_level"} for a in assertions)
    assert any(a["kind"] == "block_count" and a["of"] == "paragraph" for a in assertions)


def test_the_structure_counts_are_what_the_tree_says() -> None:
    from oc_eval.ground_truth import from_structtree

    counts = from_structtree.structure_counts(a_tagged_pdf())

    assert counts["/H1"] == 1
    assert counts["/P"] >= 4


def test_an_untagged_pdf_is_refused_rather_than_reported_as_structureless() -> None:
    """A file with no tree has said nothing about itself, which is not the same as 'no parts'."""
    from oc_eval.ground_truth import from_structtree

    untagged = REPO_ROOT / "corpus" / "fixtures" / "handmade" / "h01_two_glyphs.pdf"

    with pytest.raises(from_structtree.NoStructTree):
        from_structtree.ground_truth(untagged)


# --------------------------------------------------------------------------- arXiv LaTeX

PAPER = r"""
\documentclass{article}
\begin{document}
\section{Introduction}
Some prose with a note.\footnote{The note's text.}
% \section{A Commented-Out Section}
\subsection{Related \emph{Work}}
\begin{figure}
  \includegraphics[width=\linewidth]{plots/result.pdf}
  \caption{The result, at 95\% confidence.}
\end{figure}
\subsubsection{Deeper}
\end{document}
"""


def test_latex_sectioning_gives_the_heading_hierarchy() -> None:
    from oc_eval.ground_truth import from_latex

    truth = from_latex.ground_truth(PAPER, title="A Paper")

    assert [(h.level, h.text) for h in truth.headings] == [
        (1, "Introduction"),
        (2, "Related Work"),
        (3, "Deeper"),
    ]


def test_a_commented_out_section_is_not_a_section() -> None:
    from oc_eval.ground_truth import from_latex

    truth = from_latex.ground_truth(PAPER, title="A Paper")

    assert not any("Commented-Out" in h.text for h in truth.headings)


def test_latex_footnotes_and_figures_come_out_paired_with_their_captions() -> None:
    from oc_eval.ground_truth import from_latex

    truth = from_latex.ground_truth(PAPER, title="A Paper")

    assert [n.body for n in truth.footnotes] == ["The note's text."]
    assert len(truth.figures) == 1
    assert truth.figures[0].src == "plots/result.pdf"
    assert truth.figures[0].caption == "The result, at 95% confidence."


def test_a_paper_this_reader_cannot_be_trusted_on_says_so_before_it_is_scored() -> None:
    """TEST_CORPUS §5.2: a curated 'parses cleanly' subset, as a predicate rather than a hope."""
    from oc_eval.ground_truth import from_latex

    assert from_latex.looks_parseable(PAPER)
    assert not from_latex.looks_parseable(r"\documentclass{article}\begin{document}x\end{document}")
    assert not from_latex.looks_parseable(r"\section{A}" + "\n" + r"\input{sections/two}")
