"""Byte flips in the cross-reference table (PHASE 14 detail 9; row 14.17).

Each file flips a few bytes of one seed's `xref` section — offsets that point mid-object, a
generation that is not a number, an `n` that becomes something else — from a seeded generator.
"""

from __future__ import annotations

from oc_eval.mutate import pdfbytes as pb

SEEDS = ("h01_two_glyphs", "h23_paragraph_across_pages", "h13_outline")
VARIANTS = 4
FLIPS = 6


def mutations() -> list[tuple[str, bytes, str]]:
    out = []
    for seed_index, name in enumerate(SEEDS):
        base = pb.seed(name)
        start = pb.startxref(base)
        end = base.index(b"trailer", start)
        for variant in range(VARIANTS):
            rng = pb.Rng(1 + seed_index * 100 + variant)
            data = bytearray(base)
            for _ in range(FLIPS):
                at = start + rng.next(end - start)
                data[at] = rng.next(256)
            out.append(
                (
                    f"xref_flip__{name}__{variant}",
                    bytes(data),
                    f"{FLIPS} random bytes of {name}'s xref table replaced",
                )
            )
    return out
