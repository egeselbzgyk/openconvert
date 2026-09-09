"""D15 / IMPLEMENTATION_PLAN §1.7: PyMuPDF (AGPL) must never be importable here."""

import importlib.util


def test_pymupdf_is_not_installed() -> None:
    for banned in ("pymupdf", "fitz"):
        assert importlib.util.find_spec(banned) is None, f"{banned} is AGPL and banned (D15)"
