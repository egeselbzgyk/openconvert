import { flushSync, mount, unmount } from "svelte";
import { afterEach, describe, expect, it } from "vitest";

import App from "../App.svelte";
import { renderWarning } from "../lib/i18n";
import { setLanguage } from "../lib/locale.svelte";
import type { Report } from "../lib/report";
import f08 from "../test/reports/f08.report.json";
import { FakeBackend, settle } from "../test/fake-backend";

let app: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (app !== null) unmount(app);
  app = null;
  document.body.innerHTML = "";
  setLanguage("en");
});

const job = { id: "job-1", input: "/books/f08_footnotes.pdf", output: "/books/f08_footnotes.epub", renamed: false, unlocked: false };

/** Run a job to completion through the fake backend, with a real engine report. */
async function complete(report: Report, language: "en" | "de" | "tr" = "en", backend = new FakeBackend()) {
  backend.saved = { ...backend.saved, language };
  backend.reports.set(job.id, report);
  app = mount(App, { target: document.body, props: { backend, clock: () => 0 } });
  await settle();
  backend.change({ ...job, state: "running" });
  backend.line(job.id, { t: "hello", protocol: 1 });
  backend.line(job.id, { t: "done", status: "ok", report_path: "/books/f08_footnotes.epub.report.json" });
  await settle();
  flushSync();
  return backend;
}

const row = () => document.querySelector<HTMLElement>(`[data-job="${job.id}"]`);

describe("result panel", () => {
  it("renders report.json: output, validation, warnings, facts", async () => {
    await complete(f08 as unknown as Report);
    const text = row()?.textContent ?? "";
    expect(row()?.classList.contains("oc-row--expanded"), "a finished book opens into its result").toBe(true);
    expect(text).toContain("f08_footnotes.epub");
    expect(text).toContain("Validation: passed with warnings (1)");
    const warning = renderWarning("en", "W_LOW_RETENTION", { retention: "0.968", floor: "0.980" });
    expect(text).toContain(warning ?? "missing");
    expect(text).toContain("96.8%");
    expect(text, "3 notes, all linked").toContain("3 of 3");
    expect(text).toContain("Not run");
    expect(text).toContain("Deterministic processing");
    expect(text, "AI decisions only when AI was on").not.toContain("AI-assisted");
  });

  it("A12.3: the same warning in German and Turkish, never English", async () => {
    await complete(f08 as unknown as Report, "de");
    const german = renderWarning("de", "W_LOW_RETENTION", { retention: "0.968", floor: "0.980" }) ?? "";
    expect(row()?.textContent).toContain(german);
    expect(german).toContain("0,968");
    expect(row()?.textContent).toContain("Validierung: bestanden, mit Warnungen (1)");
    unmount(app!);
    app = null;
    document.body.innerHTML = "";

    await complete(f08 as unknown as Report, "tr");
    const turkish = renderWarning("tr", "W_LOW_RETENTION", { retention: "0.968", floor: "0.980" }) ?? "";
    expect(row()?.textContent).toContain(turkish);
    expect(row()?.textContent).not.toContain(renderWarning("en", "W_LOW_RETENTION", { retention: "0.968", floor: "0.980" }) ?? "x");
  });

  it("an invalid book is shown as plainly as a passing one, and still offered", async () => {
    await complete({ ...(f08 as unknown as Report), status: "invalid" });
    const text = row()?.textContent ?? "";
    expect(text).toContain("Validation: invalid");
    expect(text).toContain("The EPUB was saved and can still be opened.");
    expect(text).toContain("saved, marked invalid");
    expect(text).toContain("Open in reader");
  });

  it("no reader: the app explains and offers the folder; Enter collapses the row", async () => {
    const backend = new FakeBackend();
    backend.noReader = true;
    await complete(f08 as unknown as Report, "en", backend);
    const open = [...(row()?.querySelectorAll("button") ?? [])].find((b) => b.textContent === "Open in reader");
    open?.click();
    await settle();
    flushSync();
    expect(row()?.textContent).toContain("No EPUB reader is set up on this computer.");

    row()?.focus();
    row()?.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    flushSync();
    expect(row()?.classList.contains("oc-row--expanded")).toBe(false);
    expect(row()?.querySelector('[aria-expanded="false"]')).not.toBeNull();
  });
});
