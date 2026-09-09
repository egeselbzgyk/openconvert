import { describe, expect, it } from "vitest";

import {
  describeEngine,
  EngineError,
  handshake,
  parseEvents,
  type HelloEvent,
  type Spawner,
} from "./engine";

const APP_VERSION = "0.1.0";

/** A stub engine: the same shape Tauri's sidecar `Command` presents, without Tauri. */
function stubEngine(events: object[], code = 0): Spawner {
  return {
    async run() {
      return {
        code,
        stdout: "",
        stderr: events.map((event) => JSON.stringify(event)).join("\n") + "\n",
      };
    },
  };
}

function hello(overrides: Partial<HelloEvent> = {}): object {
  return {
    v: 1,
    t: "hello",
    seq: 0,
    ts_ms: 1_757_000_000_000,
    engine_version: APP_VERSION,
    ir_version: 1,
    protocol: 1,
    pdfium_version: "151.0.7881.0",
    capabilities: ["inspect"],
    ...overrides,
  };
}

describe("engine", () => {
  it("spawn_receives_hello", async () => {
    const spawner = stubEngine([hello(), { v: 1, t: "done", seq: 1, ts_ms: 1, status: "ok" }]);
    const received = await handshake(spawner, APP_VERSION);

    expect(received.t).toBe("hello");
    expect(received.seq).toBe(0);
    expect(received.engine_version).toBe(APP_VERSION);
    expect(received.pdfium_version).toBe("151.0.7881.0");

    // What the window renders (A0.6).
    expect(describeEngine(received)).toBe(
      "engine 0.1.0 · ir 1 · protocol 1 · pdfium 151.0.7881.0",
    );
  });

  it("refuses a staged engine whose version differs from the app's (A0.7)", async () => {
    const spawner = stubEngine([hello({ engine_version: "0.0.9" })]);
    await expect(handshake(spawner, APP_VERSION)).rejects.toMatchObject({
      name: "EngineError",
      kind: "version-mismatch",
    });
  });

  it("refuses a protocol or IR version it does not know", async () => {
    await expect(handshake(stubEngine([hello({ protocol: 2 })]), APP_VERSION)).rejects.toMatchObject(
      { kind: "protocol-mismatch" },
    );
    await expect(
      handshake(stubEngine([hello({ ir_version: 2 })]), APP_VERSION),
    ).rejects.toMatchObject({ kind: "ir-mismatch" });
  });

  it("refuses a stream that does not begin with hello", async () => {
    const notFirst = stubEngine([
      { v: 1, t: "warning", seq: 0, ts_ms: 1, code: "W_X" },
      hello(),
    ]);
    await expect(handshake(notFirst, APP_VERSION)).rejects.toMatchObject({ kind: "not-hello" });
    await expect(handshake(stubEngine([]), APP_VERSION)).rejects.toBeInstanceOf(EngineError);
  });

  it("stops parsing at the first line that is not an event", () => {
    const stderr =
      JSON.stringify(hello()) + "\nwarning: something else wrote here\n" + JSON.stringify(hello());
    // One event, not two: output from something that is not the engine is not silently
    // skipped over, because whatever produced it is usually the interesting part.
    expect(parseEvents(stderr)).toHaveLength(1);
    expect(parseEvents("")).toHaveLength(0);
  });
});
