"""Scan simulation: turn clean text into something that looks photographed.

`--scanned-fixtures` (PHASE 13 detail 12) renders existing Typst fixtures at 200 and 300 dpi,
applies Gaussian noise, a rotation within +/-1.5 degrees, a brightness gradient and JPEG artefacts,
and wraps the pages with img2pdf into image-only PDFs under `corpus/fixtures/scanned/`, each with
its `.assert.json`. The ground truth, `<id>.gt.txt`, is not written here: it is the text this
pipeline extracts from the born-digital source with OCR off — what a perfect OCR of the scan would
give it — and the Rust test `scanned_ground_truth_is_the_born_digital_text` writes and checks it
(`OC_UPDATE_SCAN_GT=1`). Every random
choice comes from a seed derived from the fixture's name, so a re-run on the same toolchain
reproduces the bytes; `--check` regenerates into memory and compares with what is committed.

The PDFs are committed rather than regenerated at test time (TEST_CORPUS §6.4's fallback): the
render goes through pypdfium2's PDFium and Pillow's JPEG encoder, neither of which promises the
same bytes on every platform, and the Rust tests that read them must not depend on Python (D1).

`--make-fixture-asset` writes `corpus/fixtures/assets/scan_page_01.png`, the image
`f03_image_only.typ` places full-bleed on both of its pages. The result is a page that
contains rendered *pixels* of text and no PDF text objects at all, which is exactly the
`image_only` condition page classification has to recognise (D13.10).

The asset is generated once and committed, so nothing in the test path depends on this
script or on Python at all (D1). The speckle is drawn from a fixed seed, so re-running this
on the same machine reproduces the file byte for byte — but the text is drawn with whichever
font `FONT_CANDIDATES` finds first, which differs between platforms. That is precisely why
the PNG is committed rather than generated during a build: the fixture must be the same
bytes for everyone, and only a committed file can promise that.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import random
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, cast

from PIL import Image, ImageDraw, ImageFont

# A4 at 150 dpi, the resolution a consumer scanner defaults to.
WIDTH_PX = 1240
HEIGHT_PX = 1754

# Paper is never pure white in a scan.
PAPER_GREY = 235
INK_GREY = 40

# Speckle rather than per-pixel grain: a scan does look noisy, but noise on every pixel
# defeats PNG's filters and turns a 60 KB fixture into a megabyte. A sparse speckle over a
# posterised image reads the same to a classifier and to a human, and compresses.
NOISE_AMPLITUDE = 24
NOISE_PIXEL_SHARE = 0.002
NOISE_SEED = 20260909

# Scanners quantise; so does this, which is most of why the file stays small. Eight levels
# is the fewest that still leaves the paper grey rather than white: six rounds 235 up to
# 255, and pure-white paper is exactly what a scan is not.
GREY_LEVELS = 8

# A page laid down slightly crooked in the feeder.
SKEW_DEGREES = 0.4

MARGIN_PX = 110
LINE_HEIGHT_PX = 34
BODY_SIZE_PX = 22
HEADING_SIZE_PX = 34

HEADING = "A Scanned Chapter"
BODY = """It was a dark and stormy night; the rain fell in torrents, except at
occasional intervals, when it was checked by a violent gust of wind which swept
up the streets, rattling along the housetops, and fiercely agitating the scanty
flame of the lamps that struggled against the darkness.

The office was quiet. A single clerk remained at his desk, copying a schedule of
freight rates in a hand so regular that the page might have been printed. He did
not look up when the door opened, nor when it closed again.

