/**
 * A queue row as a pure function of what the engine and the supervisor reported.
 *
 * Every visible piece of job state — stage label, progress fraction, "not responding", the result
 * — is a projection of engine events and queue changes (UI_UX §8). Nothing here advances on a
 * timer: a bar moves when a `progress` event says so, a stage changes when a `stage` event says
 * so, and the only thing time decides is that a heartbeat is *overdue* — which is the one
 * question heartbeats exist to answer (RT C2).
 *
 * Pure and synchronous, so the rules are tested without a window or a clock; `jobs.svelte.ts`
 * holds the reactive copy.
 */

import type { Provider } from "./backend";
import type { Done, Event, Fatal, Warning } from "./events";
import type { Report } from "./report";

/** The user-facing steps (UI_UX §2.2), in order. `repairing` is shown only when it runs. */
export const STEPS = ["analyzing", "extracting", "reconstructing", "building", "checking"] as const;
export type Step = (typeof STEPS)[number] | "repairing";

/** Engine stage → user-facing step (UI_UX §2.2). Unknown stages map to nothing. */
export const STAGE_STEP: Readonly<Record<string, Step>> = {
  inspect: "analyzing",
  ingest: "analyzing",
  text: "extracting",
  furniture: "extracting",
  layout: "reconstructing",
  paragraphs: "reconstructing",
  structure: "reconstructing",
  document: "reconstructing",
  epub: "building",
  validate: "checking",
  report: "checking",
  repair: "repairing",
};

/** Why a job that asked for AI assistance converted without it (`src-tauri/src/ai.rs`). */
export type AiUnavailable = "no_model" | "no_server" | "server_failed" | "no_endpoint" | "plain_http";

/** AI assistance for a job, as the queue knows it; `null` with AI off. */
export interface AiView {
  provider: Provider;
  unavailable: AiUnavailable | null;
}

/** The queue's view of a job, as the Rust side sends it (`job-changed`). */
export type QueueState =
  | { state: "queued"; position: number }
  | { state: "preparing" }
  | { state: "running" }
  | { state: "cancelling" }
  | { state: "cancelled_before_start" }
  | { state: "exited"; code: number | null }
  | { state: "failed_to_start"; error: { kind: string; detail?: unknown } };

export type JobView = {
  id: string;
  input: string;
  output: string;
  renamed: boolean;
  /** This run was given the password typed on the row (never the password itself). */
  unlocked: boolean;
  /** This run applies the user's corrections to a book already converted ("Fix and rebuild"). */
  rebuild: boolean;
  /** Always sent by the Rust side; `null` with AI off. */
  ai?: AiView | null;
} & QueueState;

export type Phase = "queued" | "running" | "cancelling" | "cancelled" | "complete" | "failed";

export interface ProgressCount {
  step: Step;
  stage: string;
  done: number;
  total: number;
  unit: string;
}

export interface Row {
  id: string;
  input: string;
  output: string;
  renamed: boolean;
  /** This run was given a password, so a password failure means that password was wrong. */
  unlocked: boolean;
  /** "Fix and rebuild": the user's corrections, applied to a book already converted. */
  rebuild: boolean;
  /** AI assistance for this job, as the queue reported it; `null` with AI off. */
  ai: AiView | null;
  /** Its turn has come and it waits for the app's model server to load. */
  preparing: boolean;
  /** How many `llm` events the engine sent: one per model call, cached ones included (D13.2). */
  llmCalls: number;
  phase: Phase;
  /** Waiting position, the running job counted as #1 (design decision 6). */
  position: number | null;
  /** The step whose stage began last; `null` before any stage. */
  current: Step | null;
  /** Steps that have finished, in the order they finished. */
  finished: Step[];
  /** Whether `repair` ran at all, so "Repairing" is shown only then. */
  repaired: boolean;
  /** The latest count of the current step, exactly as the engine reported it. */
  progress: ProgressCount | null;
  /** When the last heartbeat (or the start of the run) was seen, in ms. */
  lastSignalMs: number | null;
  /** More than the heartbeat timeout since the last heartbeat. */
  stalled: boolean;
  /** How many heartbeats have arrived: each one is one pulse. */
  beats: number;
  pages: number | null;
  warnings: Warning[];
  done: Done | null;
  fatal: Fatal | null;
  /** The engine's exit code, once the queue saw it exit. */
  exit: number | null | undefined;
  /** report.json, once a completed job's report has been read; `"unavailable"` if it could not be. */
  report: Report | "unavailable" | null;
  /** The completed row is expanded into its result (route `result`). */
  expanded: boolean;
  /** "Open in reader" failed: no EPUB reader is set up (result.html §5). */
  noReader: boolean;
}

