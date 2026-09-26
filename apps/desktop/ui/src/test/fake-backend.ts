/**
 * A scripted Rust side, for component tests: it answers commands from fields and lets the test
 * emit `engine-line` and `job-changed` exactly as the Rust side would.
 */

import type {
  Backend,
  Bundle,
  CacheUsage,
  CatalogKind,
  CatalogRow,
  CatalogView,
  CorrectionPatch,
  Detected,
  DropEvent,
  EndpointCheck,
  NetworkLog,
  Enqueued,
  HistoryEntry,
  LicenseView,
  ModelReadiness,
  ModelRow,
  PackReadiness,
  PackRow,
  PreviewIndex,
  ProbeResult,
  Settings,
  UiConfig,
  UiError,
  UpdateCheck,
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
  defaultStageDeadlineSecs: 1800,
  minStageDeadlineSecs: 60,
  maxStageDeadlineSecs: 86400,
  aiTasksEnabled: 0,
  updater: true,
};

/** Two books of an earlier run, as `history_list` sends them: one converted, one that failed. */
export function historyEntries(): HistoryEntry[] {
  return [
    {
      id: "s1-job-2",
      input: "/books/moby-dick.pdf",
      inputName: "moby-dick.pdf",
      output: "/home/me/Documents/OpenConvert/moby-dick.epub",
      startedAt: "2026-09-20T09:14:02Z",
      finishedAt: "2026-09-20T09:15:25Z",
      durationMs: 83250,
      status: "complete",
      exitCode: 0,
      errorCode: null,
      errorCap: null,
      title: "Moby-Dick; or, The Whale",
      authors: ["Herman Melville"],
      pages: 214,
      outputExists: true,
    },
    {
      id: "s1-job-1",
      input: "/books/atlas.pdf",
      inputName: "atlas.pdf",
      output: "/home/me/Documents/OpenConvert/atlas.epub",
      startedAt: "2026-09-20T08:00:00Z",
      finishedAt: "2026-09-20T08:30:00Z",
      durationMs: 1800000,
      status: "failed",
      exitCode: 1,
      errorCode: "E_LIMIT_EXCEEDED",
      errorCap: "stage_deadline_secs",
      title: null,
      authors: [],
      pages: null,
      outputExists: false,
    },
  ];
}

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

/** Rows as the model manager sends them, from a registry that pins two models. */
export function modelRows(): ModelRow[] {
  return [
    {
      id: "qwen3-1.7b-q4_k_m",
      display_name: "Qwen3 1.7B (Q4_K_M)",
      tier: "default",
      is_default: true,
      installed: false,
      size_bytes: 1_181_116_006,
      ram_estimate_bytes: 3_221_225_472,
      cpu_expectation: "moderate",
      license: "Apache-2.0",
      license_path: null,
      warn: null,
      license_accepted: false,
      download: { state: "idle" },
    },
    {
      id: "qwen3.5-2b-q4_k_m",
      display_name: "Qwen3.5 2B (Q4_K_M, community quant)",
      tier: "experimental",
      is_default: false,
      installed: false,
      size_bytes: 1_395_864_371,
      ram_estimate_bytes: 3_758_096_384,
      cpu_expectation: "not yet measured",
      license: "Apache-2.0",
      license_path: null,
      warn: "Community quantisation of a new hybrid architecture.",
      license_accepted: false,
      download: { state: "idle" },
    },
  ];
}

