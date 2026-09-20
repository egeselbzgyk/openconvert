"""arXiv LaTeX source as ground truth.

TEST_CORPUS §5.2. `\\section` and `\\subsection` give the exact heading hierarchy, `\\footnote`
gives the footnotes, `\\includegraphics` with `\\caption` gives figure/caption pairs — the
structure is in the source, so no OCR pipeline has to guess it.

§5.2 also says to budget for a curated "LaTeX-parses-cleanly" subset rather than assuming every
paper is usable, and that is the rule this module is written to. It is a reader of the sectioning
commands, not a TeX engine: a paper whose sections come out of a custom macro, a conditional or
an `\\input` chain is one this cannot read, and [`looks_parseable`] says so *before* the paper is
admitted to a scored set rather than after it has quietly scored zero.
"""

from __future__ import annotations

import re

from oc_eval.ground_truth.schema import Figure, Footnote, GroundTruth, Heading

# The sectioning commands, and the level each one means. `\part` is 0 in LaTeX's own numbering
# and a level-1 heading in any book made from it.
SECTION_LEVELS = {
    "part": 1,
    "chapter": 1,
    "section": 1,
    "subsection": 2,
    "subsubsection": 3,
    "paragraph": 4,
    "subparagraph": 5,
}

SECTION_RE = re.compile(
    r"\\(" + "|".join(SECTION_LEVELS) + r")\*?\s*(?:\[[^\]]*\])?\s*\{", re.MULTILINE
)
FOOTNOTE_RE = re.compile(r"\\footnote\s*\{")
GRAPHICS_RE = re.compile(r"\\includegraphics\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}")
CAPTION_RE = re.compile(r"\\caption\s*\{")
FIGURE_ENV_RE = re.compile(r"\\begin\{figure\*?\}(.*?)\\end\{figure\*?\}", re.DOTALL)
COMMENT_RE = re.compile(r"(?<!\\)%.*?$", re.MULTILINE)
INPUT_RE = re.compile(r"\\(?:input|include)\s*\{")

# Inline markup that carries no text of its own. Stripped so a heading reads as a heading.
MARKUP_RE = re.compile(r"\\(?:emph|textit|textbf|texttt|textsc|mbox|text)\s*\{")
ESCAPED_RE = re.compile(r"\\([&%$#_{}])")
SPACING_RE = re.compile(r"\\[,;:!]|\\ |~")

# A brace nesting deeper than this in one argument is a macro doing something this reader is
# not going to understand anyway.
MAX_BRACE_DEPTH = 32


def looks_parseable(source: str) -> bool:
    """Whether this reader can be trusted on a paper, asked before it is scored against.

    §5.2's curated subset, as a predicate. A paper that pulls its sections in through
    `\\input`, or that has no sectioning commands at all, is not one whose structure this file
    holds.
    """
    body = _strip_comments(source)
    if INPUT_RE.search(body):
        return False
    return bool(SECTION_RE.search(body))


def ground_truth(source: str, *, title: str) -> GroundTruth:
    body = _strip_comments(source)

    headings = tuple(
        Heading(level=SECTION_LEVELS[match.group(1)], text=text, id=None)
        for match, text in _arguments(body, SECTION_RE)
        if text
    )
    footnotes = tuple(
        Footnote(marker_id=f"fn-{index + 1}", note_id=f"note-{index + 1}", body=text)
        for index, (_, text) in enumerate(_arguments(body, FOOTNOTE_RE))
        if text
    )
    figures = tuple(_figures(body))

    return GroundTruth(
        title=title,
        headings=headings,
        paragraphs=(),
        footnotes=footnotes,
        figures=figures,
        languages=("en",),
    )


def _figures(body: str) -> list[Figure]:
    found: list[Figure] = []
    for environment in FIGURE_ENV_RE.findall(body):
        graphic = GRAPHICS_RE.search(environment)
        captions = [text for _, text in _arguments(environment, CAPTION_RE)]
        found.append(
            Figure(
                alt="",
                src=graphic.group(1).strip() if graphic else "",
                caption=captions[0] if captions else None,
            )
        )
    return found


def _arguments(body: str, pattern: re.Pattern[str]) -> list[tuple[re.Match[str], str]]:
    """Each match of `pattern`, paired with the balanced `{...}` argument that follows it."""
    found: list[tuple[re.Match[str], str]] = []
    for match in pattern.finditer(body):
        argument = _balanced(body, match.end() - 1)
        if argument is not None:
            found.append((match, _plain(argument)))
    return found


def _balanced(body: str, open_brace: int) -> str | None:
    """The text between `body[open_brace]` and its matching close brace."""
    depth = 0
    for index in range(open_brace, len(body)):
        char = body[index]
        if char == "{" and not _escaped(body, index):
            depth += 1
            if depth > MAX_BRACE_DEPTH:
                return None
        elif char == "}" and not _escaped(body, index):
            depth -= 1
            if depth == 0:
                return body[open_brace + 1 : index]
    return None


def _escaped(body: str, index: int) -> bool:
    backslashes = 0
    while index - 1 - backslashes >= 0 and body[index - 1 - backslashes] == "\\":
        backslashes += 1
    return backslashes % 2 == 1


def _strip_comments(source: str) -> str:
    return COMMENT_RE.sub("", source)


def _plain(fragment: str) -> str:
    """A LaTeX argument reduced to the words a reader would see."""
    text = MARKUP_RE.sub("", fragment)
    text = text.replace("{", " ").replace("}", " ")
    text = SPACING_RE.sub(" ", text)
    text = ESCAPED_RE.sub(r"\1", text)
    text = re.sub(r"\\[a-zA-Z]+", " ", text)
    return " ".join(text.split())
