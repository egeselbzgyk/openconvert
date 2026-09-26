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
        rebuild: false,
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

    backend.change({ id: "job-4", input: "/b/d.pdf", output: "/b/d.epub", renamed: false, unlocked: false, rebuild: false, state: "queued", position: 3 });
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
    backend.change({ ...locked, unlocked: false, rebuild: false, state: "running" });
    backend.line("job-1", { t: "fatal", code: "E_PASSWORD_REQUIRED", message: "the PDF needs a password" });
    backend.change({ ...locked, unlocked: false, rebuild: false, state: "exited", code: 2 });
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
    backend.change({ ...locked, id: "job-1-unlocked", unlocked: true, rebuild: false, state: "exited", code: 2 });
    flushSync();
    expect(field()?.value, "nothing typed is kept").toBe("");
    expect(field()?.getAttribute("aria-invalid")).toBe("true");
    const error = document.getElementById(field()?.getAttribute("aria-describedby") ?? "");
    expect(error?.textContent).toBe("That password didn't open the file. Try again.");
    expect(row()?.textContent).not.toContain("An empty password was tried first.");
  });

  // PHASE 11's hand-off: `E_CONSENT_REQUIRED` (or the queue refusing to start for that reason)
  // re-opens the consent dialog, naming the host — never a generic error — and Allow converts the
  // book again with the consent in its spec.
  it("a job stopped for want of consent re-opens the consent dialog, and Allow converts it again", async () => {
    const backend = new FakeBackend();
    backend.saved = {
      ...backend.saved,
      aiEnabled: true,
      provider: "custom",
      custom: { endpoint: "https://llm.example.org/v1", model: "qwen3-8b", apiKeyFile: null, consent: null },
    };
    await start(backend);
    const job = { id: "job-1", input: "/b/novel.pdf", output: "/b/novel.epub", renamed: false, unlocked: false, rebuild: false };
    backend.change({ ...job, ai: { provider: "custom", unavailable: null }, state: "running" });
    backend.line(job.id, { t: "hello", protocol: 1 });
    backend.line(job.id, {
      t: "fatal",
      code: "E_CONSENT_REQUIRED",
      message: "`llm.example.org` is not this computer: with --ai, text from the book would be sent there.",
    });
    await settle();
    flushSync();
    const dialog = document.querySelector('[role="dialog"]');
    expect(dialog?.querySelector("h2")?.textContent).toBe("Send document text to llm.example.org?");
    expect(dialog?.textContent).toContain("qwen3-8b");

    [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Allow llm.example.org")?.click();
    await settle();
    flushSync();
    expect(backend.saved.custom.consent?.host).toBe("llm.example.org");
    expect(backend.calls, "converted again").toContainEqual(["enqueue", ["/b/novel.pdf"]]);
    expect(document.querySelector(".oc-dropzone, .oc-queue"), "back on the queue").not.toBeNull();
  });

  // A stage that ran past `limits.stage_deadline_secs` is not a size limit: the row says which limit
  // it was and where to raise it.
  it("a step that ran out of time says so, and where to raise the limit", async () => {
    const backend = await start();
    const job = { id: "job-1", input: "/b/atlas.pdf", output: "/b/atlas.epub", renamed: false, unlocked: false, rebuild: false };
    backend.change({ ...job, state: "running" });
    backend.line(job.id, {
      t: "fatal",
      code: "E_LIMIT_EXCEEDED",
      message: "stage_deadline_secs exceeded: stage `layout` ran past 1800 s",
    });
    backend.change({ ...job, state: "exited", code: 1 });
    flushSync();
    const row = document.querySelector('[data-job="job-1"]');
    expect(row?.textContent).toContain("A step of the conversion ran longer than the time limit.");
    expect(row?.textContent).toContain('You can raise "Time limit per step" in Settings › Advanced');
    expect([...(row?.querySelectorAll("button") ?? [])].map((b) => b.textContent)).toContain("Convert again");

    // A size limit is still a size limit.
    const big = { ...job, id: "job-2", input: "/b/big.pdf", output: "/b/big.epub" };
    backend.change({ ...big, state: "running" });
    backend.line(big.id, { t: "fatal", code: "E_LIMIT_EXCEEDED", message: "max_pages exceeded: 5000 pages, limit 3000" });
    backend.change({ ...big, state: "exited", code: 1 });
    flushSync();
    expect(document.querySelector('[data-job="job-2"]')?.textContent).toContain("exceeds a size limit");
  });

  it("the row itself says consent is needed and offers the dialog again", async () => {
    const backend = await start();
    const job = { id: "job-2", input: "/b/essay.pdf", output: "/b/essay.epub", renamed: false, unlocked: false, rebuild: false };
    backend.change({
      ...job,
      ai: { provider: "custom", unavailable: null },
      state: "failed_to_start",
      error: { kind: "consent_required", detail: { host: "llm.example.org" } },
    });
    await settle();
    flushSync();
    // The dialog opened by itself once (settings route); back on the queue, the row explains.
    [...document.querySelectorAll<HTMLButtonElement>(".oc-header button")].find((b) => b.textContent?.includes("Queue"))?.click();
    flushSync();
    const row = document.querySelector('[data-job="job-2"]');
    expect(row?.textContent).toContain("Sending text to another computer needs your consent");
    expect(row?.textContent).toContain("Nothing was sent.");
    expect([...(row?.querySelectorAll("button") ?? [])].map((b) => b.textContent)).toContain("Review consent…");
  });
});
