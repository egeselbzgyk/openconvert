import { flushSync, mount, unmount } from "svelte";
import { afterEach, describe, expect, it } from "vitest";

import App from "../../App.svelte";
import { renderWarning } from "../../lib/i18n";
import { setLanguage } from "../../lib/locale.svelte";
import type { Report } from "../../lib/report";
import f09 from "../../test/reports/f09.report.json";
import rebuiltReport from "../../test/reports/f09.rebuilt.report.json";
import staleReport from "../../test/reports/f09.stale.report.json";
import { FakeBackend, settle } from "../../test/fake-backend";

let app: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (app !== null) unmount(app);
  app = null;
  document.body.innerHTML = "";
  setLanguage("en");
});

const job = {
  id: "job-1",
  input: "/books/f09_novel_structure.pdf",
  output: "/books/f09_novel_structure.epub",
  renamed: false,
  unlocked: false,
  rebuild: false,
};

/** Run `id` to completion through the fake backend, with a real engine report. */
async function complete(backend: FakeBackend, id: string, report: Report, rebuild = false) {
  backend.reports.set(id, report);
  backend.change({ ...job, id, rebuild, state: "running" });
  backend.line(id, { t: "hello", protocol: 1 });
  backend.line(id, { t: "done", status: "ok", report_path: `${job.output}.report.json` });
  await settle();
  flushSync();
}

async function start() {
  const backend = new FakeBackend();
  app = mount(App, { target: document.body, props: { backend, clock: () => 0 } });
  await settle();
  await complete(backend, job.id, f09 as unknown as Report);
  return backend;
}

const button = (label: string) =>
  [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === label);
const type = (input: HTMLInputElement | HTMLSelectElement, value: string) => {
  input.value = value;
  input.dispatchEvent(new Event(input instanceof HTMLSelectElement ? "change" : "input", { bubbles: true }));
  flushSync();
};

describe("metadata and TOC editors", () => {
  it("Edit metadata sends only what changed, and the row becomes the rebuild", async () => {
    const backend = await start();
    button("Edit metadata")?.click();
    flushSync();

    expect(document.querySelector("h2")?.textContent).toBe("Edit metadata · f09_novel_structure.epub");
    expect(document.activeElement?.tagName, "focus lands on the editor's heading").toBe("H2");
    const title = document.querySelector<HTMLInputElement>(".oc-editor input:not([aria-label])");
    expect(title?.value).toBe("A Short Novel");
    expect(button("Fix and rebuild")?.disabled, "nothing changed yet").toBe(true);

    type(title!, "A Short Novel, Corrected");
    button("Add author")?.click();
    await settle();
    flushSync();
    const second = document.querySelector<HTMLInputElement>('input[aria-label="Author 2"]');
    expect(document.activeElement, "focus goes to the new author").toBe(second);
    type(second!, "Ada Reader");
    const language = document.querySelector<HTMLSelectElement>("#editor-language");
    expect([...(language?.options ?? [])].map((option) => option.textContent)).toEqual([
      "Not detected (und)",
      "English (en)",
      "German (de)",
      "Turkish (tr)",
    ]);
    type(language!, "en");

    button("Fix and rebuild")?.click();
    await settle();
    flushSync();
    expect(backend.corrected).toEqual([
      [
        "job-1",
        {
          metadata: { title: "A Short Novel, Corrected", authors: ["O. Convert", "Ada Reader"], language: "en" },
          toc: [],
        },
      ],
    ]);

    // Back on the queue, the row is the rebuild, running only the steps after structure.
    const rows = [...document.querySelectorAll<HTMLElement>(".oc-queue > li")];
    expect(rows.map((item) => item.dataset.job)).toEqual(["job-1-rebuilt"]);
    expect(document.activeElement, "focus follows the row").toBe(rows[0]);
    backend.line("job-1-rebuilt", { t: "stage", name: "document", phase: "begin" });
    flushSync();
    expect(rows[0]?.textContent).toContain("Fix and rebuild");
    expect(rows[0]?.textContent).toContain("Reconstructing, from cache");
    const steps = [...rows[0]!.querySelectorAll(".oc-step__label")].map((step) => step.textContent);
    expect(steps, "only the stages that run").toEqual(["Reconstructing", "Building", "Checking"]);

    // Done: the result says what the corrections did, from the rebuilt book's own report.
    await complete(backend, "job-1-rebuilt", rebuiltReport as unknown as Report, true);
    const text = document.querySelector('[data-job="job-1-rebuilt"]')?.textContent ?? "";
    expect(text).toMatch(/Complete· rebuilt in [\d.]+ s with your corrections/);
    expect(text).toContain("Applied: title, 2 authors, 1 heading. The rest of the book is unchanged.");
    expect(text).toContain("replaced the previous version");
  });

  it("Review TOC renames and re-levels headings, marks what changed, and counts it", async () => {
    const backend = await start();
    button("Review TOC")?.click();
    flushSync();

    const items = () => [...document.querySelectorAll<HTMLElement>(".oc-toc__item")];
    expect(items().map((item) => item.querySelector("input")?.value)).toEqual(
      (f09 as unknown as Report).document.toc.map((entry) => entry.title),
    );
    const chapterOne = items().find((item) => item.querySelector("input")?.value === "Chapter One")!;
    expect(chapterOne.querySelector("input")?.getAttribute("aria-label")).toBe("Heading, page 3");
    type(chapterOne.querySelector("input")!, "I. The Beginning");
    const chapterTwo = items().find((item) => item.querySelector("input")?.value === "Chapter Two")!;
    type(chapterTwo.querySelector("select")!, "2");

    expect(items().filter((item) => item.classList.contains("is-changed")).length).toBe(2);
    expect(chapterTwo.dataset.level).toBe("2");
    button("Fix and rebuild (2 changes)")?.click();
    await settle();
    const toc = (f09 as unknown as Report).document.toc;
    const id = (title: string) => toc.find((entry) => entry.title === title)?.heading;
    expect(backend.corrected).toEqual([
      [
        "job-1",
        {
          toc: [
            { heading: id("Chapter One"), title: "I. The Beginning" },
            { heading: id("Chapter Two"), level: 2 },
          ],
        },
      ],
    ]);
  });

  it("Cancel leaves the book alone; refused corrections are said plainly", async () => {
    const backend = await start();
    button("Review TOC")?.click();
    flushSync();
    button("Cancel")?.click();
    flushSync();
    expect(backend.corrected).toEqual([]);
    expect(document.querySelector('[data-job="job-1"]')).not.toBeNull();

    await complete(backend, "job-2", staleReport as unknown as Report, true);
    const text = document.querySelector('[data-job="job-2"]')?.textContent ?? "";
    expect(text).toContain("converted without your saved corrections");
    const warning = staleReport.warnings.find((w) => w.code === "W_OVERRIDES_STALE")!;
    const sentence = renderWarning("en", warning.code, warning.args) ?? "missing";
    expect(sentence).toContain("IR 0");
    expect(document.querySelector('[data-job="job-2"] [role="alert"]')?.textContent).toContain(sentence);
  });
});