Outside, the harbour lights went out one by one, and the last of the coasting
steamers slipped her moorings and stood away for the open sea. This page exists
only as pixels: there is not one text object anywhere in the PDF that carries it,
which is what makes it a test of page classification rather than of extraction."""

# Tried in order. A missing font is not fatal — the asset only has to look like text.
FONT_CANDIDATES = (
    "DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/Library/Fonts/Arial.ttf",
    "C:/Windows/Fonts/arial.ttf",
)


def _load_font(size: int) -> ImageFont.ImageFont | ImageFont.FreeTypeFont:
    for candidate in FONT_CANDIDATES:
        try:
            return ImageFont.truetype(candidate, size)
        except OSError:
            continue
    return ImageFont.load_default(size)


def make_fixture_asset(out_path: Path) -> Path:
    """Render the committed scan asset and write it to `out_path`."""
    page = Image.new("L", (WIDTH_PX, HEIGHT_PX), PAPER_GREY)
    draw = ImageDraw.Draw(page)

    y = MARGIN_PX
    draw.text((MARGIN_PX, y), HEADING, fill=INK_GREY, font=_load_font(HEADING_SIZE_PX))
    y += HEADING_SIZE_PX * 2

    body_font = _load_font(BODY_SIZE_PX)
    for line in BODY.splitlines():
        draw.text((MARGIN_PX, y), line, fill=INK_GREY, font=body_font)
        y += LINE_HEIGHT_PX

    # A slight rotation, filling the corners with paper rather than black. Done before
    # quantisation so the interpolated edges land on the same grey levels as everything else.
    page = page.rotate(SKEW_DEGREES, resample=Image.Resampling.BICUBIC, fillcolor=PAPER_GREY)
    page = page.point(lambda v: round(v / 255 * (GREY_LEVELS - 1)) * 255 // (GREY_LEVELS - 1))

    # Speckle, from a fixed seed so the file is reproducible.
    rng = random.Random(NOISE_SEED)
    pixels = page.load()
    if pixels is None:  # pragma: no cover - a mode-"L" image always loads
        raise RuntimeError("the page image could not be loaded for pixel access")
    speckles = int(WIDTH_PX * HEIGHT_PX * NOISE_PIXEL_SHARE)
    for _ in range(speckles):
        px = rng.randrange(WIDTH_PX)
        py = rng.randrange(HEIGHT_PX)
        # Mode "L" is one byte a pixel, so `pixels[x, y]` is a single value rather than the
        # tuple a colour mode would give. The cast says so; Pillow's stubs cannot.
        current = cast("int", pixels[px, py])
        value = current + rng.randint(-NOISE_AMPLITUDE, NOISE_AMPLITUDE)
        pixels[px, py] = max(0, min(255, value))

    out_path.parent.mkdir(parents=True, exist_ok=True)
    page.save(out_path, format="PNG", optimize=True)
    return out_path


# --------------------------------------------------------------------------- scanned fixtures

REPO_ROOT = Path(__file__).resolve().parents[4]
SCANNED_DIR = REPO_ROOT / "corpus" / "fixtures" / "scanned"
TYPST_OUTPUT = REPO_ROOT / "target" / "fixtures"
MANIFEST = REPO_ROOT / "corpus" / "manifest.json"

# PHASE 13 detail 12's degradations. The rotation is drawn per page from +/-MAX_ROTATION.
MAX_ROTATION_DEGREES = 1.5
NOISE_SIGMA = 6.0
# The brightness gradient: the left edge of the page is this much darker than the right, as a
# page lit from one side is.
GRADIENT_DEPTH = 0.12
# Low enough to leave visible block artefacts around the glyphs, high enough that the fixture is
# still a scan of text rather than a test of JPEG.
JPEG_QUALITY = 60
PAPER_WHITE = 245

POINTS_PER_INCH = 72.0


@dataclass(frozen=True)
class ScanSpec:
    """One scanned fixture: which Typst fixture, at which resolution, in which language."""

    source: str
    dpi: int
    lang: str

    @property
    def id(self) -> str:
        return f"{self.source.split('_', 1)[0]}__scan{self.dpi}"


# English at both resolutions (detail 12's 200 and 300 dpi), German and Turkish at 300: the three
# v1 languages, and the one resolution pair whose CER gap is the thing a 200-dpi scanner costs.
SCANS: tuple[ScanSpec, ...] = (
    ScanSpec("f01_prose_single_column", 300, "en"),
    ScanSpec("f01_prose_single_column", 200, "en"),
    ScanSpec("f04_german_prose", 300, "de"),
    ScanSpec("f05_turkish_prose", 300, "tr"),
)

# What each fixture's `.assert.json` checks, beyond the image count: a heading and a line of body
# text that OCR has to recover, each spelled as the source spells it.
EXPECTED: dict[str, dict[str, str]] = {
    "f01_prose_single_column": {
        "heading": "Chapter 3",
        "text": "It was a dark and stormy night",
    },
    "f04_german_prose": {
        "heading": "Der Herbst im Dorf",
        "text": "Der Herbst kam früh in diesem Jahr",
    },
    "f05_turkish_prose": {
        "heading": "Köyde Sonbahar",
        "text": "Sonbahar bu yıl erken geldi",
    },
}


def _seed(name: str) -> int:
    return int.from_bytes(hashlib.sha256(name.encode("utf-8")).digest()[:8], "big")


def _degrade(page: Image.Image, seed: int) -> Image.Image:
    """Noise, rotation, a brightness gradient and JPEG artefacts, all from `seed`."""
    import numpy as np

    rng = np.random.default_rng(seed)
    angle = float(rng.uniform(-MAX_ROTATION_DEGREES, MAX_ROTATION_DEGREES))
    rotated = page.rotate(angle, resample=Image.Resampling.BICUBIC, fillcolor=PAPER_WHITE)

    pixels = np.asarray(rotated, dtype=np.float64)
    width = pixels.shape[1]
    gradient = np.linspace(1.0 - GRADIENT_DEPTH, 1.0, width)[np.newaxis, :]
    pixels = pixels * gradient
    pixels = pixels + rng.normal(0.0, NOISE_SIGMA, size=pixels.shape)
    degraded = Image.fromarray(np.clip(pixels, 0, 255).astype(np.uint8), mode="L")

    buffer = io.BytesIO()
    degraded.save(buffer, format="JPEG", quality=JPEG_QUALITY, optimize=True)
    return Image.open(io.BytesIO(buffer.getvalue()))


def _jpeg_bytes(image: Image.Image) -> bytes:
    buffer = io.BytesIO()
    image.save(buffer, format="JPEG", quality=JPEG_QUALITY, optimize=True)
    return buffer.getvalue()


def build_scan(spec: ScanSpec, source_pdf: Path) -> bytes:
    """The scanned PDF's bytes."""
    import img2pdf  # type: ignore[import-untyped]
    import pypdfium2 as pdfium  # type: ignore[import-untyped]

    document = pdfium.PdfDocument(str(source_pdf))
    jpegs: list[bytes] = []
    sizes: list[tuple[float, float]] = []
    for index, page in enumerate(document):
        sizes.append((page.get_width(), page.get_height()))
        bitmap = page.render(scale=spec.dpi / POINTS_PER_INCH, grayscale=True)
        image = bitmap.to_pil().convert("L")
        jpegs.append(_jpeg_bytes(_degrade(image, _seed(f"{spec.id}/{index}"))))

    width_pt, height_pt = sizes[0]
    layout = img2pdf.get_layout_fun((width_pt, height_pt))
    # The internal writer, not pikepdf: pikepdf gives every file a fresh random `/ID`, which would
    # make the fixture different bytes on every run.
    pdf = img2pdf.convert(jpegs, layout_fun=layout, nodate=True, engine=img2pdf.Engine.internal)
    if pdf is None:  # pragma: no cover - img2pdf returns bytes when no stream is given
        raise RuntimeError("img2pdf produced nothing")
    return pdf


