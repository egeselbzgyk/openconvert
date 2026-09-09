/**
 * Talking to the engine (D13.1, D13.2).
 *
 * The app spawns the CLI and reads NDJSON events from its stderr. It never parses stderr
 * as prose and never infers the outcome from anything but the events and the exit code.
 *
 * The `Spawner` indirection exists so this module can be tested without Tauri: the real
 * implementation wraps `@tauri-apps/plugin-shell`'s sidecar `Command`, and the test passes
 * a stub. That is also what makes the version-handshake logic testable at all, and the
 * handshake is the point — RT A5.7 records a stale staged sidecar as a live footgun.
 */

/** The protocol version this UI speaks. A mismatch is fatal, never coerced (§2.3). */
export const PROTOCOL_VERSION = 1;

/** The IR version this UI understands. */
export const IR_VERSION = 1;

/** The common envelope every event carries. */
export interface EngineEvent {
  v: number;
  t: string;
  seq: number;
  ts_ms: number;
  [key: string]: unknown;
}

/** The first event of every run. */
export interface HelloEvent extends EngineEvent {
  t: "hello";
  engine_version: string;
  ir_version: number;
  protocol: number;
  pdfium_version: string;
  capabilities: string[];
}

/** Something that can run the engine and give back what it wrote to stderr. */
export interface Spawner {
  run(args: string[]): Promise<{ code: number | null; stderr: string; stdout: string }>;
}

export class EngineError extends Error {
  constructor(
    message: string,
    readonly kind:
      | "no-events"
      | "not-hello"
      | "protocol-mismatch"
      | "ir-mismatch"
      | "version-mismatch",
  ) {
    super(message);
    this.name = "EngineError";
  }
}

/**
 * Parse NDJSON, skipping blank lines and stopping at the first line that is not JSON.
 *
 * A non-JSON line means something wrote to stderr that is not the engine — a linker
 * warning, a sanitizer, a crash handler. Those are worth surfacing rather than silently
 * dropping, so parsing stops there and the caller sees a short event list.
 */
export function parseEvents(stderr: string): EngineEvent[] {
  const events: EngineEvent[] = [];
  for (const line of stderr.split("\n")) {
    const trimmed = line.trim();
    if (trimmed === "") continue;
    try {
      events.push(JSON.parse(trimmed) as EngineEvent);
    } catch {
      break;
    }
  }
  return events;
}

/**
 * Spawn the engine, read its `hello`, and check that it is the engine this app was built
 * against.
 *
 * `appVersion` is the app's own version. A staged sidecar left over from an earlier build
 * reports a different one, and the app must refuse to start rather than run a mismatched
 * pair — the failure it would otherwise produce arrives much later and looks like a bug in
 * the conversion (A0.7, RT A5.7).
 */
export async function handshake(spawner: Spawner, appVersion: string): Promise<HelloEvent> {
  const { stderr } = await spawner.run(["--version"]);
  const events = parseEvents(stderr);

  const first = events[0];
  if (first === undefined) {
    throw new EngineError("the engine produced no events", "no-events");
  }
  if (first.t !== "hello") {
    throw new EngineError(`the first event was ${first.t}, not hello`, "not-hello");
  }

  const hello = first as HelloEvent;
  if (hello.protocol !== PROTOCOL_VERSION) {
    throw new EngineError(
      `engine speaks protocol ${hello.protocol}, this app speaks ${PROTOCOL_VERSION}`,
      "protocol-mismatch",
    );
  }
  if (hello.ir_version !== IR_VERSION) {
    throw new EngineError(
      `engine writes IR ${hello.ir_version}, this app reads ${IR_VERSION}`,
      "ir-mismatch",
    );
  }
  if (hello.engine_version !== appVersion) {
    throw new EngineError(
      `staged engine is ${hello.engine_version}, this app is ${appVersion}; ` +
        "re-run `cargo run -p xtask -- stage-sidecars`",
      "version-mismatch",
    );
  }
  return hello;
}

/** The line the window shows once the handshake succeeds (A0.6). */
export function describeEngine(hello: HelloEvent): string {
  return `engine ${hello.engine_version} · ir ${hello.ir_version} · protocol ${hello.protocol} · pdfium ${hello.pdfium_version}`;
}