export function newRow(view: JobView): Row {
  return applyView(
    {
      id: view.id,
      input: view.input,
      output: view.output,
      renamed: view.renamed,
      unlocked: view.unlocked,
      rebuild: view.rebuild,
      ai: view.ai ?? null,
      preparing: false,
      llmCalls: 0,
      phase: "queued",
      position: null,
      current: null,
      finished: [],
      repaired: false,
      progress: null,
      lastSignalMs: null,
      stalled: false,
      beats: 0,
      pages: null,
      warnings: [],
      done: null,
      fatal: null,
      exit: undefined,
      report: null,
      expanded: false,
      noReader: false,
    },
    view,
  );
}

/** The queue's state change, folded in. The engine's own events win where both speak. */
export function applyView(row: Row, view: JobView): Row {
  const next: Row = {
    ...row,
    input: view.input,
    output: view.output,
    renamed: view.renamed,
    unlocked: view.unlocked,
    rebuild: view.rebuild,
    ai: view.ai ?? null,
    preparing: view.state === "preparing",
  };
  switch (view.state) {
    case "queued":
      return { ...next, phase: "queued", position: view.position };
    case "preparing":
      // Under way — Cancel applies — though no engine has started yet.
      return { ...next, phase: row.phase === "queued" ? "running" : row.phase, position: null };
    case "running":
      return { ...next, phase: row.phase === "queued" ? "running" : row.phase, position: null };
    case "cancelling":
      return { ...next, phase: settled(row) ? row.phase : "cancelling", position: null };
    case "cancelled_before_start":
      return { ...next, phase: "cancelled", position: null };
    case "failed_to_start":
      return {
        ...next,
        phase: "failed",
        position: null,
        fatal: row.fatal ?? synthetic("E_START", view.error.kind),
      };
    case "exited": {
      const exited = { ...next, position: null, exit: view.code, stalled: false };
      if (settled(row)) return exited;
      // The engine exited without a final event: killed at the cancel deadline, or crashed.
      if (row.phase === "cancelling") return { ...exited, phase: "cancelled" };
      return {
        ...exited,
        phase: "failed",
        fatal: row.fatal ?? synthetic("E_EXIT", `exit ${String(view.code)}`),
      };
    }
  }
}

/** Whether the engine already told this row how it ended. */
function settled(row: Row): boolean {
  return row.phase === "complete" || row.phase === "failed" || row.phase === "cancelled";
}

function synthetic(code: string, message: string): Fatal {
  return { v: 1, t: "fatal", seq: -1, ts_ms: 0, code, message };
}

/** One engine event, folded in at time `now` (ms). */
export function applyEvent(row: Row, event: Event, now: number): Row {
  switch (event.t) {
    case "hello":
      return { ...row, lastSignalMs: now, stalled: false };
    case "job":
      return {
        ...row,
        phase: row.phase === "queued" ? "running" : row.phase,
        pages: event.pages,
        lastSignalMs: row.lastSignalMs ?? now,
      };
    case "heartbeat":
      return { ...row, lastSignalMs: now, stalled: false, beats: row.beats + 1 };
    case "stage": {
      const step = STAGE_STEP[event.name];
      if (step === undefined) return row;
      if (event.phase === "begin") {
        const finished =
          row.current !== null && row.current !== step && !row.finished.includes(row.current)
            ? [...row.finished, row.current]
            : row.finished;
        return {
          ...row,
          current: step,
          finished: finished.filter((done) => done !== step),
          repaired: row.repaired || step === "repairing",
          // A new step starts with no count: it shows a spinner until one arrives.
          progress: row.current === step ? row.progress : null,
        };
      }
      return row;
    }
    case "progress": {
      const step = STAGE_STEP[event.stage] ?? row.current;
      if (step === null || step === undefined) return row;
      return {
        ...row,
        progress: {
          step,
          stage: event.stage,
          done: Math.min(event.done, event.total),
          total: event.total,
          unit: event.unit,
        },
      };
    }
    case "warning":
      return { ...row, warnings: [...row.warnings, event] };
    case "done":
      return {
        ...row,
        done: event,
        stalled: false,
        phase:
          event.status === "ok" ? "complete" : event.status === "cancelled" ? "cancelled" : "failed",
        finished:
          event.status === "ok"
            ? [...STEPS, ...(row.repaired ? (["repairing"] as const) : [])]
            : row.finished,
        current: event.status === "ok" ? null : row.current,
        // A finished book opens into its result (motion.md, "Row expands into result").
        expanded: event.status === "ok" ? true : row.expanded,
      };
    case "fatal":
      return { ...row, fatal: event, phase: "failed", stalled: false };
    case "llm":
      return { ...row, llmCalls: row.llmCalls + 1 };
  }
}

