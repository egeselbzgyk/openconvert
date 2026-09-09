"""Scan simulation: turn clean text into something that looks photographed.

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
import random
from pathlib import Path

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


def _load_font(size: int) -> ImageFont.ImageFont:
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
    page = page.rotate(SKEW_DEGREES, resample=Image.BICUBIC, fillcolor=PAPER_GREY)
    page = page.point(lambda v: round(v / 255 * (GREY_LEVELS - 1)) * 255 // (GREY_LEVELS - 1))

    # Speckle, from a fixed seed so the file is reproducible.
    rng = random.Random(NOISE_SEED)
    pixels = page.load()
    speckles = int(WIDTH_PX * HEIGHT_PX * NOISE_PIXEL_SHARE)
    for _ in range(speckles):
        px = rng.randrange(WIDTH_PX)
        py = rng.randrange(HEIGHT_PX)
        value = pixels[px, py] + rng.randint(-NOISE_AMPLITUDE, NOISE_AMPLITUDE)
        pixels[px, py] = max(0, min(255, value))

    out_path.parent.mkdir(parents=True, exist_ok=True)
    page.save(out_path, format="PNG", optimize=True)
    return out_path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--make-fixture-asset",
        action="store_true",
        help="write corpus/fixtures/assets/scan_page_01.png and exit",
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
    else:
        parser.print_help()


if __name__ == "__main__":
    main()
