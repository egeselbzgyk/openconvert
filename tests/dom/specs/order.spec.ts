// Row 6.14: the DOM's sequence equals the nav's order.
//
// The nav is the table of contents a reading system draws, and the documents are what the reader
// meets scrolling. When they disagree, tapping "Chapter Three" lands somewhere else — and nothing in
// EPUB's content model forbids it: both documents are valid, the fragments all resolve, and the book
// is wrong. This is the check that catches it.
//
// **The nav is read in the browser, not from the manifest.** Both sides of the comparison then come
// from the same parser the reader's software uses, and a spec that read one side out of a JSON file
// produced by our own Rust could not fail when the nav itself was wrong. The manifest is used only
// for the spine, which is a list of documents a browser cannot enumerate from inside one of them.
//
// **A nav entry is not always a heading.** The unheaded preamble — everything printed before the
// first heading — becomes a front-matter section of its own so that nothing is lost (PIPELINE §8),
// and the nav names that *section*, which has no heading inside it. So the claim is stated over nav
// *targets*: every one resolves in the document it names, and they occur in the same relative order
// the nav lists them in. The heading-specific claim, that levels never skip, is separate below.

import { expect, test } from "@playwright/test";

import { documentUrl, fixtures } from "./fixtures";

interface Target {
  document: string;
  fragment: string;
  text: string;
}

for (const fixture of fixtures()) {
  test.describe(fixture.fixture, () => {
    test("dom_heading_order_matches_nav", async ({ page }) => {
      // The `toc` nav's own anchors, in document order, as a browser parses them.
      await page.goto(documentUrl(fixture.fixture, "nav.xhtml"));
      const nav: Target[] = await page.evaluate(() => {
        const toc = document.querySelector('nav[*|type="toc"]');
        if (toc === null) {
          return [];
        }
        return [...toc.querySelectorAll("a[href]")].map((anchor) => {
          const href = anchor.getAttribute("href") ?? "";
          const [path, fragment] = href.split("#");
          return {
            document: path,
            fragment: fragment ?? "",
            text: (anchor.textContent ?? "").replace(/\s+/g, " ").trim(),
          };
        });
      });
      expect(nav.length, `${fixture.fixture}: the toc nav has no entries`).toBeGreaterThan(0);

      // For each spine document, the ids it carries, in document order.
      const idsByDocument = new Map<string, string[]>();
      for (const document of fixture.spine) {
        await page.goto(documentUrl(fixture.fixture, document));
        idsByDocument.set(
          document,
          await page.evaluate(() =>
            [...document.querySelectorAll("[id]")].map((element) => element.id),
          ),
        );
      }

      // Every nav target resolves, in the document the nav names.
      const missing = nav.filter((item) => {
        const ids = idsByDocument.get(item.document);
        return ids === undefined || (item.fragment !== "" && !ids.includes(item.fragment));
      });
      expect(
        missing.map((item) => `${item.document}#${item.fragment}`),
        `${fixture.fixture}: nav targets that do not resolve in the DOM`,
      ).toEqual([]);

      // And they occur in the order the nav lists them: spine position first, then position within
      // the document. Strictly increasing, because two nav entries pointing at one element would be
      // a table of contents with the same place in it twice.
      let previous = -1;
      for (const item of nav) {
        const spineIndex = fixture.spine.indexOf(item.document);
        const ids = idsByDocument.get(item.document) ?? [];
        const within = item.fragment === "" ? 0 : ids.indexOf(item.fragment);
        const position = spineIndex * 100_000 + within;
        expect(
          position,
          `${fixture.fixture}: ${item.document}#${item.fragment} ("${item.text}") comes before ` +
            `the nav entry that precedes it`,
        ).toBeGreaterThan(previous);
        previous = position;
      }

      // The headings the nav does name are in the same order as the nav names them. Compared on the
      // id and never on the label: an outline entry says "3" where the page says "Chapter 3", and
      // comparing text would fail on a book that is right.
      const navTargets = new Set(nav.map((item) => `${item.document}#${item.fragment}`));
      const headingOrder: string[] = [];
      for (const document of fixture.spine) {
        await page.goto(documentUrl(fixture.fixture, document));
        const ids = await page.evaluate(() =>
          [...document.querySelectorAll("h1, h2, h3, h4, h5, h6")].map(
            (element) => element.id || element.closest("[id]")?.id || "",
          ),
        );
        for (const id of ids) {
          const key = `${document}#${id}`;
          if (navTargets.has(key)) {
            headingOrder.push(key);
          }
        }
      }
      const navHeadingOrder = nav
        .map((item) => `${item.document}#${item.fragment}`)
        .filter((key) => headingOrder.includes(key));
      expect(
        headingOrder,
        `${fixture.fixture}: the DOM's headings are not in the nav's order`,
      ).toEqual(navHeadingOrder);
    });

    test("dom_heading_levels_never_skip", async ({ page }) => {
      // The same claim `oc-validate`'s structural validator makes over the markup, asserted where a
      // screen reader would meet it: a level that jumps from `h1` to `h3` is announced as a missing
      // section, and the browser is where the document's final heading tree exists.
      let previous: number | null = null;
      for (const document of fixture.spine) {
        await page.goto(documentUrl(fixture.fixture, document));
        const levels = await page.evaluate(() =>
          [...document.querySelectorAll("h1, h2, h3, h4, h5, h6")].map((element) =>
            Number(element.tagName.slice(1)),
          ),
        );
        for (const level of levels) {
          if (previous !== null) {
            expect(
              level,
              `${document}: h${previous} is followed by h${level}, skipping a level`,
            ).toBeLessThanOrEqual(previous + 1);
          }
          previous = level;
        }
      }
    });
  });
}
