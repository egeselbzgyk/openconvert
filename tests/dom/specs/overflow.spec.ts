// Row 6.13: no horizontal overflow, at any of the three viewports.
//
// The one thing a reflowable book must not do is make the reader scroll sideways, and it is the one
// defect no amount of markup validation catches: a `<pre>` with a long line and an unconstrained
// `<table>` are both perfectly valid XHTML. `scrollWidth <= clientWidth` is the measurement, taken
// in a browser, because the answer depends on line breaking and on the stylesheet together.
//
// Row 6.13's other half is here too: an image must render at a non-zero size. A figure whose `src`
// does not resolve still parses, still validates, and shows nothing — which is exactly the RSC-007
// defect EPUBCheck found in Phase 5, seen from the reader's side.

import { expect, test } from "@playwright/test";

import { documentUrl, fixtures } from "./fixtures";

/**
 * How much a block may exceed its own client width before it counts as overflow.
 *
 * One CSS pixel. Sub-pixel layout rounding makes an exactly-fitting element measure 0.4px wide in
 * some engines, and a zero tolerance would fail on arithmetic rather than on a defect. One pixel is
 * far below anything a reader could scroll.
 */
const TOLERANCE_PX = 1;

for (const fixture of fixtures()) {
  test.describe(fixture.fixture, () => {
    for (const document of fixture.spine) {
      test(`dom_no_horizontal_overflow: ${document}`, async ({ page }) => {
        await page.goto(documentUrl(fixture.fixture, document));

        // The document itself first: a page that scrolls sideways as a whole is the failure a reader
        // meets, whichever element caused it.
        const root = await page.evaluate(() => ({
          scrollWidth: document.documentElement.scrollWidth,
          clientWidth: document.documentElement.clientWidth,
        }));
        expect(
          root.scrollWidth,
          `${document}: the page scrolls sideways (${root.scrollWidth} > ${root.clientWidth})`,
        ).toBeLessThanOrEqual(root.clientWidth + TOLERANCE_PX);

        // Then every block, so a failure names the element rather than the book.
        const overflowing = await page.evaluate((tolerance) => {
          const selector = "p, h1, h2, h3, h4, h5, h6, li, blockquote, pre, table, figure, aside, div";
          return [...document.querySelectorAll(selector)]
            .filter((element) => element.scrollWidth > element.clientWidth + tolerance)
            .map((element) => ({
              tag: element.tagName.toLowerCase(),
              id: element.id || null,
              scrollWidth: element.scrollWidth,
              clientWidth: element.clientWidth,
              text: (element.textContent ?? "").trim().slice(0, 60),
            }));
        }, TOLERANCE_PX);

        expect(
          overflowing,
          `${document}: ${overflowing.length} blocks overflow their own width`,
        ).toEqual([]);
      });

      test(`dom_images_render_at_a_non_zero_size: ${document}`, async ({ page }) => {
        await page.goto(documentUrl(fixture.fixture, document));

        const images = await page.evaluate(() =>
          [...document.querySelectorAll("img")].map((image) => ({
            src: image.getAttribute("src"),
            alt: image.getAttribute("alt"),
            naturalWidth: image.naturalWidth,
            naturalHeight: image.naturalHeight,
            complete: image.complete,
          })),
        );

        for (const image of images) {
          expect(image.complete, `${document}: ${image.src} did not load`).toBe(true);
          expect(
            image.naturalWidth * image.naturalHeight,
            `${document}: ${image.src} rendered at zero size`,
          ).toBeGreaterThan(0);
          // Alt text is Tier 1's check as well, and it is asserted here because an empty `alt` means
          // "decorative" to a screen reader — a claim about the book, made by the markup.
          expect(image.alt, `${document}: ${image.src} has no alt text`).toBeTruthy();
        }
      });
    }
  });
}
