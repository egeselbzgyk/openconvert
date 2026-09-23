// The desktop UI in a real browser, under the CSP it ships with (Phase 12 rows 12.14–12.16).
//
// The built bundle (`apps/desktop/ui/dist`) is served with the policy from `tauri.conf.json`, and
// the Rust side is played in the page (`ui/tauri-mock.ts`) with a recorded run of the real engine
// on f09 and that run's report — so what is exercised is the app's own `tauriBackend()`, its own
// event handling and its own markup, and nothing is rendered that the engine did not say.
//
// Chromium on every pull request; WebKit — the engine of macOS's WKWebView and Linux's WebKitGTK —
// nightly, as the `webkit-ui` project.

import { readFileSync } from "node:fs";
import path from "node:path";

import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

import { serveUi } from "../ui/serve";
import { installTauriMock, type MockFixture } from "../ui/tauri-mock";

const ROOT = path.resolve(import.meta.dirname, "../../..");
const EN: Record<string, string> = JSON.parse(
  readFileSync(path.join(ROOT, "apps/desktop/ui/locales/en.json"), "utf8"),
);
const LINES = readFileSync(path.join(import.meta.dirname, "../ui/f09.events.ndjson"), "utf8")
  .split("\n")
  .filter((line) => line !== "");
const REPORT = JSON.parse(
  readFileSync(path.join(ROOT, "apps/desktop/ui/src/test/reports/f09.report.json"), "utf8"),
);
const HELLO = JSON.parse(LINES[0] ?? "{}");

const FIXTURE: MockFixture = {
  hello: HELLO,
  // What `ui_config` sends, from thresholds.toml on the Rust side.
  config: {
    appVersion: HELLO.engine_version,
    heartbeatTimeoutMs: 6000,
    cancelDeadlineMs: 2000,
    killAfterMs: 5000,
    copiedRevertMs: 2000,
    supervisorTickMs: 250,
    os: "linux",
    maxPages: 3000,
    maxMemoryBytes: 4294967296,
    aiTasksEnabled: 0,
  },
  lines: LINES,
  report: REPORT,
  picked: ["/books/f09_novel_structure.pdf"],
  pace: 20,
  // A registry that pins two models, as `models_list` would send it once `models.toml` is filled;
  // the pack registry as it ships, pinning nothing.
  models: {
    unavailable: null,
    rows: [
      {
        id: "qwen3-1.7b-q4_k_m",
        display_name: "Qwen3 1.7B (Q4_K_M)",
        tier: "default",
        is_default: true,
        installed: false,
        size_bytes: 1181116006,
        ram_estimate_bytes: 3221225472,
        cpu_expectation: "moderate",
        license: "Apache-2.0",
        license_path: null,
        warn: null,
        license_accepted: false,
        download: { state: "idle" },
      },
      {
        id: "qwen3.5-2b-q4_k_m",
        display_name: "Qwen3.5 2B (Q4_K_M, community quant)",
        tier: "experimental",
        is_default: false,
        installed: false,
        size_bytes: 1395864371,
        ram_estimate_bytes: 3758096384,
        cpu_expectation: "not yet measured",
        license: "Apache-2.0",
        license_path: null,
        warn: "Community quantisation of a new hybrid architecture.",
        license_accepted: false,
        download: { state: "idle" },
      },
    ],
  },
  packs: { unavailable: "model `validation` still has a placeholder in `license`", rows: [] },
  licenseText: readFileSync(path.join(ROOT, "crates/oc-net/licenses/Apache-2.0.txt"), "utf8"),
};

let server: { url: string; close: () => Promise<void> };
test.beforeAll(async () => {
  server = await serveUi();
});
test.afterAll(async () => {
  await server.close();
});

const mocked = new WeakSet<Page>();

/** Load the app afresh: a new "Rust side" with an empty queue, installed before the bundle runs. */
async function open(page: Page) {
  if (!mocked.has(page)) {
    await page.addInitScript(installTauriMock, FIXTURE);
    mocked.add(page);
  }
  await page.goto(server.url);
  await expect(page.locator(".oc-dropzone")).toBeVisible();
}

/** Nothing the shipped policy refused, on the whole visit. */
async function noCspViolations(page: Page) {
  const violations = await page.evaluate(
    () => (window as unknown as { __ocTest: { violations: string[] } }).__ocTest.violations,
  );
  expect(violations, "the shipped CSP refused something").toEqual([]);
}

/** Press Tab until the focused element's text is `label`; fail if it never is. */
async function tabTo(page: Page, label: string) {
  const seen: string[] = [];
  for (let presses = 0; presses < 40; presses += 1) {
    await page.keyboard.press("Tab");
    const text = await page.evaluate(() => document.activeElement?.textContent?.trim() ?? "");
    seen.push(text);
    if (text === label) return;
  }
  throw new Error(`Tab never reached ${JSON.stringify(label)}; it went ${JSON.stringify(seen)}`);
}

