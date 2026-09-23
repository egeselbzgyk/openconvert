"""Byte-level PDF surgery for the crash corpus (PHASE 14 detail 9).

The mutations are lies a writing library would refuse to tell — a ``/Prev`` that loops, a
``/Length`` that is wrong, a page tree that contains itself — so they are made here, on bytes,
with exact offsets. Everything is deterministic: the corpus is regenerated and compared in CI.
"""

from __future__ import annotations

import re
import zlib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[4]
SEEDS = REPO_ROOT / "corpus" / "fixtures" / "handmade"

STARTXREF = re.compile(rb"startxref\s+(\d+)\s+%%EOF\s*$")


def seed(name: str) -> bytes:
    return (SEEDS / f"{name}.pdf").read_bytes()


def startxref(pdf: bytes) -> int:
    match = STARTXREF.search(pdf)
    if match is None:
        raise ValueError("no startxref")
    return int(match.group(1))


def object_offsets(pdf: bytes) -> dict[int, int]:
    """Where each ``N 0 obj`` begins, by object number (the last one wins, as in an update)."""
    return {int(m.group(1)): m.start() for m in re.finditer(rb"(?m)^(\d+) 0 obj\b", pdf)}


def root_ref(pdf: bytes) -> str:
    match = re.search(rb"/Root\s+(\d+\s+\d+\s+R)", pdf)
    if match is None:
        raise ValueError("no /Root")
    return match.group(1).decode()


def size_of(pdf: bytes) -> int:
    matches = re.findall(rb"/Size\s+(\d+)", pdf)
    return int(matches[-1]) if matches else 0


SELF = -1
"""``update``'s ``prev`` for an update whose ``/Prev`` is its own offset."""


def update(
    pdf: bytes, objects: dict[int, bytes], prev: int | None, extra: str = ""
) -> tuple[bytes, int]:
    """Append an incremental update with ``objects``; return the file and its xref offset.

    ``prev`` is the previous section's offset, ``None`` for none, or :data:`SELF`.
    """
    out = bytearray(pdf)
    if not out.endswith(b"\n"):
        out += b"\n"
    offsets = {}
    for number in sorted(objects):
        offsets[number] = len(out)
        out += f"{number} 0 obj\n".encode() + objects[number] + b"\nendobj\n"
    at = len(out)
    out += b"xref\n0 1\n0000000000 65535 f \n"
    for number in sorted(offsets):
        out += f"{number} 1\n{offsets[number]:010d} 00000 n \n".encode()
    size = max([size_of(pdf), *[n + 1 for n in objects]])
    target = at if prev == SELF else prev
    prev_entry = f" /Prev {target}" if target is not None else ""
    out += f"trailer\n<< /Size {size} /Root {root_ref(pdf)}{prev_entry}{extra} >>\n".encode()
    out += f"startxref\n{at}\n%%EOF\n".encode()
    return bytes(out), at


def assemble(objects: list[bytes], trailer_extra: str = "") -> bytes:
    """A one-section PDF from object bodies numbered from 1; object 1 is the catalogue."""
    out = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
    offsets = []
    for number, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += f"{number} 0 obj\n".encode() + body + b"\nendobj\n"
    at = len(out)
    out += f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n".encode()
    for offset in offsets:
        out += f"{offset:010d} 00000 n \n".encode()
    out += f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R{trailer_extra} >>\n".encode()
    out += f"startxref\n{at}\n%%EOF\n".encode()
    return bytes(out)


def stream(entries: str, data: bytes) -> bytes:
    return f"<< {entries} /Length {len(data)} >>\nstream\n".encode() + data + b"\nendstream"


def flate(data: bytes) -> bytes:
    return zlib.compress(data, 9)


class Rng:
    """A seeded xorshift: the same corpus on every machine and every Python."""

    def __init__(self, seed: int) -> None:
        self.state = seed or 1

    def next(self, bound: int) -> int:
        x = self.state
        x ^= (x << 13) & 0xFFFFFFFFFFFFFFFF
        x ^= x >> 7
        x ^= (x << 17) & 0xFFFFFFFFFFFFFFFF
        self.state = x
        return x % max(bound, 1)
