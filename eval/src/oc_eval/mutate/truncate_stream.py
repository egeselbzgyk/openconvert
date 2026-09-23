"""Streams cut short, and `/Length`s that lie (PHASE 14 detail 9; row 14.17)."""

from __future__ import annotations

import re

from oc_eval.mutate import pdfbytes as pb


def _content_stream(pdf: bytes) -> tuple[int, int]:
    """The data span of the first content stream."""
    start = pdf.index(b"stream\n") + len(b"stream\n")
    end = pdf.index(b"endstream", start)
    return start, end


def mutations() -> list[tuple[str, bytes, str]]:
    base = pb.seed("h23_paragraph_across_pages")
    start, end = _content_stream(base)
    half = start + (end - start) // 2
    out = []
    # The file ends in the middle of a stream: no endstream, no xref, no trailer.
    out.append(("truncate__mid_stream", base[:half], "the file ends inside a content stream"))
    # The file ends after the objects, before the xref.
    xref = pb.startxref(base)
    out.append(("truncate__no_xref", base[:xref], "no cross-reference table and no trailer"))
    # /Length far larger than the data, and far smaller.
    lengths = list(re.finditer(rb"/Length (\d+)", base))
    first = lengths[0]
    huge = base[: first.start(1)] + b"999999999" + base[first.end(1) :]
    out.append(("truncate__length_huge", huge, "/Length claims a gigabyte the file does not hold"))
    tiny = base[: first.start(1)] + b"3" + base[first.end(1) :]
    out.append(("truncate__length_tiny", tiny, "/Length claims 3 bytes of a longer stream"))
    # A flate stream cut in half, with an honest dictionary otherwise.
    data = pb.flate(b"BT /F1 12 Tf 72 700 Td (" + b"Cut short. " * 400 + b") Tj ET")
    cut = pb.assemble(
        [
            b"<< /Type /Catalog /Pages 2 0 R >>",
            b"<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R"
            b" /Resources << /Font << /F1 5 0 R >> >> >>",
            pb.stream("/Filter /FlateDecode", data[: len(data) // 2]),
            b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
        ]
    )
    out.append(("truncate__flate_half", cut, "a FlateDecode content stream cut in half"))
    return out
