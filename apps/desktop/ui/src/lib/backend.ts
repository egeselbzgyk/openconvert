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
  /** How many of the four AI tasks this build enables for any language (`ai.task.*.languages`). */
  aiTasksEnabled: number;
  /** This build has the in-app updater (every build but the Flatpak's, which Flathub updates). */
  updater: boolean;
}

/** The Rust side's error: a kind the UI words, and a detail it never shows as the message. */
export interface UiError {
  kind: string;
  detail?: unknown;
}

export type Preset = "auto" | "novel" | "academic" | "textbook" | "poetry" | "scanned";

/** Settings › Provider (UI_UX §2.4). */
export type Provider = "builtin" | "ollama" | "custom";

/** The user allowed document text to be sent to `host` (D10). Set by the Rust side only. */
export interface Consent {
  host: string;
  grantedAt: string;
}

/** A custom endpoint. The key file and the consent are set by the Rust side only. */
export interface CustomEndpoint {
  endpoint: string;
  model: string;
  apiKeyFile: string | null;
  consent: Consent | null;
}

/** The user's settings, kept by the Rust side (`src-tauri/src/settings.rs`). */
export interface Settings {
  language: "en" | "de" | "tr" | null;
  preset: Preset;
  maxPages: number | null;
  maxMemoryBytes: number | null;
  firstrunDismissed: boolean;
  /** AI assistance: off by default (D17). */
  aiEnabled: boolean;
  provider: Provider;
  ollamaModel: string | null;
  custom: CustomEndpoint;
}

/** `openconvert provider detect --json`: Ollama on this computer, or `null`. */
export interface Detected {
  ollama: { url: string; models: string[] } | null;
}

/** `openconvert provider check <URL> --json`: sends nothing. */
export interface EndpointCheck {
  url: string;
  host: string;
  loopback: boolean;
  requires_consent: boolean;
  /** `false` only for plain http off this computer. */
  usable: boolean;
  reason: string | null;
}

/** `openconvert provider probe … --json` ("Test connection"). */
export type ProbeResult =
  | { available: true; url: string; host: string | null; provider: string; model: string; models: string[] }
  | { available: false; url: string; reason: string };

/** Settings › Network log (`src-tauri/src/netlog.rs`): `oc-net`'s audit log, newest first (PHASE 14 detail 12). */
export type NetworkLog =
  | { state: "not_recorded" }
  | { state: "entries"; entries: Array<{ ts: string; host: string; purpose: string; bytes: number; outcome: string }> };

/** What an update check found (`src-tauri/src/updater.rs`, `Checked`). `ready` means downloaded and
    its signature verified; a failure is a code the UI localises (`update.failed.<code>`). */
export type UpdateCheck =
  | { state: "up_to_date" }
  | { state: "ready"; version: string }
  | {
      state: "failed";
      code: "no_key" | "bad_signature" | "too_large" | "no_platform" | "bad_manifest" | "network";
    };

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

/** What the engine's cache holds (Settings › Advanced). */
export interface CacheUsage {
  bytes: number;
  books: number;
}

/** Where a model's or pack's download is (`src-tauri/src/models.rs`, `DownloadState`). */
export type DownloadState =
  | { state: "idle" }
  | { state: "downloading"; done: number; total: number }
  | { state: "cancelling"; done: number; total: number }
  | { state: "failed"; kind: "unreachable" | "verification" | "disk" | "refused"; detail: string };

/** Phase 9's `ModelReadiness`, exactly: what `openconvert model list --json` prints. */
export interface ModelReadiness {
  id: string;
  display_name: string;
  /** `default`, `small`, `quality` or `experimental` (D9). */
  tier: string;
  is_default: boolean;
  installed: boolean;
  size_bytes: number;
  ram_estimate_bytes: number;
  /** UI_UX §2's words: "fast", "moderate", "slower, higher quality", or "not yet measured". */
  cpu_expectation: string;
  license: string;
  license_path: string | null;
  warn: string | null;
}

/** What a pack row shows (`src-tauri/src/packs.rs`, `PackReadiness`). */
export interface PackReadiness {
  id: string;
  display_name: string;
  contents: string;
  installed: boolean;
  size_bytes: number;
  license: string;
  license_path: string | null;
}

/** A row: the readiness fields, and what the manager knows on top. */
export type Downloadable<R> = R & { license_accepted: boolean; download: DownloadState };
export type ModelRow = Downloadable<ModelReadiness>;
export type PackRow = Downloadable<PackReadiness>;

/** A screen of rows, or why nothing can be downloaded in this build. */
export interface CatalogView<R> {
  unavailable: string | null;
  rows: Array<Downloadable<R>>;
}

/** A licence, in full, as it is shown before a download. */
export interface LicenseView {
  id: string;
  license: string;
  text: string;
}

