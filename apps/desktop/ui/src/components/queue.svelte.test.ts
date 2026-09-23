import { flushSync, mount, unmount } from "svelte";
import { afterEach, describe, expect, it, vi } from "vitest";

import App from "../App.svelte";
import { CONFIG, FakeBackend, settle } from "../test/fake-backend";

const job = { id: "job-1", input: "/books/moby-dick.pdf", output: "/books/moby-dick.epub", renamed: false, unlocked: false, rebuild: false };

let app: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (app !== null) unmount(app);
  app = null;
  document.body.innerHTML = "";
  vi.useRealTimers();
});

async function start(backend: FakeBackend, clock: () => number) {
  app = mount(App, { target: document.body, props: { backend, clock } });
  await settle();
  backend.change({ ...job, state: "running" });
  backend.line(job.id, { t: "hello", protocol: 1, engine_version: "0.1.0", ir_version: 1 });
  backend.line(job.id, { t: "job", job_id: job.id, pages: 214, phase: "started" });
  flushSync();
}

function bar(): HTMLElement | null {
  return document.querySelector('[role="progressbar"]');
}

describe("queue", () => {
  // 12.4 — the bar reflects progress{done,total} and never advances without an event.
  it("progress_events_drive_the_bar", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    let now = 0;
    const backend = new FakeBackend();
    await start(backend, () => now);

    backend.line(job.id, { t: "stage", name: "layout", phase: "begin" });
    flushSync();
    expect(bar(), "no count yet: a spinner, not a bar").toBeNull();
    expect(document.querySelector(".oc-spinner.is-spinning")).not.toBeNull();

    backend.line(job.id, { t: "progress", stage: "layout", done: 132, total: 214, unit: "pages" });
    flushSync();
    const first = bar();
    expect(first?.getAttribute("aria-valuenow")).toBe("132");
    expect(first?.getAttribute("aria-valuemax")).toBe("214");
    expect(first?.getAttribute("aria-valuetext")).toBe("132 of 214 pages");
    const fill = first?.querySelector<HTMLElement>(".oc-progress__fill");
    const width = fill?.style.width ?? "";
    expect(Number.parseFloat(width)).toBeCloseTo((132 / 214) * 100, 5);

    // Time passes and heartbeats arrive, but no progress: nothing moves.
    for (let second = 1; second <= 20; second += 1) {
      now = second * 1000;
      backend.line(job.id, { t: "heartbeat" });
      vi.advanceTimersByTime(CONFIG.supervisorTickMs * 4);
      flushSync();
    }
    expect(bar()?.getAttribute("aria-valuenow")).toBe("132");
    expect(fill?.style.width).toBe(width);

    backend.line(job.id, { t: "progress", stage: "layout", done: 214, total: 214, unit: "pages" });
    flushSync();
    expect(bar()?.getAttribute("aria-valuenow")).toBe("214");
    expect(Number.parseFloat(bar()?.querySelector<HTMLElement>(".oc-progress__fill")?.style.width ?? "")).toBe(100);
  });

  // 12.5 — more than the heartbeat timeout without a heartbeat flips the row; heartbeats restore it.
  it("missing_heartbeat_marks_not_responding", async () => {
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    let now = 0;
    const backend = new FakeBackend();
    await start(backend, () => now);
    backend.line(job.id, { t: "stage", name: "structure", phase: "begin" });
    backend.line(job.id, { t: "heartbeat" });
    flushSync();

    const row = () => document.querySelector<HTMLElement>(`[data-job="${job.id}"]`);
    now = CONFIG.heartbeatTimeoutMs;
    vi.advanceTimersByTime(CONFIG.supervisorTickMs);
    flushSync();
    expect(row()?.classList.contains("oc-row--stalled"), "exactly at the timeout: still fine").toBe(false);

    now = CONFIG.heartbeatTimeoutMs + 1;
    vi.advanceTimersByTime(CONFIG.supervisorTickMs);
    flushSync();
    expect(row()?.classList.contains("oc-row--stalled")).toBe(true);
    expect(row()?.textContent).toContain("Not responding");
    expect(row()?.querySelector('[role="alert"]')).not.toBeNull();
    expect(row()?.querySelector("button")?.textContent, "only Cancel is offered").toBe("Cancel");

    // It recovers on its own when heartbeats return (design decision 7).
    backend.line(job.id, { t: "heartbeat" });
    flushSync();
    expect(row()?.classList.contains("oc-row--stalled")).toBe(false);
  });
});
