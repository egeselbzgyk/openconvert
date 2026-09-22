<script lang="ts">
  // The window. Before anything else: is the converter the one this app was built with? If not,
  // the blocking error is the whole window (firstrun.html §2, RT A5.7). If it is, the queue.
  import { onMount } from "svelte";

  import BlockingError from "./components/BlockingError.svelte";
  import QueueRow from "./components/QueueRow.svelte";
  import { tauriBackend, type Backend, type UiConfig, type UiError } from "./lib/backend";
  import { IR_VERSION, PROTOCOL_VERSION } from "./lib/events";
  import { SPRITE } from "./lib/icons";
  import { JobStore, type Blocking } from "./lib/jobs.svelte";
  import type { Row } from "./lib/jobstate";
  import { i18n, t } from "./lib/locale.svelte";

  let { backend = tauriBackend(), clock = () => Date.now() }: { backend?: Backend; clock?: () => number } = $props();

  let config = $state<UiConfig | null>(null);
  let store = $state<JobStore | null>(null);
  let startupError = $state<Blocking | null>(null);

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

  onMount(() => {
    const unlisten: Array<() => void> = [];
    let timer: ReturnType<typeof setInterval> | undefined;
    let closed = false;

    (async () => {
      config = await backend.config();
      try {
        await backend.startup();
      } catch (error) {
        startupError = fromStartup(error as UiError);
        return;
      }
      const jobs = new JobStore(config.heartbeatTimeoutMs);
      store = jobs;
      unlisten.push(await backend.onJobChanged((view) => jobs.view(view)));
      unlisten.push(await backend.onLine((job, line) => jobs.line(job, line, clock())));
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

  async function cancel(id: string) {
    await backend.cancel(id);
  }
  async function remove(id: string) {
    await backend.remove(id);
    store?.keepOnly(new Set((await backend.rows()).map((view) => view.id)));
  }
  async function retry(row: Row) {
    await backend.enqueue([row.input]);
    await remove(row.id);
  }
</script>

<!-- The icon sprite, once, invisible; every icon is a <use> into it. -->
<div class="oc-sprite" aria-hidden="true">{@html SPRITE}</div>

<div class="oc-app" lang={i18n.locale}>
  {#if blocking !== null && config !== null}
    <BlockingError {blocking} appVersion={config.appVersion} onquit={() => backend.quit()} />
  {:else if store !== null}
    <main class="oc-main">
      {#if store.rows.length > 0}
        <ul class="oc-queue" aria-label={t("queue.title")}>
          {#each store.rows as row (row.id)}
            <QueueRow {row} now={store.now} oncancel={cancel} onremove={remove} onretry={retry} />
          {/each}
        </ul>
      {/if}
    </main>
  {/if}
  <header class="oc-header">
    <span class="oc-header__title">
      <svg class="oc-logo" aria-hidden="true"><use href="#oc-logo-sm"></use></svg>{t("app.title")}
    </span>
  </header>
</div>
