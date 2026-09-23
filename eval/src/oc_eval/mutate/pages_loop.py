"""Page trees that contain themselves (PHASE 14 detail 9; row 14.17)."""

from __future__ import annotations

from oc_eval.mutate import pdfbytes as pb

RESOURCES = b" /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>"
PAGE = b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792]" + RESOURCES
CONTENT = pb.stream("", b"BT /F1 12 Tf 72 700 Td (A page.) Tj ET")
FONT = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"


def mutations() -> list[tuple[str, bytes, str]]:
    kids_self = pb.assemble(
        [
            b"<< /Type /Catalog /Pages 2 0 R >>",
            b"<< /Type /Pages /Count 2 /Kids [3 0 R 2 0 R] >>",
            PAGE,
            CONTENT,
            FONT,
        ]
    )
    two_node = pb.assemble(
        [
            b"<< /Type /Catalog /Pages 2 0 R >>",
            b"<< /Type /Pages /Count 2 /Kids [3 0 R 6 0 R] >>",
            PAGE,
            CONTENT,
            FONT,
            b"<< /Type /Pages /Parent 2 0 R /Count 1 /Kids [2 0 R] >>",
        ]
    )
    kids_catalog = pb.assemble(
        [
            b"<< /Type /Catalog /Pages 2 0 R >>",
            b"<< /Type /Pages /Count 1 /Kids [1 0 R] >>",
            PAGE,
            CONTENT,
            FONT,
        ]
    )
    parent_loop = pb.assemble(
        [
            b"<< /Type /Catalog /Pages 2 0 R >>",
            b"<< /Type /Pages /Parent 2 0 R /Count 1 /Kids [3 0 R] >>",
            b"<< /Type /Page /Parent 3 0 R /MediaBox [0 0 612 792]" + RESOURCES,
            CONTENT,
            FONT,
        ]
    )
    return [
        ("pages_loop__kids_self", kids_self, "/Pages lists itself among its /Kids"),
        ("pages_loop__two_nodes", two_node, "two /Pages nodes that are each other's kid"),
        ("pages_loop__kids_catalog", kids_catalog, "/Kids names the catalogue"),
        ("pages_loop__parent_self", parent_loop, "every node is its own /Parent"),
    ]