def assertions(spec: ScanSpec, pages: int) -> list[dict[str, Any]]:
    expected = EXPECTED[spec.source]
    return [
        {
            "kind": "text_present",
            "text": expected["text"],
            "note": "a line of body text, recovered by OCR from pixels",
        },
        {
            "kind": "heading_level",
            "text": expected["heading"],
            "level": 1,
            "note": "the chapter heading, recovered by OCR and recognised as one",
        },
        {
            "kind": "image_count",
            "equals": 0,
            "note": f"all {pages} scanned pages were read, so none is carried as a picture",
        },
    ]


def manifest_entry(spec: ScanSpec, pdf: bytes, pages: int) -> dict[str, Any]:
    return {
        "id": spec.id,
        "title": f"{spec.source}, scanned at {spec.dpi} dpi",
        "source": {
            "name": "synthetic-generator",
            "url": "https://github.com/openconvert/openconvert",
            "retrieved_date": "2026-09-23",
        },
        "license": {
            "name": "CC0-1.0",
            "url": "https://creativecommons.org/publicdomain/zero/1.0/",
            "verified_by": "maintainer",
            "verified_date": "2026-09-23",
        },
        "sha256": hashlib.sha256(pdf).hexdigest(),
        "file_size_bytes": len(pdf),
        "pages": pages,
        "category": "edge-case",
        # The pixels came from our own Typst renderer, so this is `ours(*)` and counts against
        # `corpus.ours_max_share` (D18), however much it looks like a scan.
        "producer_stratum": "ours(Typst)",
        "producer_raw": "img2pdf",
        "tagged": False,
        "holdout": False,
        "expected_problems": ["image-only"],
        "expected_output_characteristics": {"languages": [spec.lang]},
        "ground_truth_type": "generated-from-source-text",
        "ground_truth_ref": f"corpus/fixtures/scanned/{spec.id}.gt.txt",
        "generator": "oc-eval scan-sim",
        "defect_injection": [
            f"rasterized-{spec.dpi}dpi",
            "gaussian-noise",
            "rotation",
            "brightness-gradient",
            "jpeg-artefacts",
        ],
        "source_path": f"corpus/fixtures/scanned/{spec.id}.pdf",
    }


