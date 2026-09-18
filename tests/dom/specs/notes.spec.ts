// Row 6.15: clicking each `noteref` reveals a `footnote` with the matching id.
//
// Tier 1 already asserts the bijection over the markup, and this asserts the thing the markup cannot:
// that a reader who taps the marker **arrives somewhere they can read**. A footnote inside a
// `hidden` container, or behind a `display: none` rule the stylesheet applies to `aside`, satisfies
// every structural check and shows the reader nothing.
//
// Clicking rather than navigating by URL, deliberately: the click is what a reader does, and it
// exercises the anchor's own `href` — including the relative path, which is where the RSC-007 defect
// Phase 5 found would have shown up.

import { expect, test } from "@playwright/test";

import { documentUrl, fixtures } from "./fixtures";

for (const fixture of fixtures()) {
  // A book with no footnotes has nothing to assert here, and a spec that passed vacuously on every
  // fixture would be a spec nobody could tell was working.
  if (fixture.noterefs.length === 0) {
    continue;
  }

  test.describe(fixture.fixture, () => {
    test("dom_every_noteref_resolves", async ({ page }) => {
      const documents = [...new Set(fixture.noterefs.map((ref) => ref.from))];

      for (const from of documents) {
        await page.goto(documentUrl(fixture.fixture, from));

        const anchors = page.locator('a[*|type="noteref"]');
        const count = await anchors.count();
        expect(
          count,
          `${from}: the manifest lists note references here and the DOM has none`,
        ).toBeGreaterThan(0);

        for (let index = 0; index < count; index += 1) {
          // Re-resolved each iteration: a click may navigate, and a locator from the previous
          // document is stale after it.
          await page.goto(documentUrl(fixture.fixture, from));
          const anchor = page.locator('a[*|type="noteref"]').nth(index);
          const href = await anchor.getAttribute("href");
          expect(href, `${from}: a noteref with no href`).toBeTruthy();
          const fragment = (href ?? "").split("#")[1] ?? "";
          expect(fragment, `${from}: ${href} has no fragment`).not.toBe("");

          await anchor.click();

          // `CSS.escape` is a browser global and this runs in Node, so the attribute selector is
          // what escapes the id: the emitter writes `n0001`-shaped ids, but a selector built by
          // concatenation would break the day one of them started with a digit.
          const target = page.locator(`[id="${fragment}"]`);
          await expect(
            target,
            `${from}: clicking ${href} did not reach a footnote`,
          ).toHaveCount(1);

          // The note is a `footnote`, and it is visible. Both matter: a marker that lands on a
          // paragraph is a broken pairing, and one that lands on something `display: none` is a
          // footnote the reader cannot read.
          const role = await target.getAttribute("epub:type");
          expect(role, `${from}: ${href} points at something that is not a footnote`).toContain(
            "footnote",
          );
          await expect(target, `${from}: ${href} reaches a hidden footnote`).toBeVisible();
        }
      }
    });

    test("dom_every_footnote_is_reachable", async ({ page }) => {
      // The other direction of the bijection, in the browser: a footnote nothing points at is a
      // detection failure hiding behind a link that happens to resolve.
      const targets = new Set(fixture.noterefs.map((ref) => `${ref.to}#${ref.fragment}`));

      for (const document of fixture.spine) {
        await page.goto(documentUrl(fixture.fixture, document));
        const ids = await page.evaluate(() =>
          [...document.querySelectorAll('aside[*|type="footnote"]')].map(
            (element) => element.id,
          ),
        );
        for (const id of ids) {
          expect(
            targets.has(`${document}#${id}`),
            `${document}: the footnote #${id} is not the target of any note reference`,
          ).toBe(true);
        }
      }
    });
  });
}
