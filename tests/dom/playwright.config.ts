// D7 / R6 §11 Layer 2: a real browser loading the emitted XHTML, asserting **structurally**.
//
// Three viewports, from IMPLEMENTATION_PLAN Phase 6 detail 7: 600×800 is a small e-reader, 390×844
// is a phone, 1024×768 a tablet in landscape. The narrow one is the one that finds things — a `<pre>`
// or a `<table>` that fits a tablet overflows a phone — and the wide one is there because a
// `width: 100%` mistake only shows when there is room to spare.
//
// **No screenshots, on any project.** A pixel comparison over a reflowable book would fail on a
// font-rendering difference between two runners and teach everyone to ignore the colour. Every
// assertion here is about the DOM: a measured overflow, an order, a resolved fragment, a rendered
// size. That is what makes these checks portable and worth gating on (D7).
//
// Chromium runs on every pull request; WebKit is nightly, because it is the engine every Apple
// reading system uses and also the slowest to install.

import { defineConfig, devices } from "@playwright/test";

const VIEWPORTS = [
  { name: "reader-600x800", width: 600, height: 800 },
  { name: "phone-390x844", width: 390, height: 844 },
  { name: "tablet-1024x768", width: 1024, height: 768 },
];

export default defineConfig({
  testDir: "./specs",
  // A book is a handful of small documents; a spec that needs more than this is stuck.
  timeout: 30_000,
  expect: { timeout: 5_000 },
  // Deterministic reporting order, and no flake-hiding retries: a DOM assertion that passes on the
  // second try is a race in the page, and the page is ours.
  fullyParallel: false,
  retries: 0,
  forbidOnly: !!process.env.CI,
  reporter: process.env.CI ? [["list"], ["junit", { outputFile: "results/junit.xml" }]] : "list",
  use: { screenshot: "off", video: "off", trace: "off" },
  projects: [
    ...VIEWPORTS.map((viewport) => ({
      name: `chromium-${viewport.name}`,
      use: {
        ...devices["Desktop Chrome"],
        viewport: { width: viewport.width, height: viewport.height },
      },
    })),
    // Nightly only: selected with `--project=webkit`, never part of the default run.
    {
      name: "webkit",
      use: { ...devices["Desktop Safari"], viewport: { width: 600, height: 800 } },
    },
  ],
});
