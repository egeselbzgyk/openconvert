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
    backend.change({ id: "job-1", input: "/b/f03_image_only.pdf", output: "/b/f03_image_only.epub", renamed: false, state: "running" });
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
    expect(text).toContain("not run: the validation pack isn't installed");
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
});