def merge_manifest_files(
    existing: list[dict[str, Any]], entries: list[dict[str, Any]]
) -> list[dict[str, Any]]:
    """Replace or add `entries` among `existing`, in the manifest's one order: sorted by id.

    `xtask fixtures` and `manifest.dump` sort the same way, so whichever writer ran last the
    file is byte-identical — CI runs two of them in a row and then checks the tree is clean.
    """
    ids = {entry["id"] for entry in entries}
    kept = [entry for entry in existing if entry["id"] not in ids]
    return sorted(kept + entries, key=lambda entry: str(entry["id"]))


def scanned_fixtures(*, check: bool = False) -> list[str]:
    """Write (or, with `check`, compare) every scanned fixture.

    Returns the paths that differ, or that were written.
    """
    import pypdfium2 as pdfium  # type: ignore[import-untyped]

    changed: list[str] = []
    entries: list[dict[str, Any]] = []
    SCANNED_DIR.mkdir(parents=True, exist_ok=True)
    for spec in SCANS:
        source = TYPST_OUTPUT / f"{spec.source}.pdf"
        if not source.is_file():
            raise FileNotFoundError(f"{source} is missing; run `cargo run -p xtask -- fixtures`")
        pdf = build_scan(spec, source)
        pages = len(pdfium.PdfDocument(pdf))
        expected = json.dumps(assertions(spec, pages), indent=2, ensure_ascii=False) + "\n"
        outputs = {
            SCANNED_DIR / f"{spec.id}.pdf": pdf,
            SCANNED_DIR / f"{spec.id}.assert.json": expected.encode("utf-8"),
        }
        for path, data in outputs.items():
            current = path.read_bytes() if path.exists() else None
            if current != data:
                changed.append(str(path.relative_to(REPO_ROOT)))
                if not check:
                    path.write_bytes(data)
        entries.append(manifest_entry(spec, pdf, pages))

    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    manifest["files"] = merge_manifest_files(manifest["files"], entries)
    rendered = json.dumps(manifest, indent=2, ensure_ascii=False) + "\n"
    if MANIFEST.read_text(encoding="utf-8") != rendered:
        changed.append(str(MANIFEST.relative_to(REPO_ROOT)))
        if not check:
            MANIFEST.write_text(rendered, encoding="utf-8")
    return changed


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--make-fixture-asset",
        action="store_true",
        help="write corpus/fixtures/assets/scan_page_01.png and exit",
    )
    parser.add_argument(
        "--scanned-fixtures",
        action="store_true",
        help="write corpus/fixtures/scanned/*.pdf and their assertions",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="with --scanned-fixtures: regenerate in memory and fail if anything differs",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=Path("corpus/fixtures/assets/scan_page_01.png"),
    )
    args = parser.parse_args()

    if args.make_fixture_asset:
        written = make_fixture_asset(args.out)
        print(f"wrote {written} ({written.stat().st_size} bytes)")
    elif args.scanned_fixtures:
        changed = scanned_fixtures(check=args.check)
        verb = "differs" if args.check else "written"
        for path in changed:
            print(f"{verb}: {path}")
        if args.check and changed:
            sys.exit(1)
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