export const APACHE: LicenseView = {
  id: "qwen3-1.7b-q4_k_m",
  license: "Apache-2.0",
  text: "Apache License\nVersion 2.0, January 2004\n\nTERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION",
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
  /** What `ui_config` answers; a test changes it to be another build (the Flatpak's). */
  configured: UiConfig = { ...CONFIG };
  async config(): Promise<UiConfig> {
    return this.configured;
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
    stageDeadlineSecs: null,
    saveToLibrary: true,
    libraryDir: null,
    historyOpen: true,
    firstrunDismissed: false,
    aiEnabled: false,
    provider: "builtin",
    ollamaModel: null,
    custom: { endpoint: "", model: "", apiKeyFile: null, consent: null },
  };
  async pickPdfs(): Promise<string[]> {
    this.calls.push(["pick", null]);
    return this.picked;
  }
  async settings(): Promise<Settings> {
    return this.saved;
  }
  async saveSettings(next: Settings): Promise<Settings> {
    this.calls.push(["settings", next]);
    // As the Rust side does: the library folder, the key file and the consent are never the
    // webview's to write.
    this.saved = {
      ...next,
      libraryDir: this.saved.libraryDir,
      custom: { ...next.custom, apiKeyFile: this.saved.custom.apiKeyFile, consent: this.saved.custom.consent },
    };
    return this.saved;
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
  cache: CacheUsage = { bytes: 312 * 1024 * 1024, books: 14 };
  async cacheUsage(): Promise<CacheUsage> {
    return this.cache;
  }
  async clearCache(): Promise<void> {
    this.calls.push(["clearCache", null]);
    this.cache = { bytes: 0, books: 0 };
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

  /** The model manager's screen, and the pack registry's as 1.0 ships it: usable, offering no pack
   * (the validation pack is deferred past 1.0). */
  models: CatalogView<ModelReadiness> = { unavailable: null, rows: modelRows() };
  packs: CatalogView<PackReadiness> = { unavailable: null, rows: [] };
  private catalogHandlers: { models: Array<(row: ModelRow) => void>; packs: Array<(row: PackRow) => void> } = {
    models: [],
    packs: [],
  };
  async catalog<K extends CatalogKind>(kind: K): Promise<CatalogView<K extends "models" ? ModelReadiness : PackReadiness>> {
    return (kind === "models" ? this.models : this.packs) as never;
  }
  async license(kind: CatalogKind, id: string): Promise<LicenseView> {
    this.calls.push(["license", [kind, id]]);
    return { ...APACHE, id };
  }
  async acceptLicense(kind: CatalogKind, id: string): Promise<void> {
    this.calls.push(["acceptLicense", [kind, id]]);
  }
  async download(kind: CatalogKind, id: string): Promise<void> {
    this.calls.push(["download", [kind, id]]);
  }
  async cancelDownload(kind: CatalogKind, id: string): Promise<void> {
    this.calls.push(["cancelDownload", [kind, id]]);
  }
  async removeDownload(kind: CatalogKind, id: string): Promise<void> {
    this.calls.push(["removeDownload", [kind, id]]);
  }
  async onCatalogChanged<K extends CatalogKind>(kind: K, handler: (row: CatalogRow<K>) => void) {
    (this.catalogHandlers[kind] as Array<(row: CatalogRow<K>) => void>).push(handler);
    return () => undefined;
  }
  /** What `provider detect` answers. */
  detected: Detected = { ollama: { url: "localhost:11434", models: ["qwen3:1.7b", "llama3.2:3b"] } };
  async providerDetect(): Promise<Detected> {
    this.calls.push(["providerDetect", null]);
    return this.detected;
  }
  /** `provider check`, as the engine answers it: a host off this computer needs consent, plain http
      off it is not usable. */
  async providerCheck(url: string): Promise<EndpointCheck> {
    this.calls.push(["providerCheck", url]);
    const match = /^(https?):\/\/([^/:]+)/i.exec(url);
    if (match === null) throw { kind: "provider", detail: "not a URL" };
    const host = (match[2] ?? "").toLowerCase();
    const loopback = host === "localhost" || host.startsWith("127.");
    const https = (match[1] ?? "").toLowerCase() === "https";
    return { url, host, loopback, requires_consent: !loopback, usable: loopback || https, reason: null };
  }
  probe: ProbeResult = { available: false, url: "", reason: "the endpoint did not answer the capability probe" };
  async providerProbe(): Promise<ProbeResult> {
    this.calls.push(["providerProbe", null]);
    return this.probe;
  }
  async pickKeyFile(): Promise<Settings> {
    this.saved = { ...this.saved, custom: { ...this.saved.custom, apiKeyFile: "/home/me/keys/llm.key" } };
    return this.saved;
  }
  async clearKeyFile(): Promise<Settings> {
    this.saved = { ...this.saved, custom: { ...this.saved.custom, apiKeyFile: null } };
    return this.saved;
  }
  async grantConsent(): Promise<Settings> {
    const check = await this.providerCheck(this.saved.custom.endpoint);
    this.calls.push(["grantConsent", check.host]);
    this.saved = { ...this.saved, custom: { ...this.saved.custom, consent: { host: check.host, grantedAt: "2026-09-23T10:00:00Z" } } };
    return this.saved;
  }

  /** What the network log reads: nothing, until PHASE 14 detail 12's audit log exists. */
  network: NetworkLog = { state: "not_recorded" };
  async networkLog(): Promise<NetworkLog> {
    return this.network;
  }

  /** What the next update check finds. */
  update: UpdateCheck = { state: "up_to_date" };
  async updateCheck(): Promise<UpdateCheck> {
    this.calls.push(["updateCheck", null]);
    return this.update;
  }
  async updateInstall(): Promise<void> {
    this.calls.push(["updateInstall", null]);
  }

  /** What `history_list` answers: earlier runs' books. Empty unless a test fills it. */
  earlier: HistoryEntry[] = [];
  /** Opening an earlier book fails, as it does for a book moved away since. */
  earlierGone = false;
  async history(): Promise<HistoryEntry[]> {
    return this.earlier;
  }
  async historyRemove(id: string): Promise<void> {
    this.calls.push(["historyRemove", id]);
    this.earlier = this.earlier.filter((entry) => entry.id !== id);
  }
  async historyClear(): Promise<void> {
    this.calls.push(["historyClear", null]);
    this.earlier = [];
  }
  async historyOpen(id: string): Promise<void> {
    this.calls.push(["historyOpen", id]);
    if (this.earlierGone) throw { kind: "not_on_disk", detail: id };
  }
  async historyShow(id: string): Promise<void> {
    this.calls.push(["historyShow", id]);
  }
  library = "/home/me/Documents/OpenConvert";
  async libraryPath(): Promise<string> {
    return this.saved.libraryDir ?? this.library;
  }
  async openLibrary(): Promise<void> {
    this.calls.push(["openLibrary", null]);
  }
  async pickLibraryDir(): Promise<Settings> {
    this.saved = { ...this.saved, libraryDir: "/home/me/Books" };
    return this.saved;
  }
  async resetLibraryDir(): Promise<Settings> {
    this.saved = { ...this.saved, libraryDir: null };
    return this.saved;
  }

  /** The Rust side announces a changed model row. */
  modelChanged(row: ModelRow): void {
    this.models = { ...this.models, rows: this.models.rows.map((known) => (known.id === row.id ? row : known)) };
    this.catalogHandlers.models.forEach((handler) => handler(row));
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