/** The screens row 12.15 names, reached the way a user reaches them. */
const SCREENS: Record<string, (page: Page) => Promise<void>> = {
  queue: async () => {},
  result: async (page) => {
    await page.getByRole("button", { name: EN["drop.select"] }).click();
    await expect(page.locator(".oc-row--expanded")).toBeVisible();
  },
  report: async (page) => {
    await SCREENS.result!(page);
    await page.getByRole("button", { name: EN["action.details"], exact: true }).click();
    await expect(page.locator(".oc-header__title")).toHaveText(EN["report.title"]!);
  },
  settings: async (page) => {
    await page.getByRole("button", { name: EN["settings.title"] }).click();
    await expect(page.locator(".oc-header__title")).toHaveText(EN["settings.title"]!);
  },
  // The one settings section with live controls of every kind: numbers, units, a danger button.
  "settings-advanced": async (page) => {
    await SCREENS.settings!(page);
    await page.locator(".oc-nav__item", { hasText: EN["settings.nav.advanced"] }).click();
    await expect(page.getByRole("button", { name: EN["settings.advanced.clear"] })).toBeVisible();
  },
  // The model manager: tags, facts, a licence shown in full before its download.
  models: async (page) => {
    await SCREENS.settings!(page);
    await page.locator(".oc-nav__item", { hasText: EN["settings.nav.models"] }).click();
    await page.getByRole("button", { name: EN["models.download"] }).first().click();
    await expect(page.locator(".oc-license")).toContainText("Apache License");
  },
  // The first-run route, opened from the card at the default model.
  firstrun: async (page) => {
    await page.getByRole("button", { name: EN["firstrun.setup"] }).click();
    await expect(page.locator(".oc-settings__title")).toHaveText(EN["firstrun.title"]!);
    await expect(page.locator(".oc-license")).toContainText("Apache License");
  },
};

/** 12.14 — tab to the drop zone → open a file → convert → open the report, no mouse. */
test("keyboard_only_flow_completes_a_conversion", async ({ page }) => {
  await open(page);

  // The drop zone's one Tab stop comes first (design decision 4: drop zone → queue → Settings).
  await tabTo(page, EN["drop.select"]!);
  await page.keyboard.press("Enter");

  // The recorded conversion runs; the finished book opens into its result.
  const row = page.locator(".oc-queue > li").first();
  await expect(row).toHaveClass(/oc-row--expanded/);
  await expect(row).toContainText(EN["queue.complete"]!);

  await tabTo(page, EN["action.details"]!);
  await page.keyboard.press("Enter");
  await expect(page.locator(".oc-header__title")).toHaveText(EN["report.title"]!);
  await expect(page.locator("main")).toContainText(REPORT.input.filename);

  // And back to the queue, still without a mouse.
  await tabTo(page, EN["queue.title"]!);
  await page.keyboard.press("Enter");
  await expect(page.locator(".oc-dropzone")).toBeVisible();
  await noCspViolations(page);
});

/** 12.12's screens by keyboard: the first-run card opens the default model with its licence shown,
    the download streams real progress to the end, and the way back to the queue is one key away. */
test("first_run_downloads_the_default_model_by_keyboard", async ({ page }) => {
  await open(page);
  await expect(page.locator(".oc-card--firstrun")).toContainText("~1.1 GB download");

  await tabTo(page, EN["firstrun.setup"]!);
  await page.keyboard.press("Enter");
  await expect(page.locator(".oc-settings__title")).toHaveText(EN["firstrun.title"]!);
  await expect(page.locator(".oc-license")).toContainText("TERMS AND CONDITIONS");

  await tabTo(page, "Accept license and download 1.1 GB");
  await page.keyboard.press("Enter");
  const bar = page.getByRole("progressbar");
  await expect(bar).toBeVisible();
  await expect(bar).toHaveAttribute("aria-valuetext", /of 1\.1 GB$/);
  await expect(page.locator(".oc-model__state--ok")).toHaveText(EN["models.installed"]!);

  await tabTo(page, EN["firstrun.back"]!);
  await page.keyboard.press("Enter");
  await expect(page.locator(".oc-dropzone")).toBeVisible();
  await expect(page.locator(".oc-card--firstrun"), "installed: not offered again").toHaveCount(0);
  await noCspViolations(page);
});

/** 12.15 — zero serious or critical axe violations on queue, result, report and settings. */
test("axe_has_no_serious_violations", async ({ page }) => {
  for (const [name, reach] of Object.entries(SCREENS)) {
    await open(page);
    await reach(page);
    const results = await new AxeBuilder({ page }).analyze();
    const serious = results.violations
      .filter((violation) => violation.impact === "serious" || violation.impact === "critical")
      .map((violation) => ({
        screen: name,
        id: violation.id,
        nodes: violation.nodes.map((node) => node.target.join(" ")),
      }));
    expect(serious, `${name}: serious or critical violations`).toEqual([]);
    await noCspViolations(page);
  }
});

/** 12.16 — contrast checks pass in both themes, on every screen. */
test("dark_and_light_render_without_contrast_failures", async ({ page }) => {
  for (const colorScheme of ["light", "dark"] as const) {
    await page.emulateMedia({ colorScheme });
    for (const [name, reach] of Object.entries(SCREENS)) {
      await open(page);
      await reach(page);
      const scheme = await page.evaluate(() => getComputedStyle(document.documentElement).colorScheme);
      expect(scheme, `${name} follows the ${colorScheme} scheme`).toBe(colorScheme);
      const results = await new AxeBuilder({ page }).withRules(["color-contrast"]).analyze();
      const failures = results.violations.flatMap((violation) =>
        violation.nodes.map((node) => `${node.target.join(" ")}: ${node.failureSummary ?? ""}`),
      );
      expect(failures, `${colorScheme} ${name}: contrast`).toEqual([]);
    }
  }
});
