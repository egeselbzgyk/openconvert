import { flushSync, mount, unmount } from "svelte";
import { afterEach, describe, expect, it } from "vitest";

import App from "../App.svelte";
import { renderWarning } from "../lib/i18n";
import { setLanguage } from "../lib/locale.svelte";
import type { JobView } from "../lib/jobstate";
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

const job = { id: "job-1", input: "/books/f08_footnotes.pdf", output: "/books/f08_footnotes.epub", renamed: false, unlocked: false, rebuild: false };

/** Run a job to completion through the fake backend, with a real engine report. */
async function complete(
  report: Report,
  language: "en" | "de" | "tr" = "en",
  backend = new FakeBackend(),
  extra: { ai?: JobView["ai"]; warnings?: Array<Record<string, unknown>> } = {},
) {
  backend.saved = { ...backend.saved, language };
  backend.reports.set(job.id, report);
  app = mount(App, { target: document.body, props: { backend, clock: () => 0 } });
  await settle();
  backend.change({ ...job, ai: extra.ai ?? null, state: "running" });
  backend.line(job.id, { t: "hello", protocol: 1 });
  for (const warning of extra.warnings ?? []) backend.line(job.id, { t: "warning", ...warning });
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

  // UI_UX §4: AI assistance on and unusable — the book converts deterministically and a non-modal
  // banner says so, with the reason the app knows.
  it("a job that asked for AI and could not use it says so, and converted deterministically", async () => {
    await complete(f08 as unknown as Report, "en", new FakeBackend(), {
      ai: { provider: "builtin", unavailable: "no_model" },
    });
    const banner = row()?.querySelector(".oc-banner--warn[role=status]");
    expect(banner?.textContent).toBe("AI assistance unavailable this run — converted deterministically. No model is installed.");
    expect(row()?.textContent).toContain("Deterministic processing");
    unmount(app!);
    app = null;
    document.body.innerHTML = "";

    // The engine's own finding (`W_LLM_UNAVAILABLE`): the same banner, and the warning says why.
    await complete(f08 as unknown as Report, "de", new FakeBackend(), {
      ai: { provider: "ollama", unavailable: null },
      warnings: [{ code: "W_LLM_UNAVAILABLE", severity: "warn", args: { reason: "Ollama serves no model" } }],
    });
    expect(row()?.querySelector(".oc-banner--warn[role=status]")?.textContent).toBe(
      "KI-Unterstützung in diesem Durchlauf nicht verfügbar – deterministisch konvertiert.",
    );
  });

  // UI_UX §2.3: with AI on, the result counts the model's decisions instead of "Deterministic".
  it("with AI on the result counts the decisions a model made", async () => {
    const report = {
      ...(f08 as unknown as Report),
      ai: { provider: "local_sidecar", model_id: "qwen3-1.7b", all_tasks: false, calls: 0, cached_calls: 0, llm_ms: 0 },
    };
    await complete(report, "en", new FakeBackend(), { ai: { provider: "builtin", unavailable: null } });
    expect(row()?.textContent).toContain("AI-assisted decisions: 0");
    expect(row()?.textContent).not.toContain("Deterministic processing");
    expect(row()?.querySelector(".oc-banner--warn[role=status]")).toBeNull();
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
