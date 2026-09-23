import { describe, expect, it } from "vitest";

import { parseLine, ProtocolError, PROTOCOL_VERSION } from "./events";

function line(event: Record<string, unknown>): string {
  return JSON.stringify({ v: PROTOCOL_VERSION, seq: 0, ts_ms: 1, ...event });
}

describe("events", () => {
  it("parses every event kind of protocol 1", () => {
    for (const t of ["hello", "job", "stage", "progress", "warning", "llm", "heartbeat", "done", "fatal"]) {
      const extra = t === "hello" ? { protocol: PROTOCOL_VERSION } : {};
      expect(parseLine(line({ t, ...extra }))?.t).toBe(t);
    }
  });

  it("hard-errors on another protocol version, in the envelope or in hello", () => {
    expect(() => parseLine(JSON.stringify({ v: 2, t: "heartbeat", seq: 0, ts_ms: 1 }))).toThrow(
      ProtocolError,
    );
    expect(() => parseLine(line({ t: "hello", protocol: 2 }))).toThrow(ProtocolError);
  });

  it("does not take a foreign line for an event", () => {
    expect(parseLine("")).toBeNull();
    expect(parseLine("warning: something else wrote here")).toBeNull();
    expect(parseLine("[1,2]")).toBeNull();
    expect(parseLine(line({ t: "telemetry" }))).toBeNull();
  });
});
