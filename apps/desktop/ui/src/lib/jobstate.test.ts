import { describe, expect, it } from "vitest";

import type { Event } from "./events";
import { applyEvent, applyView, checkHeartbeat, newRow, stepState, type Row } from "./jobstate";

const view = { id: "job-1", input: "/b/book.pdf", output: "/b/book.epub", renamed: false, unlocked: false, rebuild: false };

function event(e: Record<string, unknown>): Event {
  return { v: 1, seq: 0, ts_ms: 0, ...e } as Event;
}

function running(): Row {
  let row = newRow({ ...view, state: "running" });
  row = applyEvent(row, event({ t: "hello", protocol: 1 }), 0);
  return applyEvent(row, event({ t: "job", pages: 10, phase: "started" }), 0);
}

describe("jobstate", () => {
  it("maps engine stages onto the five user-facing steps", () => {
    let row = running();
    row = applyEvent(row, event({ t: "stage", name: "ingest", phase: "begin" }), 1);
    expect(stepState(row, "analyzing")).toBe("current");
    row = applyEvent(row, event({ t: "stage", name: "text", phase: "begin" }), 2);
    row = applyEvent(row, event({ t: "stage", name: "furniture", phase: "begin" }), 3);
    expect(stepState(row, "analyzing")).toBe("done");
    expect(stepState(row, "extracting")).toBe("current");
    expect(stepState(row, "building")).toBe("pending");
    row = applyEvent(row, event({ t: "done", status: "ok", report_path: "/r" }), 4);
    expect(row.phase).toBe("complete");
    expect(stepState(row, "checking")).toBe("done");
  });

  it("shows Repairing only when repair ran", () => {
    let row = running();
    expect(row.repaired).toBe(false);
    row = applyEvent(row, event({ t: "stage", name: "repair", phase: "begin" }), 1);
    expect(row.repaired).toBe(true);
    expect(stepState(row, "repairing")).toBe("repair");
  });

  it("a cancelled run is cancelled by the engine's word, or by the exit after a kill", () => {
    let row = applyView(running(), { ...view, state: "cancelling" });
    expect(row.phase).toBe("cancelling");
    const saidSo = applyEvent(row, event({ t: "done", status: "cancelled" }), 1);
    expect(saidSo.phase).toBe("cancelled");
    row = applyView(row, { ...view, state: "exited", code: null });
    expect(row.phase).toBe("cancelled");
  });

  it("an exit with no final event is a failure, never a success", () => {
    const row = applyView(running(), { ...view, state: "exited", code: 101 });
    expect(row.phase).toBe("failed");
    expect(row.fatal?.code).toBe("E_EXIT");
  });

  it("the heartbeat watchdog only watches running rows", () => {
    const queued = newRow({ ...view, state: "queued", position: 2 });
    expect(checkHeartbeat(queued, 1_000_000, 6000).stalled).toBe(false);
  });
});
