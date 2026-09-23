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
  aiTasksEnabled: 0,
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
    // As the Rust side does: the key file and the consent are never the webview's to write.
    this.saved = { ...next, custom: { ...next.custom, apiKeyFile: this.saved.custom.apiKeyFile, consent: this.saved.custom.consent } };
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

  /** The model manager's screen, and the pack registry's (unpinned, as it ships). */
  models: CatalogView<ModelReadiness> = { unavailable: null, rows: modelRows() };
  packs: CatalogView<PackReadiness> = { unavailable: "model `validation` still has a placeholder in `license`", rows: [] };
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
