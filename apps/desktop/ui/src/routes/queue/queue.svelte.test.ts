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
        unlocked: false,
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

    backend.change({ id: "job-4", input: "/b/d.pdf", output: "/b/d.epub", renamed: false, unlocked: false, state: "queued", position: 3 });
    flushSync();
    const bulk = [...document.querySelectorAll("button")].find((b) => b.textContent === "Remove all waiting…");
    bulk?.click();
    flushSync();
    const dialog = document.querySelector('[role="dialog"]');
    expect(dialog?.textContent).toContain("Remove 2 waiting files from the queue?");
    expect(document.activeElement?.textContent, "opens on the safe button").toBe("Cancel");
  });

  it("a locked PDF asks for its password on the row, hands it over once, and says when it was wrong", async () => {
    const backend = await start();
    const locked = { id: "job-1", input: "/b/annual-report-locked.pdf", output: "/b/annual-report-locked.epub", renamed: false };
    backend.change({ ...locked, unlocked: false, state: "running" });
    backend.line("job-1", { t: "fatal", code: "E_PASSWORD_REQUIRED", message: "the PDF needs a password" });
    backend.change({ ...locked, unlocked: false, state: "exited", code: 2 });
    flushSync();

    const row = () => document.querySelector<HTMLElement>(".oc-queue > li");
    const field = () => document.querySelector<HTMLInputElement>('input[type="password"]');
    const button = () => row()?.querySelector<HTMLButtonElement>('button[type="submit"]');
    expect(row()?.textContent).toContain("This PDF is password-protected.");
    expect(row()?.textContent).toContain("An empty password was tried first.");
    expect(row()?.textContent).not.toContain("Export diagnostic bundle");
    const label = document.querySelector(`label[for="${field()?.id}"]`);
    expect(label?.textContent).toBe("Password for annual-report-locked.pdf");
    expect(row()?.textContent).toContain("Used for this job only, never saved.");
    expect(button()?.disabled, "an empty password was already tried").toBe(true);

    const input = field() as HTMLInputElement;
    input.value = "hunter22";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    flushSync();
    expect(button()?.disabled).toBe(false);
    button()?.click();
    await settle();
    flushSync();

    expect(backend.unlocked).toEqual([["job-1", "hunter22"]]);
    expect(backend.calls.some(([, arg]) => JSON.stringify(arg).includes("hunter22")), "only unlock carries it").toBe(false);
    const rows = [...document.querySelectorAll<HTMLElement>(".oc-queue > li")];
    expect(rows.map((item) => item.dataset.job), "the locked row is replaced").toEqual(["job-1-unlocked"]);
    expect(document.activeElement, "focus follows the new job").toBe(rows[0]);
    expect(field(), "no password field while it converts").toBeNull();

    // The typed password did not open it either: the field comes back, marked, with the reason.
    backend.line("job-1-unlocked", { t: "fatal", code: "E_PASSWORD_REQUIRED", message: "the PDF needs a password" });
    backend.change({ ...locked, id: "job-1-unlocked", unlocked: true, state: "exited", code: 2 });
    flushSync();
    expect(field()?.value, "nothing typed is kept").toBe("");
    expect(field()?.getAttribute("aria-invalid")).toBe("true");
    const error = document.getElementById(field()?.getAttribute("aria-describedby") ?? "");
    expect(error?.textContent).toBe("That password didn't open the file. Try again.");
    expect(row()?.textContent).not.toContain("An empty password was tried first.");
  });
});
