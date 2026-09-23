// The Rust side of the desktop app, played in the page, so the built UI runs in a real browser
// exactly as it runs in the Tauri webview: its own code, its own bundle, the shipped CSP.
//
// Installed with `page.addInitScript`, before the bundle loads, as `window.__TAURI_INTERNALS__` —
// the object `@tauri-apps/api` calls into — so the spec drives the app's *real* `tauriBackend()`
// rather than a test double of it. The engine's side is a real engine's: the event lines are a
// recorded run of f09 (`f09.events.ndjson`) and the report is that run's `report.json`.
//
// Serialised into the page by Playwright, so this function may use nothing from outside itself.

export interface MockFixture {
  hello: unknown;
  config: unknown;
  /** The recorded engine lines of one conversion, replayed for every job. */
  lines: string[];
  report: unknown;
  /** What the native file picker "returns". */
  picked: string[];
  /** Milliseconds between two replayed lines, and between two download progress events. */
  pace: number;
  /** The model manager's rows (`models_list`), as the Rust side sends them. */
  models: { unavailable: string | null; rows: Array<Record<string, unknown>> };
  /** The pack registry's (`packs_list`). */
  packs: { unavailable: string | null; rows: Array<Record<string, unknown>> };
  /** The licence text `model_license` returns. */
  licenseText: string;
}

export function installTauriMock(fixture: MockFixture): void {
  type Job = { id: string; input: string; output: string; state: Record<string, unknown> };
  const callbacks = new Map<number, (data: unknown) => void>();
  const listeners = new Map<string, number[]>();
  const jobs: Job[] = [];
  let nextCallback = 1;
  let nextJob = 1;
  let settings: unknown = {
    language: "en",
    preset: "auto",
    maxPages: null,
    maxMemoryBytes: null,
    firstrunDismissed: false,
  };
  const calls: string[] = [];
  const violations: string[] = [];
  (window as unknown as Record<string, unknown>).__ocTest = { calls, violations };
  document.addEventListener("securitypolicyviolation", (event) => {
    violations.push(`${event.violatedDirective} ${event.blockedURI}`);
  });

  const view = (job: Job) => ({
    id: job.id,
    input: job.input,
    output: job.output,
    renamed: false,
    unlocked: false,
    rebuild: false,
    ...job.state,
  });
  const emit = (event: string, payload: unknown) => {
    for (const id of listeners.get(event) ?? []) callbacks.get(id)?.({ event, id, payload });
  };

  /** Start `job` the way the queue does, and replay the recorded engine run against it. */
  const run = (job: Job) => {
    job.state = { state: "running" };
    emit("job-changed", view(job));
    fixture.lines.forEach((line, index) => {
      setTimeout(() => emit("engine-line", { job: job.id, line }), fixture.pace * (index + 1));
    });
    setTimeout(() => {
      job.state = { state: "exited", code: 0 };
      emit("job-changed", view(job));
    }, fixture.pace * (fixture.lines.length + 1));
  };

  /** A download, played the way the model manager streams one: a few progress rows, then the
      row installed — unless cancelled first, which leaves it not installed. */
  const models = fixture.models.rows.map((row) => ({ ...row }));
  const cancelled = new Set<string>();
  const pull = (id: string) => {
    const row = models.find((candidate) => candidate.id === id);
    if (row === undefined) return;
    cancelled.delete(id);
    const total = row.size_bytes as number;
    const steps = 5;
    for (let step = 0; step <= steps; step += 1) {
      setTimeout(() => {
        if (cancelled.has(id)) return;
        Object.assign(row, { download: { state: "downloading", done: Math.floor((total * step) / steps), total } });
        emit("model-changed", { ...row });
      }, fixture.pace * (step + 1) * 10);
    }
    setTimeout(() => {
      if (cancelled.has(id)) return;
      Object.assign(row, { installed: true, license_path: `/data/openconvert/models/${id}/LICENSE`, download: { state: "idle" } });
      emit("model-changed", { ...row });
    }, fixture.pace * (steps + 2) * 10);
  };

  const commands: Record<string, (args: Record<string, unknown>) => unknown> = {
    models_list: () => ({ unavailable: fixture.models.unavailable, rows: models }),
    packs_list: () => fixture.packs,
    model_license: (args) => ({ id: args.id, license: "Apache-2.0", text: fixture.licenseText }),
    model_accept_license: (args) => {
      const row = models.find((candidate) => candidate.id === args.id);
      if (row !== undefined) row.license_accepted = true;
      return null;
    },
    model_pull: (args) => {
      pull(args.id as string);
      return null;
    },
    model_cancel: (args) => {
      const row = models.find((candidate) => candidate.id === args.id);
      cancelled.add(args.id as string);
      if (row !== undefined) {
        Object.assign(row, { download: { state: "idle" } });
        emit("model-changed", { ...row });
      }
      return null;
    },
    model_remove: () => null,
    startup_status: () => fixture.hello,
    ui_config: () => fixture.config,
    settings_get: () => settings,
    settings_set: (args) => {
      settings = args.next;
      return null;
    },
    queue_rows: () => jobs.map(view),
    pick_pdfs: () => fixture.picked,
    enqueue: (args) => {
      const paths = (args.paths as string[]) ?? [];
      const added: string[] = [];
      for (const input of paths.filter((path) => /\.pdf$/i.test(path))) {
        const job = {
          id: `job-${nextJob++}`,
          input,
          output: input.replace(/\.pdf$/i, ".epub"),
          state: { state: "queued", position: 1 },
        };
        jobs.push(job);
        added.push(job.id);
        run(job);
      }
      return { jobs: added, skipped: paths.filter((path) => !/\.pdf$/i.test(path)) };
    },
    read_report: () => fixture.report,
    open_output: () => null,
    show_output: () => null,
    preview_index: () => ({ chapters: [], pages: [], lang: "en" }),
    preview_base: () => "ocpreview://localhost/",
    export_diagnostics: () => null,
    show_bundle: () => null,
    cancel: () => null,
    remove: (args) => {
      const index = jobs.findIndex((job) => job.id === args.job);
      if (index >= 0) jobs.splice(index, 1);
      return null;
    },
    // Something to clear, so the danger button is drawn enabled and its contrast is checked.
    cache_usage: () => ({ bytes: 5 * 1024 * 1024, books: 2 }),
    clear_cache: () => null,
    quit: () => null,
  };

  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { windowLabel: "main", label: "main" },
    },
    transformCallback(callback: (data: unknown) => void, once = false) {
      const id = nextCallback++;
      callbacks.set(id, (data) => {
        if (once) callbacks.delete(id);
        callback?.(data);
      });
      return id;
    },
    unregisterCallback(id: number) {
      callbacks.delete(id);
    },
    convertFileSrc(path: string) {
      return path;
    },
    async invoke(command: string, args: Record<string, unknown> = {}) {
      calls.push(command);
      if (command === "plugin:event|listen") {
        const event = args.event as string;
        listeners.set(event, [...(listeners.get(event) ?? []), args.handler as number]);
        return args.handler;
      }
      if (command === "plugin:event|unlisten") return null;
      const handler = commands[command];
      if (handler === undefined) throw { kind: "io", detail: `no mock for ${command}` };
      return handler(args);
    },
  };
  (window as unknown as Record<string, unknown>).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener() {},
  };
}
