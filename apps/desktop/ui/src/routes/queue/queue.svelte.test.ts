import { flushSync, mount, unmount } from "svelte";
import { afterEach, describe, expect, it } from "vitest";

import App from "../../App.svelte";
import { FakeBackend, settle } from "../../test/fake-backend";

let app: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (app !== null) unmount(app);
  app = null;
  document.body.innerHTML = "";
});

async function start(backend = new FakeBackend()) {
  app = mount(App, { target: document.body, props: { backend, clock: () => 0 } });
  await settle();
  flushSync();
  return backend;
}

const zone = () => document.querySelector<HTMLElement>(".oc-dropzone");
const status = () => document.querySelector('[role="status"][aria-live="polite"]')?.textContent ?? "";

describe("queue route", () => {
  it("says what a drop would do before it happens, and never rejects the whole drop", async () => {
    const backend = await start();
    backend.drop({ type: "enter", paths: ["/b/a.pdf", "/b/B.PDF", "/b/notes.docx"] });
    flushSync();
    expect(zone()?.classList.contains("is-dragover-mixed")).toBe(true);
    expect(zone()?.textContent).toContain("Release to add 2 PDFs");
    expect(zone()?.textContent).toContain("notes.docx is not a PDF and will be skipped.");

    backend.drop({ type: "leave" });
    flushSync();
    expect(zone()?.classList.contains("is-dragover-mixed")).toBe(false);

    backend.drop({ type: "enter", paths: ["/b/a.pdf"] });
    backend.drop({ type: "drop", paths: ["/b/a.pdf"] });
    await settle();
    flushSync();
    expect(backend.calls).toContainEqual(["enqueue", ["/b/a.pdf"]]);
  });

  it("Select PDF… is the drop zone's one Tab stop and opens the native picker", async () => {
    const backend = new FakeBackend();
    backend.picked = ["/b/picked.pdf"];
    await start(backend);
    const buttons = zone()?.querySelectorAll("button") ?? [];
    expect(buttons.length).toBe(1);
    (buttons[0] as HTMLButtonElement).click();
    await settle();
    expect(backend.calls).toContainEqual(["pick", null]);
    expect(backend.calls).toContainEqual(["enqueue", ["/b/picked.pdf"]]);
  });

  it("arrows move between rows; Delete removes a waiting row; Remove all waiting asks once", async () => {
    const backend = await start();
    for (const [index, name] of ["a", "b", "c"].entries()) {
      backend.change({
        id: `job-${index + 1}`,
        input: `/b/${name}.pdf`,
        output: `/b/${name}.epub`,
        renamed: false,
        ...(index === 0 ? { state: "running" as const } : { state: "queued" as const, position: index + 1 }),
      });
    }
    flushSync();
    const rows = () => [...document.querySelectorAll<HTMLElement>(".oc-queue > li")];
    expect(rows().map((row) => row.tabIndex)).toEqual([0, -1, -1]);

    rows()[0]?.focus();
    rows()[0]?.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    flushSync();
    expect(document.activeElement).toBe(rows()[1]);
    expect(rows().map((row) => row.tabIndex)).toEqual([-1, 0, -1]);

    rows()[1]?.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", bubbles: true }));
    await settle();
    flushSync();
    expect(backend.calls).toContainEqual(["remove", "job-2"]);
    expect(status()).toContain("b.pdf removed.");

    backend.change({ id: "job-4", input: "/b/d.pdf", output: "/b/d.epub", renamed: false, state: "queued", position: 3 });
    flushSync();
    const bulk = [...document.querySelectorAll("button")].find((b) => b.textContent === "Remove all waiting…");
    bulk?.click();
    flushSync();
    const dialog = document.querySelector('[role="dialog"]');
    expect(dialog?.textContent).toContain("Remove 2 waiting files from the queue?");
    expect(document.activeElement?.textContent, "opens on the safe button").toBe("Cancel");
  });
});
