/**
 * Everything the window asks of the Rust side, and everything it hears back.
 *
 * The webview's whole reach is the app's own commands (`src-tauri/src/main.rs`) plus listening for
 * its events — the capability grants nothing else (test 12.13). It never names an argument for the
 * engine, never builds a path the engine will read, and never opens a socket.
 *
 * An interface, so the app can be driven by a scripted double in component tests; the Playwright
 * specs instead replace Tauri's IPC underneath {@link tauriBackend}, so the real wiring is what
 * they exercise.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";

import type { Hello } from "./events";
import type { JobView } from "./jobstate";

/** The numbers the UI needs, from `thresholds.toml` via the Rust side — never literals here. */
export interface UiConfig {
  appVersion: string;
  heartbeatTimeoutMs: number;
  cancelDeadlineMs: number;
  killAfterMs: number;
  copiedRevertMs: number;
  supervisorTickMs: number;
  maxPages: number;
  maxMemoryBytes: number;
}

/** The Rust side's error: a kind the UI words, and a detail it never shows as the message. */
export interface UiError {
  kind: string;
  detail?: unknown;
}

export interface Enqueued {
  jobs: string[];
  skipped: string[];
}

/** Native drag and drop, as Tauri delivers it: absolute paths (Phase 12 detail 2). */
export type DropEvent =
  | { type: "enter"; paths: string[] }
  | { type: "over" }
  | { type: "drop"; paths: string[] }
  | { type: "leave" };

export type Unlisten = () => void;

export interface Backend {
  startup(): Promise<Hello>;
  config(): Promise<UiConfig>;
  rows(): Promise<JobView[]>;
  enqueue(paths: string[]): Promise<Enqueued>;
  cancel(job: string): Promise<void>;
  remove(job: string): Promise<void>;
  onLine(handler: (job: string, line: string) => void): Promise<Unlisten>;
  onJobChanged(handler: (view: JobView) => void): Promise<Unlisten>;
  onDragDrop(handler: (event: DropEvent) => void): Promise<Unlisten>;
  quit(): Promise<void>;
}

/** The real one, over Tauri's IPC. */
export function tauriBackend(): Backend {
  return {
    startup: () => invoke<Hello>("startup_status"),
    config: () => invoke<UiConfig>("ui_config"),
    rows: () => invoke<JobView[]>("queue_rows"),
    enqueue: (paths) => invoke<Enqueued>("enqueue", { paths }),
    cancel: (job) => invoke<void>("cancel", { job }),
    remove: (job) => invoke<void>("remove", { job }),
    onLine: (handler) =>
      listen<{ job: string; line: string }>("engine-line", (event) =>
        handler(event.payload.job, event.payload.line),
      ),
    onJobChanged: (handler) => listen<JobView>("job-changed", (event) => handler(event.payload)),
    onDragDrop: (handler) =>
      getCurrentWebview().onDragDropEvent((event) => {
        const payload = event.payload;
        switch (payload.type) {
          case "enter":
            handler({ type: "enter", paths: payload.paths });
            break;
          case "over":
            handler({ type: "over" });
            break;
          case "drop":
            handler({ type: "drop", paths: payload.paths });
            break;
          default:
            handler({ type: "leave" });
        }
      }),
    quit: () => invoke<void>("quit"),
  };
}
