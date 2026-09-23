"""Object streams nested in object streams, deep or in a loop (PHASE 14 details 3, 9; row 14.17).

PDF 32000-1 §7.5.7 forbids a stream inside an object stream, and so an object stream inside
another; a cross-reference stream can still *say* so. Each file here keeps the catalogue in an
object stream whose own entry says it lives in another object stream, and so on.
"""

from __future__ import annotations

import struct

from oc_eval.mutate import pdfbytes as pb


def _nested(depth: int, loop: bool) -> bytes:
    """Catalogue (1) in ObjStm 10; ObjStm 10+i in ObjStm 11+i; the last is a real stream — or,
    with ``loop``, the last points back at the first."""
    head = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
    offsets: dict[int, int] = {}

    def put(number: int, body: bytes) -> None:
        offsets[number] = len(head)
        head.extend(f"{number} 0 obj\n".encode() + body + b"\nendobj\n")

    put(2, b"<< /Type /Pages /Count 1 /Kids [3 0 R] >>")
    put(3, b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>")
    catalog = b"<< /Type /Catalog /Pages 2 0 R >>"
    first = b"1 0 "
    put(10, pb.stream(f"/Type /ObjStm /N 1 /First {len(first)}", first + catalog))
    xref_number = 10 + depth + 1
    entries: dict[int, tuple[int, int, int]] = {
        0: (0, 0, 65535),
        1: (2, 10, 0),
        2: (1, offsets[2], 0),
        3: (1, offsets[3], 0),
    }
    for level in range(depth):
        container = 10 + level
        parent = 10 + level + 1
        if loop and level == depth - 1:
            parent = 10
        entries[container] = (2, parent, 0)
    if not loop:
        entries[10 + depth] = (1, offsets[10], 0)
    xref_at = len(head)
    entries[xref_number] = (1, xref_at, 0)
    size = xref_number + 1
    rows = b"".join(struct.pack(">BIH", *entries.get(n, (0, 0, 0))) for n in range(size))
    body = pb.stream(f"/Type /XRef /Size {size} /W [1 4 2] /Root 1 0 R", rows)
    head.extend(f"{xref_number} 0 obj\n".encode() + body + b"\nendobj\n")
    head.extend(f"startxref\n{xref_at}\n%%EOF\n".encode())
    return bytes(head)


def mutations() -> list[tuple[str, bytes, str]]:
    deep = "the catalogue 200 object streams deep"
    loop = "three object streams that contain each other"
    shallow = "two object streams deep: past the spec, inside the cap"
    return [
        ("objstm_nest__depth_200", _nested(200, loop=False), deep),
        ("objstm_nest__loop", _nested(3, loop=True), loop),
        ("objstm_nest__depth_2", _nested(2, loop=False), shallow),
    ]
