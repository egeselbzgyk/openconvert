<script lang="ts">
  // The window. Before anything else: is the converter the one this app was built with? If not,
  // the blocking error is the whole window (firstrun.html §2, RT A5.7). If it is, the routes of
  // screen-map.md — `queue` first — with the header last in the DOM and drawn first.
  import { onMount } from "svelte";

  import AppHeader from "./components/AppHeader.svelte";
  import BlockingError from "./components/BlockingError.svelte";
  import {
    tauriBackend,
    type Backend,
    type DropEvent,
    type Settings,
    type UiConfig,
    type UiError,
  } from "./lib/backend";
  import { IR_VERSION, PROTOCOL_VERSION } from "./lib/events";
  import { SPRITE } from "./lib/icons";
  import { JobStore, type Blocking } from "./lib/jobs.svelte";
  import type { Row } from "./lib/jobstate";
  import { i18n, setLanguage, t, tn } from "./lib/locale.svelte";
  import Queue from "./routes/queue/Queue.svelte";
  import Preview from "./routes/preview/Preview.svelte";
  import Report from "./routes/report/Report.svelte";
  import { pageNumber } from "./lib/report";

  let { backend = tauriBackend(), clock = () => Date.now() }: { backend?: Backend; clock?: () => number } = $props();

  type Route =
    | { name: "queue" }
    | { name: "settings" }
    | { name: "report"; job: string }
    | { name: "preview"; job: string; page: string | null; from: "queue" | "report" };

  let config = $state<UiConfig | null>(null);
  let settings = $state<Settings | null>(null);
  let store = $state<JobStore | null>(null);
  let startupError = $state<Blocking | null>(null);
  let route = $state<Route>({ name: "queue" });
  let dragging = $state<{ pdfs: number; skipped: string[] } | null>(null);
  let announcement = $state("");

  const blocking = $derived(startupError ?? store?.blocking ?? null);

  /** The Rust side's startup verdict, as a blocking screen. */
  function fromStartup(error: UiError): Blocking {
    const detail = (error.detail ?? {}) as Record<string, unknown>;
    switch (error.kind) {
      case "stale_engine":
        return { kind: "version", app: String(detail.app ?? ""), engine: String(detail.engine ?? "") };
      case "protocol_mismatch":
        return { kind: "protocol", app: PROTOCOL_VERSION, engine: Number(detail.engine ?? NaN) };
      case "ir_mismatch":
        return { kind: "ir", app: IR_VERSION, engine: Number(detail.engine ?? NaN) };
      default:
        return { kind: "missing", detail: typeof error.detail === "string" ? error.detail : error.kind };
    }
  }

  const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;
  const isPdf = (path: string) => /\.pdf$/i.test(path);

  onMount(() => {
    const unlisten: Array<() => void> = [];
    let timer: ReturnType<typeof setInterval> | undefined;
    let closed = false;

    (async () => {
      config = await backend.config();
      settings = await backend.settings();
      setLanguage(settings.language ?? "system");
      try {
        await backend.startup();
      } catch (error) {
        startupError = fromStartup(error as UiError);
        return;
      }
      const jobs = new JobStore(config.heartbeatTimeoutMs, (id) => void loadReport(jobs, id));
      store = jobs;
      unlisten.push(await backend.onJobChanged((view) => jobs.view(view)));
      unlisten.push(await backend.onLine((job, line) => jobs.line(job, line, clock())));
      unlisten.push(await backend.onDragDrop(dragDrop));
      jobs.load(await backend.rows());
      // Heartbeats are judged on the supervisor's own tick; the rows never move on it.
      timer = setInterval(() => jobs.tick(clock()), config.supervisorTickMs);
      if (closed) unlisten.forEach((stop) => stop());
    })();

    return () => {
      closed = true;
      if (timer !== undefined) clearInterval(timer);
      unlisten.forEach((stop) => stop());
    };
  });

  /** Native drag and drop: say what a drop would do, then do it (queue.html §2). */
  function dragDrop(event: DropEvent) {
    switch (event.type) {
      case "enter":
        dragging = {
          pdfs: event.paths.filter(isPdf).length,
          skipped: event.paths.filter((path) => !isPdf(path)).map(fileName),
        };
        break;
      case "over":
        break;
      case "drop":
        dragging = null;
        void add(event.paths);
        break;
      case "leave":
        dragging = null;
    }
  }

  /** Queue what was dropped or picked, and say what happened in the polite region. */
  async function add(paths: string[]) {
    if (paths.length === 0) return;
    const result = await backend.enqueue(paths);
    const parts = [];
    if (result.jobs.length > 0) parts.push(tn("drop.added", result.jobs.length));
    for (const skipped of result.skipped) parts.push(t("drop.skipped", { file: fileName(skipped) }));
    announcement = parts.join(" ");
  }

  async function select() {
    await add(await backend.pickPdfs());
  }

  /** A completed job's report.json, for its result panel. */
  async function loadReport(jobs: JobStore, id: string) {
    try {
      jobs.setReport(id, await backend.report(id));
    } catch {
      jobs.setReport(id, "unavailable");
    }
  }

  async function open(id: string) {
    try {
      await backend.openOutput(id);
    } catch {
      store?.setNoReader(id);
    }
  }

  async function cancel(id: string) {
    await backend.cancel(id);
  }
  async function remove(id: string) {
    const row = store?.rows.find((candidate) => candidate.id === id);
    await backend.remove(id);
    store?.keepOnly(new Set((await backend.rows()).map((view) => view.id)));
    if (row !== undefined) {
      const waiting = store?.rows.filter((candidate) => candidate.phase === "queued").length ?? 0;
      announcement = t("queue.removed", { file: fileName(row.input), w: waiting });
    }
  }
  async function removeWaiting() {
    for (const row of store?.rows.filter((candidate) => candidate.phase === "queued") ?? []) {
      await backend.remove(row.id);
    }
    store?.keepOnly(new Set((await backend.rows()).map((view) => view.id)));
  }
  async function retry(row: Row) {
    await backend.enqueue([row.input]);
    await remove(row.id);
  }
