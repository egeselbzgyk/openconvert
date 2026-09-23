import { flushSync, mount, unmount } from "svelte";
import { afterEach, describe, expect, it } from "vitest";

import App from "../../App.svelte";
import type { Report } from "../../lib/report";
import f03 from "../../test/reports/f03.report.json";
import { FakeBackend, settle } from "../../test/fake-backend";

let app: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (app !== null) unmount(app);
  app = null;
  document.body.innerHTML = "";
});

describe("report route", () => {
  it("renders the report: counts, checks, warnings by page, timing, ledger, run details", async () => {
    const backend = new FakeBackend();
    backend.reports.set("job-1", f03 as unknown as Report);
    app = mount(App, { target: document.body, props: { backend, clock: () => 0 } });
    await settle();
    backend.change({ id: "job-1", input: "/b/f03_image_only.pdf", output: "/b/f03_image_only.epub", renamed: false, unlocked: false, rebuild: false, state: "running" });
    backend.line("job-1", { t: "done", status: "ok", report_path: "/b/f03_image_only.epub.report.json" });
    await settle();
    flushSync();

    [...document.querySelectorAll("button")].find((b) => b.textContent === "Details")?.click();
    flushSync();
    const view = document.querySelector(".oc-report");
    const text = view?.textContent ?? "";
    expect(text).toContain("f03_image_only.pdf");
    expect(text).toContain("What's in the book");
    expect(text).toContain("Built-in check");
    expect(text).toContain("not run: the validation pack arrives in a later version");
    expect(text).toContain("Time per step");
    expect(text).toContain("Building and checking");
    expect(text).toContain("scanned");
    // The page-anchored warnings carry their page links, in page order.
    const pages = [...(view?.querySelectorAll(".oc-pagelink") ?? [])].map((link) => link.textContent);
    expect(pages.length).toBeGreaterThan(0);
    expect(pages[0]).toBe("p. 1");
    // Every timeline segment's width is a real elapsed time, set through the CSSOM.
    const segments = [...(view?.querySelectorAll<HTMLElement>(".oc-timeline__seg") ?? [])];
    expect(segments.length).toBeGreaterThan(0);
    expect(segments.every((segment) => segment.style.flexGrow !== "" || segment.style.flex !== "")).toBe(true);
    expect(document.querySelector(".oc-header")?.textContent).toContain("Conversion report");
  });

  // PHASE 11's hand-off: the report page says which adapter and model answered, and — when text left
  // this computer — the consent that named the host, when it was given.
  it("names the provider and model that answered, and the consent text was sent under", async () => {
    const backend = new FakeBackend();
    const report = {
      ...(f03 as unknown as Report),
      ai: { provider: "openai_compatible", model_id: "qwen3-8b-instruct", all_tasks: false, calls: 3, cached_calls: 1, llm_ms: 812 },
      consent: { host: "llm.example.org", granted_at: "2026-09-23T10:15:00Z", scope: "run" },
    };
    backend.reports.set("job-1", report);
    backend.saved = { ...backend.saved, language: "de" };
    app = mount(App, { target: document.body, props: { backend, clock: () => 0 } });
    await settle();
    backend.change({ id: "job-1", input: "/b/f03.pdf", output: "/b/f03.epub", renamed: false, unlocked: false, rebuild: false, state: "running" });
    backend.line("job-1", { t: "done", status: "ok", report_path: "/b/f03.epub.report.json" });
    await settle();
    flushSync();
    [...document.querySelectorAll("button")].find((b) => b.textContent === "Details")?.click();
    flushSync();
    const text = document.querySelector(".oc-report")?.textContent ?? "";
    expect(text).toContain("Beantwortet von");
    expect(text).toContain("ein OpenAI-kompatibler Server · qwen3-8b-instruct");
    expect(text).toContain("3 gefragt, 1 aus dem Zwischenspeicher");
    expect(text).toContain("Text aus diesem Buch wurde mit Ihrer Zustimmung an llm.example.org gesendet");
    expect(text, "AI was on").not.toContain("KI-Unterstützung war für diesen Auftrag aus");
  });
});
