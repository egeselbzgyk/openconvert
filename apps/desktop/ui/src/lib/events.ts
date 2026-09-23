/**
 * The engine's NDJSON events, as the UI reads them (IMPLEMENTATION_PLAN §2.3, D13.2).
 *
 * Every visible piece of job state is a projection of these (UI_UX §8). The UI never parses
 * stderr prose and never infers an outcome from anything but these events and the exit code.
 *
 * **A protocol version this UI does not speak is a hard error**, not a degraded UI: `parseLine`
 * throws {@link ProtocolError}, and the app answers with a blocking error screen (test 12.3).
 */

/** The protocol this UI speaks. The engine puts its own in every line as `v`. */
export const PROTOCOL_VERSION = 1;
/** The IR version this UI understands (`hello.ir_version`). */
export const IR_VERSION = 1;

interface Envelope {
  v: number;
  seq: number;
  ts_ms: number;
}

export interface Hello extends Envelope {
  t: "hello";
  engine_version: string;
  ir_version: number;
  protocol: number;
  pdfium_version: string;
  capabilities: string[];
}
export interface JobStarted extends Envelope {
  t: "job";
  job_id: string | null;
  input_sha256: string;
  pages: number;
  phase: "started";
}
export interface Stage extends Envelope {
  t: "stage";
  name: string;
  phase: "begin" | "end";
  elapsed_ms?: number;
}
export interface Progress extends Envelope {
  t: "progress";
  stage: string;
  done: number;
  total: number;
  unit: string;
}
export interface Warning extends Envelope {
  t: "warning";
  code: string;
  severity: "info" | "warn" | "error";
  args: Record<string, unknown>;
  block_ids?: string[];
  page?: number;
}
export interface Llm extends Envelope {
  t: "llm";
  call_id: string;
  purpose: string;
  cached: boolean;
  tokens_in: number;
  tokens_out: number;
  ms: number;
}
export interface Heartbeat extends Envelope {
  t: "heartbeat";
}
export interface Done extends Envelope {
  t: "done";
  status: "ok" | "failed" | "cancelled";
  report_path?: string;
  output_path?: string | null;
}
export interface Fatal extends Envelope {
  t: "fatal";
  code: string;
  message: string;
  backtrace_id?: string;
}

export type Event = Hello | JobStarted | Stage | Progress | Warning | Llm | Heartbeat | Done | Fatal;

const KINDS: ReadonlySet<string> = new Set([
  "hello",
  "job",
  "stage",
  "progress",
  "warning",
  "llm",
  "heartbeat",
  "done",
  "fatal",
]);

/** A stream this UI must not interpret. */
export class ProtocolError extends Error {
  constructor(
    message: string,
    readonly engine: number | null,
  ) {
    super(message);
    this.name = "ProtocolError";
  }
}

/**
 * One line of the engine's stderr.
 *
 * - blank, or not a JSON object: `null` — something other than the engine wrote there (a
 *   dynamic-linker notice, a sanitizer); it is not an event and it is not trusted as one;
 * - a `v` other than {@link PROTOCOL_VERSION}: throws {@link ProtocolError};
 * - a `hello` whose `protocol` differs: throws {@link ProtocolError};
 * - an unknown `t` under the right `v`: `null` — a newer engine may add an event kind, and the
 *   protocol's major version is what says whether the UI must understand it.
 */
export function parseLine(line: string): Event | null {
  const trimmed = line.trim();
  if (trimmed === "") return null;
  let value: unknown;
  try {
    value = JSON.parse(trimmed);
  } catch {
    return null;
  }
  if (typeof value !== "object" || value === null || Array.isArray(value)) return null;
  const record = value as Record<string, unknown>;
  if (record.v !== PROTOCOL_VERSION) {
    const engine = typeof record.v === "number" ? record.v : null;
    throw new ProtocolError(
      `the converter speaks protocol ${String(record.v)}, this app speaks ${PROTOCOL_VERSION}`,
      engine,
    );
  }
  if (typeof record.t !== "string" || !KINDS.has(record.t)) return null;
  if (record.t === "hello" && record.protocol !== PROTOCOL_VERSION) {
    throw new ProtocolError(
      `the converter speaks protocol ${String(record.protocol)}, this app speaks ${PROTOCOL_VERSION}`,
      typeof record.protocol === "number" ? record.protocol : null,
    );
  }
  return record as unknown as Event;
}
