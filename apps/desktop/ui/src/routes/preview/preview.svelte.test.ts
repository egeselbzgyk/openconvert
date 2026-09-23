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

describe("preview route", () => {
  it("jumps from a warning's page link to that page, labelled approximate, in a sandboxed frame", async () => {
    const backend = new FakeBackend();
    backend.reports.set("job-1", f03 as unknown as Report);
    app = mount(App, { target: document.body, props: { backend, clock: () => 0 } });
    await settle();
    backend.change({ id: "job-1", input: "/b/f03.pdf", output: "/b/f03.epub", renamed: false, state: "running" });
    backend.line("job-1", { t: "done", status: "ok", report_path: "/b/f03.epub.report.json" });
    await settle();
    flushSync();

    const link = [...document.querySelectorAll<HTMLButtonElement>(".oc-pagelink")].find((b) => b.textContent === "p. 1");
    expect(link?.getAttribute("aria-label")).toBe("Go to page 1 in the preview");
    link?.click();
    await settle();
    flushSync();

    const label = document.querySelector('[role="note"]');
    expect(label?.textContent).toContain("Approximate preview");
    const frame = () => document.querySelector("iframe");
    expect(frame()?.getAttribute("sandbox"), "no script, opaque origin").toBe("");
    expect(frame()?.getAttribute("src")).toBe("ocpreview://localhost/job-1/text/c0001.xhtml#page0");
    expect(document.querySelector(".oc-preview__bar")?.textContent).toContain("p. 1 of 3 (source page)");

    [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.getAttribute("aria-label") === "Next page")?.click();
    flushSync();
    expect(frame()?.getAttribute("src")).toBe("ocpreview://localhost/job-1/text/c0002.xhtml#page1");
    expect(document.querySelector('.oc-preview__toc [aria-current="true"]')?.textContent).toBe("Chapter One");

    document.querySelector(".oc-preview")?.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    flushSync();
    expect(frame()?.getAttribute("src")).toBe("ocpreview://localhost/job-1/text/c0002.xhtml#page2");

    [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Close preview")?.click();
    flushSync();
    expect(document.querySelector(".oc-preview")).toBeNull();
    expect(document.querySelector(".oc-queue")).not.toBeNull();
  });
});
