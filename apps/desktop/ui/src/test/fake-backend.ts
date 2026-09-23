/**
 * A scripted Rust side, for component tests: it answers commands from fields and lets the test
 * emit `engine-line` and `job-changed` exactly as the Rust side would.
 */

import type {
  Backend,
  Bundle,
  CorrectionPatch,
  DropEvent,
  Enqueued,
  PreviewIndex,
  Settings,
  UiConfig,
  UiError,
} from "../lib/backend";
import type { Hello } from "../lib/events";
import type { JobView } from "../lib/jobstate";
import type { Report } from "../lib/report";

export const CONFIG: UiConfig = {
  appVersion: "0.1.0",
  heartbeatTimeoutMs: 6000,
  cancelDeadlineMs: 2000,
  killAfterMs: 5000,
  copiedRevertMs: 2000,
  supervisorTickMs: 250,
  os: "linux",
  maxPages: 3000,
  maxMemoryBytes: 4294967296,
};

export const HELLO: Hello = {
  v: 1,
  t: "hello",
  seq: 0,
  ts_ms: 1,
  engine_version: "0.1.0",
  ir_version: 1,
  protocol: 1,
  pdfium_version: "151.0.7881.0",
  capabilities: ["inspect"],
};

export class FakeBackend implements Backend {
  startupError: UiError | null = null;
  views: JobView[] = [];
  calls: Array<[string, unknown]> = [];
  private lines: Array<(job: string, line: string) => void> = [];
  private changes: Array<(view: JobView) => void> = [];
  private drops: Array<(event: DropEvent) => void> = [];

  async startup(): Promise<Hello> {
    if (this.startupError !== null) throw this.startupError;
    return HELLO;
  }
  async config(): Promise<UiConfig> {
    return CONFIG;
  }
  async rows(): Promise<JobView[]> {
    return this.views;
  }
  async enqueue(paths: string[]): Promise<Enqueued> {
    this.calls.push(["enqueue", paths]);
    return { jobs: [], skipped: [] };
  }
  picked: string[] = [];
  saved: Settings = {
    language: "en",
    preset: "auto",
    maxPages: null,
    maxMemoryBytes: null,
    firstrunDismissed: false,
  };
  async pickPdfs(): Promise<string[]> {
    this.calls.push(["pick", null]);
    return this.picked;
  }
  async settings(): Promise<Settings> {
    return this.saved;
  }
  async saveSettings(next: Settings): Promise<void> {
    this.calls.push(["settings", next]);
    this.saved = next;
  }
  reports = new Map<string, Report>();
  noReader = false;
  async report(job: string): Promise<Report> {
    const report = this.reports.get(job);
    if (report === undefined) throw { kind: "io", detail: "no report" };
    return report;
  }
  async openOutput(job: string): Promise<void> {
    this.calls.push(["open", job]);
    if (this.noReader) throw { kind: "io", detail: "no handler" };
  }
  async showOutput(job: string): Promise<void> {
    this.calls.push(["show", job]);
  }
  preview: PreviewIndex = {
    chapters: [
      { title: "Preface", href: "text/c0001.xhtml#sec0h", level: 1 },
      { title: "Chapter One", href: "text/c0002.xhtml#sec1h", level: 1 },
    ],
    pages: [
      { label: "1", href: "text/c0001.xhtml#page0" },
      { label: "2", href: "text/c0002.xhtml#page1" },
      { label: "3", href: "text/c0002.xhtml#page2" },
    ],
    lang: "en",
  };
  async previewIndex(): Promise<PreviewIndex> {
    return this.preview;
  }
  async previewBase(): Promise<string> {
    return "ocpreview://localhost/";
  }
  bundle: Bundle | null = {
    path: "/home/me/Desktop/openconvert-diagnostics-2026-09-23.zip",
    bytes: 188416,
    entries: [
      { name: "report.json", bytes: 63488 },
      { name: "events.ndjson", bytes: 106496 },
      { name: "versions.txt", bytes: 80 },
      { name: "system.txt", bytes: 40 },
    ],
  };
  async exportDiagnostics(job: string | null): Promise<Bundle | null> {
    this.calls.push(["export", job]);
    return this.bundle;
  }
  async showBundle(): Promise<void> {
    this.calls.push(["showBundle", null]);
  }
  async cancel(job: string): Promise<void> {
    this.calls.push(["cancel", job]);
  }
  async remove(job: string): Promise<void> {
    this.calls.push(["remove", job]);
    this.views = this.views.filter((view) => view.id !== job);
  }
  /** Unlock calls, with the password, so a test can see what reached the Rust side. */
  unlocked: Array<[string, string]> = [];
  async unlock(job: string, password: string): Promise<string> {
    this.unlocked.push([job, password]);
    const old = this.views.find((view) => view.id === job);
    this.views = this.views.filter((view) => view.id !== job);
    const id = `${job}-unlocked`;
    if (old !== undefined) {
      this.views = [...this.views, { ...old, id, unlocked: true, state: "running" }];
    }
    return id;
  }
  /** "Fix and rebuild" calls, with what the editor sent. */
  corrected: Array<[string, CorrectionPatch]> = [];
  async saveOverrides(job: string, patch: CorrectionPatch): Promise<string> {
    this.corrected.push([job, patch]);
    const old = this.views.find((view) => view.id === job);
    this.views = this.views.filter((view) => view.id !== job);
    const id = `${job}-rebuilt`;
    if (old !== undefined) {
      this.views = [...this.views, { ...old, id, rebuild: true, unlocked: false, state: "running" }];
    }
    return id;
  }
  async onLine(handler: (job: string, line: string) => void) {
    this.lines.push(handler);
    return () => undefined;
  }
  async onJobChanged(handler: (view: JobView) => void) {
    this.changes.push(handler);
    return () => undefined;
  }
  async onDragDrop(handler: (event: DropEvent) => void) {
    this.drops.push(handler);
    return () => undefined;
  }
  async quit(): Promise<void> {
    this.calls.push(["quit", null]);
  }

  /** The Rust side relays one engine line of job `job`. */
  line(job: string, event: Record<string, unknown>, seq = 0): void {
    const text = JSON.stringify({ v: 1, seq, ts_ms: 1, ...event });
    this.lines.forEach((handler) => handler(job, text));
  }
  /** The Rust side reports a queue change. */
  change(view: JobView): void {
    this.views = [...this.views.filter((known) => known.id !== view.id), view];
    this.changes.forEach((handler) => handler(view));
  }
  /** Tauri delivers a native drag-and-drop event. */
  drop(event: DropEvent): void {
    this.drops.forEach((handler) => handler(event));
  }
}

/** Let the app's async start-up (config, startup, listeners, rows) run to completion. */
export async function settle(): Promise<void> {
  for (let i = 0; i < 10; i += 1) await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 0));
}