/** The two things that download: models and packs, by one mechanism (Phase 12 detail 9). */
export type CatalogKind = "models" | "packs";
export type CatalogRow<K extends CatalogKind> = K extends "models" ? ModelRow : PackRow;

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
  /** Save; the answer is what was kept (the key file and consent stay the Rust side's). */
  saveSettings(next: Settings): Promise<Settings>;
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
  cacheUsage(): Promise<CacheUsage>;
  /** Delete the cached text of every converted book; EPUBs and corrections stay. */
  clearCache(): Promise<void>;
  onLine(handler: (job: string, line: string) => void): Promise<Unlisten>;
  onJobChanged(handler: (view: JobView) => void): Promise<Unlisten>;
  onDragDrop(handler: (event: DropEvent) => void): Promise<Unlisten>;
  quit(): Promise<void>;
  /** The models or packs screen: every registry entry's row, or why there are none. */
  catalog<K extends CatalogKind>(kind: K): Promise<CatalogView<K extends "models" ? ModelReadiness : PackReadiness>>;
  license(kind: CatalogKind, id: string): Promise<LicenseView>;
  /** The user was shown the licence and accepted it; kept in the app's local state. */
  acceptLicense(kind: CatalogKind, id: string): Promise<void>;
  /** Start a download. Refused by the Rust side until the licence is accepted. */
  download(kind: CatalogKind, id: string): Promise<void>;
  /** Stop a download; its partial file is deleted (row 12.12). */
  cancelDownload(kind: CatalogKind, id: string): Promise<void>;
  /** Delete an installed model or pack. */
  removeDownload(kind: CatalogKind, id: string): Promise<void>;
  /** A row changed: progress, the end of a download, an acceptance, a delete. */
  onCatalogChanged<K extends CatalogKind>(kind: K, handler: (row: CatalogRow<K>) => void): Promise<Unlisten>;
  /** Settings › Provider: the engine's answers (`openconvert provider …`); the webview opens no socket. */
  providerDetect(): Promise<Detected>;
  providerCheck(url: string): Promise<EndpointCheck>;
  /** "Test connection" with the saved settings. Rejects with `consent_required` for an unconsented host. */
  providerProbe(): Promise<ProbeResult>;
  /** The native picker for the API key file; the answer is the settings as saved. */
  pickKeyFile(): Promise<Settings>;
  clearKeyFile(): Promise<Settings>;
  /** The consent dialog's Allow: consent to the saved custom endpoint's own host, now (D10). */
  grantConsent(): Promise<Settings>;
  /** Settings › Network log: the audit log's lines, or that this build keeps none. */
  networkLog(): Promise<NetworkLog>;
  /** Settings › About & updates: ask for, download and verify an update — only when the user asks. */
  updateCheck(): Promise<UpdateCheck>;
  /** Install the update the last check verified, and restart into it. */
  updateInstall(): Promise<void>;
}

/** The Rust commands and event of each catalog (`src-tauri/src/main.rs`). */
const CATALOG = {
  models: {
    list: "models_list",
    license: "model_license",
    accept: "model_accept_license",
    pull: "model_pull",
    cancel: "model_cancel",
    remove: "model_remove",
    event: "model-changed",
  },
  packs: {
    list: "packs_list",
    license: "pack_license",
    accept: "pack_accept_license",
    pull: "pack_pull",
    cancel: "pack_cancel",
    remove: "pack_remove",
    event: "pack-changed",
  },
} as const;

/** The real one, over Tauri's IPC. */
export function tauriBackend(): Backend {
  return {
    startup: () => invoke<Hello>("startup_status"),
    config: () => invoke<UiConfig>("ui_config"),
    rows: () => invoke<JobView[]>("queue_rows"),
    enqueue: (paths) => invoke<Enqueued>("enqueue", { paths }),
    pickPdfs: () => invoke<string[]>("pick_pdfs"),
    settings: () => invoke<Settings>("settings_get"),
    saveSettings: (next) => invoke<Settings>("settings_set", { next }),
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
    cacheUsage: () => invoke<CacheUsage>("cache_usage"),
    clearCache: () => invoke<void>("clear_cache"),
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
    catalog: (kind) => invoke(CATALOG[kind].list),
    license: (kind, id) => invoke<LicenseView>(CATALOG[kind].license, { id }),
    acceptLicense: (kind, id) => invoke<void>(CATALOG[kind].accept, { id }),
    download: (kind, id) => invoke<void>(CATALOG[kind].pull, { id }),
    cancelDownload: (kind, id) => invoke<void>(CATALOG[kind].cancel, { id }),
    removeDownload: (kind, id) => invoke<void>(CATALOG[kind].remove, { id }),
    onCatalogChanged: (kind, handler) => listen(CATALOG[kind].event, (event) => handler(event.payload as never)),
    providerDetect: () => invoke<Detected>("provider_detect"),
    providerCheck: (url) => invoke<EndpointCheck>("provider_check", { url }),
    providerProbe: () => invoke<ProbeResult>("provider_probe"),
    pickKeyFile: () => invoke<Settings>("pick_key_file"),
    clearKeyFile: () => invoke<Settings>("clear_key_file"),
    grantConsent: () => invoke<Settings>("grant_consent"),
    networkLog: () => invoke<NetworkLog>("network_log"),
    updateCheck: () => invoke<UpdateCheck>("update_check"),
    updateInstall: () => invoke<void>("update_install"),
  };
}
