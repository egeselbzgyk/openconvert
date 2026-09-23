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
import type { Report } from "./report";

/** The numbers the UI needs, from `thresholds.toml` via the Rust side — never literals here. */
export interface UiConfig {
  appVersion: string;
  heartbeatTimeoutMs: number;
  cancelDeadlineMs: number;
  killAfterMs: number;
  copiedRevertMs: number;
  supervisorTickMs: number;
  /** `std::env::consts::OS`: "linux", "macos", "windows". */
  os: string;
  maxPages: number;
  maxMemoryBytes: number;
}

/** The Rust side's error: a kind the UI words, and a detail it never shows as the message. */
export interface UiError {
  kind: string;
  detail?: unknown;
}

export type Preset = "auto" | "novel" | "academic" | "textbook" | "poetry" | "scanned";

/** The user's settings, kept by the Rust side (`src-tauri/src/settings.rs`). */
export interface Settings {
  language: "en" | "de" | "tr" | null;
  preset: Preset;
  maxPages: number | null;
  maxMemoryBytes: number | null;
  firstrunDismissed: boolean;
}

/** The book's navigation, read from its nav document (`src-tauri/src/preview.rs`). */
export interface PreviewIndex {
  chapters: Array<{ title: string; href: string; level: number }>;
  pages: Array<{ label: string; href: string }>;
  lang: string;
}

/** A written diagnostic bundle (`src-tauri/src/diagnostics.rs`). */
export interface Bundle {
  path: string;
  bytes: number;
  entries: Array<{ name: string; bytes: number }>;
}

/** What the metadata editor changed. A field left out keeps what the book has. */
export interface MetadataPatch {
  title?: string;
  authors?: string[];
  language?: string;
}

/** One heading renamed or moved to another level, by its block id. */
export interface TocPatch {
  heading: string;
  title?: string;
  level?: number;
}

/** What an editor sends to "Fix and rebuild" (`src-tauri/src/corrections.rs`). */
export interface CorrectionPatch {
  metadata?: MetadataPatch;
  toc: TocPatch[];
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
  pickPdfs(): Promise<string[]>;
  settings(): Promise<Settings>;
  saveSettings(next: Settings): Promise<void>;
  report(job: string): Promise<Report>;
  openOutput(job: string): Promise<void>;
  showOutput(job: string): Promise<void>;
  previewIndex(job: string): Promise<PreviewIndex>;
  previewBase(): Promise<string>;
  exportDiagnostics(job: string | null): Promise<Bundle | null>;
  showBundle(): Promise<void>;
  cancel(job: string): Promise<void>;
  remove(job: string): Promise<void>;
  /**
   * Convert a password-protected job's PDF again with the password typed on its row. The row is
   * replaced by the new job, whose id is returned; the password lives in the Rust side's memory
   * until the engine starts and is never written anywhere (design decision 13).
   */
  unlock(job: string, password: string): Promise<string>;
  /**
   * "Fix and rebuild": keep these corrections with the book's others and convert it again with
   * them, replacing its EPUB. The row is replaced by the new job, whose id is returned.
   */
  saveOverrides(job: string, patch: CorrectionPatch): Promise<string>;
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
    pickPdfs: () => invoke<string[]>("pick_pdfs"),
    settings: () => invoke<Settings>("settings_get"),
    saveSettings: (next) => invoke<void>("settings_set", { next }),
    report: (job) => invoke<Report>("read_report", { job }),
    openOutput: (job) => invoke<void>("open_output", { job }),
    showOutput: (job) => invoke<void>("show_output", { job }),
    previewIndex: (job) => invoke<PreviewIndex>("preview_index", { job }),
    previewBase: () => invoke<string>("preview_base"),
    exportDiagnostics: (job) => invoke<Bundle | null>("export_diagnostics", { job }),
    showBundle: () => invoke<void>("show_bundle"),
    cancel: (job) => invoke<void>("cancel", { job }),
    remove: (job) => invoke<void>("remove", { job }),
    unlock: (job, password) => invoke<string>("unlock", { job, password }),
    saveOverrides: (job, patch) => invoke<string>("save_overrides", { job, patch }),
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
