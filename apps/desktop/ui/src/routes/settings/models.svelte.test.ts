import { flushSync, mount, unmount } from "svelte";
import { afterEach, describe, expect, it } from "vitest";

import App from "../../App.svelte";
import type { DownloadState, ModelRow } from "../../lib/backend";
import { setLanguage } from "../../lib/locale.svelte";
import { FakeBackend, modelRows, settle } from "../../test/fake-backend";

// Phase 12 detail 9 and row 12.12's UI half: the Models screen renders exactly the model
// manager's rows (Phase 9's `ModelReadiness`), shows each licence in full and has it accepted
// before a download, streams the download's real bytes, and cancels it. The first-run card and
// route (UI_UX §3, screen-map `firstrun`) take their numbers from the default model's row.

let app: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (app !== null) unmount(app);
  app = null;
  document.body.innerHTML = "";
  setLanguage("en");
});

const buttons = () => [...document.querySelectorAll<HTMLButtonElement>("button")];
const button = (text: string) => buttons().find((b) => b.textContent?.trim() === text);
const nav = (text: string) => [...document.querySelectorAll<HTMLAnchorElement>(".oc-nav__item")].find((a) => a.textContent === text);
const row = (id: string) => document.querySelector<HTMLElement>(`.oc-model[data-row="${id}"]`);
const DEFAULT = "qwen3-1.7b-q4_k_m";
const MB = 1024 * 1024;

async function start(backend = new FakeBackend()) {
  app = mount(App, { target: document.body, props: { backend, clock: () => 0 } });
  await settle();
  flushSync();
  return backend;
}

async function openModels(backend = new FakeBackend()) {
  await start(backend);
  button("Settings")?.click();
  flushSync();
  nav("Models")?.click();
  flushSync();
  return backend;
}

function withDownload(download: DownloadState, change: Partial<ModelRow> = {}): ModelRow {
  const first = modelRows()[0];
  if (first === undefined) throw new Error("no default row");
  return { ...first, license_accepted: true, ...change, download };
}

