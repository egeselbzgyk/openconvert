"""What a downloaded PDF has to tell us before it can become a manifest entry.

Page count, `/Producer`, `/Creator` and whether it carries a struct tree — the four facts D18's
stratification and §1.8's `tagged` field are built out of. Everything here reads the file with
`pikepdf` (qpdf, Apache-2.0); PyMuPDF is AGPL and banned (D15), and the engine's own backend is
not importable from Python by design.

A file that cannot be opened at all is a finding, not an exception: a corpus source that serves
an HTML error page with a `.pdf` name is a thing that happens, and the harvester needs to say so
and move on rather than stop.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import pikepdf


@dataclass(frozen=True)
class Probe:
    """The facts a manifest entry needs from the bytes themselves."""

    pages: int
    producer: str
    creator: str
    tagged: bool
    encrypted: bool
    pdf_version: str

    @property
    def usable(self) -> bool:
        return self.pages > 0 and not self.encrypted


class NotAPdf(RuntimeError):
    """The file did not open as a PDF. Usually an HTML error page under a `.pdf` name."""


def probe(path: Path) -> Probe:
    """Open `path` and read the four facts. Raises `NotAPdf` if it is not one."""
    try:
        with pikepdf.open(path) as pdf:
            info = _docinfo(pdf)
            return Probe(
                pages=len(pdf.pages),
                producer=info.get("Producer", ""),
                creator=info.get("Creator", ""),
                tagged=_is_tagged(pdf),
                encrypted=pdf.is_encrypted,
                pdf_version=str(pdf.pdf_version),
            )
    except pikepdf.PasswordError:
        # Encrypted with a password we do not have: openable enough to classify, not to read.
        return Probe(0, "", "", tagged=False, encrypted=True, pdf_version="")
    except (pikepdf.PdfError, OSError, ValueError) as failure:
        raise NotAPdf(f"{path.name}: {failure}") from failure


def _docinfo(pdf: pikepdf.Pdf) -> dict[str, str]:
    """`/Producer` and `/Creator` as text, whatever the document info dictionary holds."""
    fields: dict[str, str] = {}
    try:
        info = pdf.docinfo
    except (pikepdf.PdfError, ValueError):
        return fields
    for key in ("Producer", "Creator"):
        try:
            value = info.get(f"/{key}")
        except (pikepdf.PdfError, ValueError):
            continue
        if value is None:
            continue
        try:
            fields[key] = str(value)
        except (UnicodeDecodeError, ValueError):
            # A producer string in an encoding qpdf cannot decode is still a producer string;
            # it is just not one we can bucket on.
            fields[key] = ""
    return fields


def _is_tagged(pdf: pikepdf.Pdf) -> bool:
    """A struct tree in the catalogue. D18's `tagged` bucket is exactly this question."""
    try:
        root = pdf.Root
    except (pikepdf.PdfError, AttributeError):
        return False
    return "/StructTreeRoot" in root
