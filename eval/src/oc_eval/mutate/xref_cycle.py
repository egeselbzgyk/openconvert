"""Cross-reference chains that loop or run deep (PHASE 14 detail 9; rows 14.4, 14.5, 14.17)."""

from __future__ import annotations

from oc_eval.mutate import pdfbytes as pb


def mutations() -> list[tuple[str, bytes, str]]:
    base = pb.seed("h01_two_glyphs")
    first = pb.startxref(base)
    catalog = pb.object_offsets(base)
    body = base[catalog[1] :].split(b"endobj", 1)[0].split(b"obj", 1)[1]

    # One update whose /Prev is itself.
    self_loop, _ = pb.update(base, {1: body.strip()}, prev=pb.SELF)

    # Two updates pointing at each other: A's /Prev is written wide, then patched to B.
    width = 10
    with_a, a_at = pb.update(base, {1: body.strip()}, prev=int("9" * width))
    with_b, b_at = pb.update(with_a, {1: body.strip()}, prev=a_at)
    wide = f"/Prev {'9' * width}".encode()
    two_cycle = with_b.replace(wide, f"/Prev {b_at:0{width}d}".encode(), 1)

    # Two hundred updates, each pointing at the one before: past the 128-section cap.
    deep, prev = base, first
    for _ in range(200):
        deep, prev = pb.update(deep, {1: body.strip()}, prev=prev)

    # /Prev past the end of the file, and negative.
    beyond, _ = pb.update(base, {1: body.strip()}, prev=len(base) * 10)
    negative = beyond.replace(f"/Prev {len(base) * 10}".encode(), b"/Prev -5", 1)

    return [
        ("xref_cycle__self_loop", self_loop, "the newest section's /Prev is its own offset"),
        ("xref_cycle__two_cycle", two_cycle, "two updates whose /Prev point at each other"),
        ("xref_cycle__depth_200", deep, "200 incremental updates: a chain past max_xref_chain"),
        ("xref_cycle__prev_beyond_eof", beyond, "/Prev points past the end of the file"),
        ("xref_cycle__prev_negative", negative, "/Prev is negative"),
    ]
