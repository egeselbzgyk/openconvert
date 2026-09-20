"""What a PDF says about itself, turned into the stratum a report groups it by.

D18 buckets on `/Producer` + `/Creator` and reports per stratum, never in aggregate. Both are
free text a publisher can write anything into, so the mapping is a list of markers in priority
order rather than a table of exact strings, and everything that matches nothing is `unknown` —
a stratum in its own right, not a failure.

`/Producer` is the last tool that wrote the file and `/Creator` is usually the one that laid it
out. Distiller and Ghostscript overwrite `/Producer` on the way through, which is why a file
that says "Acrobat Distiller" is asked what its `/Creator` was before it is given up on.
"""

from __future__ import annotations

import re

from oc_eval.corpus.manifest import LICENSE_ALLOWLIST

# Ordered: the first marker that appears in the string wins. Ghostscript sits below the
# page-description tools because it is a post-processor — a Ghostscript-stamped file that also
# names InDesign was laid out in InDesign.
PRODUCER_MARKERS: tuple[tuple[str, tuple[str, ...]], ...] = (
    ("InDesign", ("indesign",)),
    ("Quark", ("quarkxpress", "quark xpress")),
    ("Word", ("microsoft word", "microsoft® word", "word for windows", "winword")),
    (
        "pdfTeX",
        ("pdftex", "xetex", "luatex", "pdflatex", "latex", "dvips", "dvipdfm", "tex output"),
    ),
    (
        "ABBYY-scanner",
        (
            "abbyy",
            "finereader",
            "luradocument",
            "scanner",
            "scansoft",
            "kofax",
            "omnipage",
            "canon",
            "xerox",
            "ricoh",
            "konica",
            "epson",
            "hp digital sending",
            "i2s ",
            "copibook",
            "bookdrive",
        ),
    ),
    ("Ghostscript", ("ghostscript",)),
)

# Ours only when we rendered it (D18). A real book someone else set in Typst is not evidence
# about our renderer, so the harvester passes `real_world=True` and it lands in `unknown`.
OURS_MARKERS: tuple[tuple[str, tuple[str, ...]], ...] = (
    ("ours(Typst)", ("typst",)),
    ("ours(WeasyPrint)", ("weasyprint",)),
)

UNKNOWN = "unknown"

# The strata `classify_producer` can return from a producer marker alone.
PRODUCER_MARKERS_STRATA = frozenset(stratum for stratum, _ in PRODUCER_MARKERS)

# A stratum that is also a post-processor: real, but outranked by anything that names the
# tool which actually laid the page out.
POST_PROCESSOR_STRATA = frozenset({"Ghostscript"})

# Producers that say only "something turned this into PDF", so the creator is worth asking.
PASS_THROUGH_MARKERS = ("distiller", "pdfwriter", "pdf library", "quartz", "macos", "preview")

_CC_LICENSE_RE = re.compile(
    r"creativecommons\.org/licenses/(?P<code>[a-z-]+)/(?P<version>\d+\.\d+)", re.IGNORECASE
)
_CC_PD_RE = re.compile(
    r"creativecommons\.org/publicdomain/(?P<kind>zero|mark)/(?P<version>\d+\.\d+)", re.IGNORECASE
)


def classify_producer(producer: str, *, creator: str = "", real_world: bool = False) -> str:
    """The stratum for a `/Producer`, consulting `/Creator` when the producer is a pass-through.

    Ghostscript is a stratum D18 names and a post-processor at the same time. It is kept when
    it is all the file says, and stood aside for whatever laid the page out when the `/Creator`
    knows: a Ghostscript-stamped file that also names LaTeX is a pdfTeX book that went through
    Ghostscript, and filing it under Ghostscript would put TeX's spacing defects in the wrong
    bucket.
    """
    from_producer = _match(producer, real_world=real_world)
    if from_producer is not None and from_producer not in POST_PROCESSOR_STRATA:
        return from_producer

    if from_producer is not None or _is_pass_through(producer) or not producer.strip():
        from_creator = _match(creator, real_world=real_world)
        if from_creator is not None and from_creator not in POST_PROCESSOR_STRATA:
            return from_creator

    return from_producer if from_producer is not None else UNKNOWN


def _match(text: str, *, real_world: bool) -> str | None:
    lowered = text.lower()
    if not real_world:
        for stratum, markers in OURS_MARKERS:
            if any(marker in lowered for marker in markers):
                return stratum
    for stratum, markers in PRODUCER_MARKERS:
        if any(marker in lowered for marker in markers):
            return stratum
    return None


def _is_pass_through(producer: str) -> bool:
    lowered = producer.lower()
    return any(marker in lowered for marker in PASS_THROUGH_MARKERS)


def license_from_url(url: str | None) -> str | None:
    """The identifier TEST_CORPUS §7.1's allowlist speaks, or None if the URL is not a licence.

    A non-commercial or no-derivatives licence is named rather than dropped, so that the gate
    rejects the entry loudly instead of the harvester discarding it quietly.
    """
    if not url:
        return None

    public_domain = _CC_PD_RE.search(url)
    if public_domain is not None:
        return "CC0-1.0" if public_domain.group("kind").lower() == "zero" else "PD-old-work"

    licensed = _CC_LICENSE_RE.search(url)
    if licensed is not None:
        code = licensed.group("code").upper()
        return f"CC-{code}-{licensed.group('version')}"

    return None


def is_acceptable_license(name: str | None) -> bool:
    """TEST_CORPUS §7.1's allowlist as the harvester's admission test.

    The same set the lint uses, exposed here so a source adapter can reject a candidate
    before spending a download on it.
    """
    return name is not None and name in LICENSE_ALLOWLIST
