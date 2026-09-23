import { flushSync, mount, unmount } from "svelte";
import { afterEach, describe, expect, it } from "vitest";

import App from "../../App.svelte";
import type { Report } from "../../lib/report";
import f08 from "../../test/reports/f08.report.json";
import { FakeBackend, settle } from "../../test/fake-backend";

let app: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (app !== null) unmount(app);
  app = null;
  document.body.innerHTML = "";
});

const button = (text: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === text);

describe("diagnostic bundle", () => {
  it("is written for the job, then reviewed — contents, sizes, what is not in it; nothing sent", async () => {
    const backend = new FakeBackend();
    backend.reports.set("job-1", f08 as unknown as Report);
    app = mount(App, { target: document.body, props: { backend, clock: () => 0 } });
    await settle();
    backend.change({ id: "job-1", input: "/b/a.pdf", output: "/b/a.epub", renamed: false, state: "running" });
    backend.line("job-1", { t: "done", status: "ok", report_path: "/b/a.epub.report.json" });
    await settle();
    flushSync();

    button("Export diagnostic bundle")?.click();
    await settle();
    flushSync();
    expect(backend.calls).toContainEqual(["export", "job-1"]);
    const text = document.querySelector(".oc-editor")?.textContent ?? "";
    expect(text).toContain("Diagnostic bundle saved");
    expect(text).toContain("openconvert-diagnostics-2026-09-23.zip");
    expect(text).toContain("OpenConvert did not send this anywhere.");
    expect(text).toContain("events.ndjson");
    expect(text).toContain("Not included: the PDF, the EPUB, your settings file, any API key.");
    expect(document.activeElement?.textContent).toBe("Done");

    button("Show in folder")?.click();
    expect(backend.calls).toContainEqual(["showBundle", null]);
    button("Done")?.click();
    flushSync();
    expect(document.querySelector(".oc-queue")).not.toBeNull();
  });

  it("a stale converter can still be reported from the blocking screen", async () => {
    const backend = new FakeBackend();
    backend.startupError = { kind: "protocol_mismatch", detail: { engine: 2, app: 1 } };
    app = mount(App, { target: document.body, props: { backend, clock: () => 0 } });
    await settle();
    flushSync();
    button("Export diagnostic bundle")?.click();
    await settle();
    flushSync();
    expect(backend.calls).toContainEqual(["export", null]);
    button("Done")?.click();
    flushSync();
    expect(document.querySelector('[role="alertdialog"]'), "back to the blocking screen").not.toBeNull();
  });
});