</script>

<!-- The icon sprite, once, invisible; every icon is a <use> into it. -->
<div class="oc-sprite" aria-hidden="true">{@html SPRITE}</div>

<div class="oc-app oc-app--window" lang={i18n.locale}>
  {#if blocking !== null && config !== null}
    <BlockingError {blocking} appVersion={config.appVersion} onquit={() => backend.quit()} />
    <AppHeader />
  {:else if store !== null}
    {#if route.name === "queue"}
      <Queue
        {store}
        {dragging}
        onselect={select}
        oncancel={cancel}
        onremove={remove}
        onretry={retry}
        onremovewaiting={removeWaiting}
        ontoggle={(id) => store?.toggle(id)}
        onopen={open}
        onshow={(id) => void backend.showOutput(id)}
        ondetails={(id) => (route = { name: "report", job: id })}
        onpreview={(id) => (route = { name: "preview", job: id, page: null, from: "queue" })}
        onpage={(id, page) => (route = { name: "preview", job: id, page, from: "queue" })}
      />
      <AppHeader onsettings={() => (route = { name: "settings" })} />
    {:else if route.name === "report"}
      {@const job = route.job}
      {@const row = store.rows.find((candidate) => candidate.id === job)}
      {#if row !== undefined && config !== null}
        <Report {row} appVersion={config.appVersion} onpage={(page) => (route = { name: "preview", job, page, from: "report" })} />
      {/if}
      <AppHeader title={t("report.title")} back={{ label: t("queue.title"), onclick: () => (route = { name: "queue" }) }} />
    {:else if route.name === "preview"}
      {@const target = route}
      {@const row = store.rows.find((candidate) => candidate.id === target.job)}
      {@const report = row?.report !== null && row?.report !== "unavailable" ? row?.report : undefined}
      {#if row !== undefined}
        <Preview
          {row}
          {backend}
          page={target.page}
          warning={report?.warnings.find((warning) => pageNumber(warning.page) === target.page) ?? null}
        />
      {/if}
      <AppHeader
        title={row === undefined ? t("action.preview") : (row.output.split(/[\\/]/).pop() ?? row.output)}
        back={{
          label: t("preview.close"),
          onclick: () => (route = target.from === "report" ? { name: "report", job: target.job } : { name: "queue" }),
        }}
      />
    {:else}
      <main class="oc-main"></main>
      <AppHeader title={t("settings.title")} back={{ label: t("queue.title"), onclick: () => (route = { name: "queue" }) }} />
    {/if}
  {/if}
  <div class="oc-sr-only" role="status" aria-live="polite">{announcement}</div>
</div>