/** Mark the row not responding once more than `timeoutMs` has passed since the last heartbeat. */
export function checkHeartbeat(row: Row, now: number, timeoutMs: number): Row {
  // A job waiting for the model server has no engine yet, so nothing to hear from.
  const watching = (row.phase === "running" && !row.preparing) || row.phase === "cancelling";
  if (!watching || row.lastSignalMs === null) return row.stalled ? { ...row, stalled: false } : row;
  const stalled = now - row.lastSignalMs > timeoutMs;
  return stalled === row.stalled ? row : { ...row, stalled };
}

/** The steps a rebuild runs when it resumes after `structure` (A12.4b). */
const REBUILD_STEPS: readonly Step[] = ["reconstructing", "building", "checking"];

/**
 * The steps a row shows, in order: the five, plus "Repairing" when it ran. A rebuild shows only
 * the steps it runs (result.html §2, "only the stages that run") — unless the engine had to start
 * from the PDF after all, which its first stage says.
 */
export function visibleSteps(row: Row): Step[] {
  const upstream = [row.current, ...row.finished].some(
    (step) => step === "analyzing" || step === "extracting",
  );
  const steps = row.rebuild && !upstream ? [...REBUILD_STEPS] : [...STEPS];
  return row.repaired ? [...steps, "repairing"] : steps;
}

/** A step's state in the StageList. */
export function stepState(row: Row, step: Step): "done" | "current" | "pending" | "repair" {
  if (row.current === step && row.phase !== "complete") {
    return step === "repairing" ? "repair" : "current";
  }
  if (row.finished.includes(step)) return "done";
  return "pending";
}

/**
 * Why AI assistance did not help this job, when it was asked for and could not: the queue's reason
 * (no model, no server, …), or the engine's `W_LLM_UNAVAILABLE`. `null` when it was not asked for
 * or answered. The banner is non-modal (UI_UX §4): the book converted either way.
 */
export function aiUnavailable(row: Row): AiUnavailable | "engine" | null {
  if (row.ai === null) return null;
  if (row.ai.unavailable !== null) return row.ai.unavailable;
  return row.warnings.some((warning) => warning.code === "W_LLM_UNAVAILABLE") ? "engine" : null;
}

/**
 * Whether the job stopped because AI assistance would have sent text to a host nobody consented to:
 * the engine's `E_CONSENT_REQUIRED`, or the queue refusing to start it (`consent_required`). The UI
 * answers with the consent dialog (D10, PHASE 11's hand-off).
 */
export function needsConsent(row: Row): boolean {
  if (row.phase !== "failed" || row.fatal === null) return false;
  return (
    row.fatal.code === "E_CONSENT_REQUIRED" || (row.fatal.code === "E_START" && row.fatal.message === "consent_required")
  );
}

/** The cap a stage that ran out of time is named by — in the `fatal` message and as the failure
    report's `cap` (`limits.stage_deadline_secs`, PHASE 14 detail 6). */
export const DEADLINE_CAP = "stage_deadline_secs";

/**
 * Whether the job stopped because a stage ran past its deadline: `E_LIMIT_EXCEEDED` naming
 * {@link DEADLINE_CAP}. Unlike the size limits, the answer is the time limit in Settings › Advanced.
 * The message is matched only to choose the sentence; it is never shown (R10 §6.20).
 */
export function ranOutOfTime(fatal: Fatal | null): boolean {
  return fatal !== null && fatal.code === "E_LIMIT_EXCEEDED" && fatal.message.startsWith(DEADLINE_CAP);
}

/** Seconds since the last heartbeat, for "no signal for {s} seconds". */
export function silentSeconds(row: Row, now: number): number {
  return row.lastSignalMs === null ? 0 : Math.floor((now - row.lastSignalMs) / 1000);
}