describe("models route", () => {
  it("renders exactly the ModelReadiness fields of each row", async () => {
    await openModels();
    const first = row(DEFAULT);
    expect(first?.querySelector(".oc-model__name")?.textContent).toBe("Qwen3 1.7B (Q4_K_M)");
    expect(first?.querySelector(".oc-tag--default")?.textContent).toBe("Default");
    expect(first?.querySelector(".oc-model__state")?.textContent).toBe("Not installed");
    const facts = [...(first?.querySelectorAll(".oc-model__facts span") ?? [])].map((span) => span.textContent);
    expect(facts).toEqual(["1.1 GB", "RAM ~3 GB", "CPU: moderate", "Apache-2.0"]);

    const experimental = row("qwen3.5-2b-q4_k_m");
    expect(experimental?.querySelector(".oc-tag--experimental")?.textContent).toBe("Experimental");
    expect(experimental?.textContent).toContain("CPU: not yet measured");
    expect(experimental?.querySelector(".oc-model__warn")?.textContent).toContain("has not passed the model gate");

    // The same fields in German: the locale's numbers and words, never English.
    setLanguage("de");
    flushSync();
    const german = [...(row(DEFAULT)?.querySelectorAll(".oc-model__facts span") ?? [])].map((span) =>
      // German writes a no-break space between a number and its unit.
      span.textContent?.replaceAll("\u00a0", " "),
    );
    expect(german).toEqual(["1,1 GB", "RAM ~3 GB", "CPU: mittel", "Apache-2.0"]);
  });

  it("model_download_progress_streams_and_cancels — the licence first, real bytes, a working Cancel", async () => {
    const backend = await openModels();
    button("Download…")?.click();
    await settle();
    flushSync();

    // The licence, in full, before anything is fetched.
    expect(backend.calls).toContainEqual(["license", ["models", DEFAULT]]);
    expect(row(DEFAULT)?.querySelector(".oc-license")?.textContent).toContain("Apache License");
    expect(row(DEFAULT)?.querySelector(".oc-model__state")?.textContent).toBe("License shown · not accepted yet");
    expect(backend.calls.some(([name]) => name === "download")).toBe(false);

    button("Accept license and download 1.1 GB")?.click();
    await settle();
    const order = backend.calls.map(([name]) => name).filter((name) => name === "acceptLicense" || name === "download");
    expect(order).toEqual(["acceptLicense", "download"]);

    // Progress moves only when the manager says bytes arrived.
    const total = 1_181_116_006;
    backend.modelChanged(withDownload({ state: "downloading", done: 0, total }));
    flushSync();
    const bar = () => row(DEFAULT)?.querySelector('[role="progressbar"]');
    expect(bar()?.getAttribute("aria-valuenow")).toBe("0");
    backend.modelChanged(withDownload({ state: "downloading", done: 638 * MB, total }));
    flushSync();
    expect(bar()?.getAttribute("aria-valuenow")).toBe(String(638 * MB));
    expect(bar()?.getAttribute("aria-valuetext")).toBe("638 MB of 1.1 GB");
    expect(row(DEFAULT)?.querySelector(".oc-model__state")?.textContent?.trim()).toBe("638 MB of 1.1 GB · 57%");

    // Cancel: asked of the Rust side, the row says so, and a cancelled download is simply not
    // installed again — its partial file is gone (row 12.12).
    button("Cancel")?.click();
    await settle();
    expect(backend.calls).toContainEqual(["cancelDownload", ["models", DEFAULT]]);
    backend.modelChanged(withDownload({ state: "cancelling", done: 640 * MB, total }));
    flushSync();
    expect(row(DEFAULT)?.querySelector(".oc-model__state")?.textContent?.trim()).toBe("Cancelling…");
    expect(button("Cancel")?.disabled).toBe(true);
    backend.modelChanged(withDownload({ state: "idle" }));
    flushSync();
    expect(bar()).toBeNull();
    expect(row(DEFAULT)?.querySelector(".oc-model__state")?.textContent).toBe("Not installed");
    expect(button("Download…")).toBeDefined();
  });

  it("an installed model offers Delete; a failed download offers Retry", async () => {
    const backend = await openModels();
    backend.modelChanged(
      withDownload({ state: "idle" }, { installed: true, license_path: "/data/openconvert/models/qwen3-1.7b-q4_k_m/LICENSE" }),
    );
    flushSync();
    expect(row(DEFAULT)?.querySelector(".oc-model__state")?.textContent).toBe("Installed");
    expect(row(DEFAULT)?.textContent).toContain("Apache-2.0, accepted");
    expect(row(DEFAULT)?.querySelector(".oc-mono")?.textContent).toBe("/data/openconvert/models/qwen3-1.7b-q4_k_m/LICENSE");
    button("Delete")?.click();
    await settle();
    expect(backend.calls).toContainEqual(["removeDownload", ["models", DEFAULT]]);

    backend.modelChanged(withDownload({ state: "failed", kind: "verification", detail: "SHA-256 mismatch" }));
    flushSync();
    expect(row(DEFAULT)?.classList.contains("oc-model--failed")).toBe(true);
    expect(row(DEFAULT)?.textContent).toContain("Download failed: the file did not match its checksum");
    button("Retry")?.click();
    await settle();
    expect(backend.calls).toContainEqual(["download", ["models", DEFAULT]]);
  });

  it("a registry without pins offers no model, and the validation pack is not available", async () => {
    const backend = new FakeBackend();
    backend.models = { unavailable: "model `qwen3-1.7b-q4_k_m` still has a placeholder in `revision`", rows: [] };
    await openModels(backend);
    expect(document.querySelector(".oc-settings__body")?.textContent).toContain(
      "No model can be downloaded with this build of the app.",
    );
    expect(document.querySelector(".oc-model")).toBeNull();

    nav("Packs")?.click();
    flushSync();
    const validation = [...document.querySelectorAll(".oc-model")].find((element) => element.textContent?.includes("Validation pack"));
    expect(validation?.textContent).toContain("Not available in this version");
    expect(validation?.querySelector("button")).toBeNull();

    // And no first-run card offers a download that cannot happen.
    const queue = await (async () => {
      unmount(app!);
      document.body.innerHTML = "";
      return start(backend);
    })();
    expect(queue).toBe(backend);
    expect(document.querySelector(".oc-card--firstrun")).toBeNull();
  });
});

describe("first run", () => {
  it("the card's costs are the default model's row; Set up opens it with its licence shown", async () => {
    const backend = await start();
    const card = document.querySelector(".oc-card--firstrun");
    expect([...(card?.querySelectorAll(".oc-chip") ?? [])].map((chip) => chip.textContent)).toEqual([
      "~1.1 GB download",
      "~3 GB RAM while converting",
      "a few extra seconds per book",
    ]);

    button("Set up AI assistance")?.click();
    await settle();
    flushSync();
    expect(document.querySelector(".oc-settings__title")?.textContent).toBe("Set up AI assistance");
    expect(document.querySelector(".oc-settings__hint")?.textContent).toContain("Downloads Qwen3 1.7B (Q4_K_M) (Apache-2.0): 1.1 GB");
    expect(document.querySelectorAll(".oc-model").length, "the default model only").toBe(1);
    expect(backend.calls).toContainEqual(["license", ["models", DEFAULT]]);
    expect(row(DEFAULT)?.querySelector(".oc-license")?.textContent).toContain("Apache License");

    button("Accept license and download 1.1 GB")?.click();
    await settle();
    backend.modelChanged(withDownload({ state: "idle" }, { installed: true }));
    flushSync();
    button("Back to the queue")?.click();
    flushSync();
    expect(document.querySelector(".oc-dropzone")).not.toBeNull();
    expect(document.querySelector(".oc-card--firstrun"), "installed: not offered again").toBeNull();
  });

  it("Not now hides the card and it stays hidden", async () => {
    const backend = await start();
    button("Not now")?.click();
    flushSync();
    expect(document.querySelector(".oc-card--firstrun")).toBeNull();
    expect(backend.saved.firstrunDismissed).toBe(true);

    unmount(app!);
    document.body.innerHTML = "";
    await start(backend);
    expect(document.querySelector(".oc-card--firstrun"), "not on the next launch either").toBeNull();
  });
});
