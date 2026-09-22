<script lang="ts">
  // One job (components.md, QueueRow): queued(pos) · running determinate · running indeterminate
  // + pulse · not responding · cancelling · cancelled · failed · complete. Every state is read off
  // the row, which is read off the engine's events; nothing here keeps time of its own.
  import type { Row } from "../lib/jobstate";
  import { silentSeconds } from "../lib/jobstate";
  import { countText, stepLabel, stepOf } from "../lib/labels";
  import { t } from "../lib/locale.svelte";
  import { pulse } from "../lib/motion";
  import Icon from "./Icon.svelte";
  import Spinner from "./Spinner.svelte";
  import ResultPanel from "./ResultPanel.svelte";
  import StageList from "./StageList.svelte";

  let {
    row,
    now,
    active = true,
    oncancel,
    onremove,
    onretry,
    ontoggle = () => undefined,
    onopen = () => undefined,
    onshow = () => undefined,
    ondetails = () => undefined,
    onpage = null,
  }: {
    row: Row;
    now: number;
    /** The roving-tabindex row: the one Tab lands on (QueueList). */
    active?: boolean;
    oncancel: (id: string) => void;
    onremove: (id: string) => void;
    onretry: (row: Row) => void;
    ontoggle?: (id: string) => void;
    onopen?: (id: string) => void;
    onshow?: (id: string) => void;
    ondetails?: (id: string) => void;
    onpage?: ((id: string, page: string) => void) | null;
  } = $props();

  const report = $derived(row.report !== null && row.report !== "unavailable" ? row.report : null);
  const invalid = $derived(report?.status === "invalid");

  const tab = $derived(active ? 0 : -1);

  /** Delete removes a waiting row; Enter/Space toggles a completed one (components.md, QueueRow). */
  function keydown(event: KeyboardEvent) {
    if (event.target !== event.currentTarget) return;
    if ((event.key === "Enter" || event.key === " ") && row.phase === "complete") {
      event.preventDefault();
      ontoggle(row.id);
      return;
    }
    if (event.key === "Delete" && (row.phase === "queued" || row.phase === "cancelled" || row.phase === "failed")) {
      event.preventDefault();
      onremove(row.id);
    }
  }

  const name = $derived(row.input.split(/[\\/]/).pop() ?? row.input);

  /** The failure's headline and its second sentence, from the fatal code. */
  const failure = $derived.by(() => {
    const code = row.fatal?.code ?? "";
    switch (code) {
      case "E_PDF":
        return { head: t("error.notPdf"), note: t("error.sameResult"), retry: false };
      case "E_PASSWORD_REQUIRED":
        return { head: t("error.password"), note: t("error.passwordTried"), retry: false };
      case "E_LIMIT_EXCEEDED":
        return { head: t("error.limit"), note: t("error.limitHint"), retry: true };
      case "E_INPUT":
        return { head: t("error.unreadable"), note: t("error.code", { code }), retry: true };
      case "E_START":
        return { head: t("error.start"), note: t("error.code", { code: row.fatal?.message ?? code }), retry: true };
      case "E_EXIT":
        return { head: t("error.crashed"), note: t("error.code", { code: row.fatal?.message ?? code }), retry: true };
      default:
        return { head: t("error.failed"), note: t("error.code", { code }), retry: true };
    }
  });

  const status = $derived.by(() => {
    switch (row.phase) {
      case "queued":
        return t("queue.queued", { pos: row.position ?? 0 });
      case "running":
        if (row.stalled) return t("queue.notResponding");
        return row.current === null ? t("queue.converting") : stepLabel(row.current);
      case "cancelling":
        return t("queue.cancelling");
      case "cancelled":
        return t("queue.cancelled");
      case "complete":
        return t("queue.complete");
      case "failed":
        return failure.head;
    }
  });
</script>

<!-- The spec's roving tabindex is over the list items themselves (components.md, QueueList): one Tab
     stop into the list, arrows between rows, Delete on a waiting row. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<li
  class="oc-row"
  class:oc-row--running={row.phase === "running" && !row.stalled}
  class:oc-row--stalled={row.stalled}
  class:oc-row--cancelling={row.phase === "cancelling"}
  class:oc-row--cancelled={row.phase === "cancelled"}
  class:oc-row--failed={row.phase === "failed"}
  class:oc-row--compact={row.phase === "queued"}
  class:oc-row--expanded={row.phase === "complete" && row.expanded}
  aria-label={t("row.label", { file: name, status })}
  data-job={row.id}
  tabindex={tab}
  onkeydown={keydown}
>
  <div class="oc-row__line">
    {#if row.phase === "queued"}
      <span class="oc-tile"><Icon name="clock" size="md" /></span>
    {:else if row.phase === "running" && row.stalled}
      <span class="oc-tile oc-tile--warn"><Icon name="alert" size="md" /></span>
    {:else if row.phase === "running"}
      <span class="oc-tile oc-tile--accent oc-pulse" use:pulse={row.beats}><Icon name="file" size="md" /></span>
    {:else if row.phase === "cancelling"}
      <span class="oc-tile"><Spinner stopped={row.stalled} /></span>
    {:else if row.phase === "cancelled"}
      <span class="oc-tile"><Icon name="ban" size="md" /></span>
    {:else if row.phase === "complete" && invalid}
      <span class="oc-tile oc-tile--err"><Icon name="xcircle" size="md" /></span>
    {:else if row.phase === "complete"}
      <span class="oc-tile oc-tile--ok"><Icon name="check" size="md" /></span>
    {:else}
      <span class="oc-tile oc-tile--err"><Icon name="x" size="md" /></span>
    {/if}

    <div class="oc-row__text" role={row.phase === "failed" || row.stalled ? "alert" : undefined}>
      <span class="oc-row__name">{name}</span>
      <span class="oc-row__status">
        {#if row.phase === "queued"}
          <span class="oc-row__position">{status}</span>{#if row.position === 2}<span>· {t("queue.startsAfter")}</span>{/if}
        {:else if row.phase === "running" && row.stalled}
          <b>{status}</b><span>· {t("queue.notRespondingDetail", { s: silentSeconds(row, now) })}</span>
        {:else if row.phase === "running" && row.progress !== null && row.progress.step === row.current}
          <b>{status}</b><span class="oc-num">{countText(row.progress)}</span>
        {:else if row.phase === "running"}
          <Spinner stopped={row.stalled} /><b>{status}</b><span>· {stepOf(row)} · {t("queue.stillWorking")}</span>
        {:else if row.phase === "cancelled"}
          <b>{status}</b><span>· {t("queue.noFile")}</span>
        {:else if row.phase === "failed"}
          <b>{status}</b><span>{failure.note}</span>
        {:else if row.phase === "complete" && invalid}
          <b>{status}</b><span>· {t("result.savedInvalid")}</span>
        {:else}
          <b>{status}</b>
        {/if}
      </span>
    </div>

    {#if row.phase === "queued"}
      <button class="oc-btn oc-btn--quiet oc-btn--sm" tabindex={tab} aria-label={t("queue.removeFile", { file: name })} onclick={() => onremove(row.id)}>{t("queue.remove")}</button>
    {:else if row.phase === "running"}
      <button class="oc-btn oc-btn--sm" tabindex={tab} aria-label={t("queue.cancelFile", { file: name })} onclick={() => oncancel(row.id)}>{t("queue.cancel")}</button>
    {:else if row.phase === "cancelling"}
      <button class="oc-btn oc-btn--sm" tabindex={tab} disabled>{t("queue.cancelling")}</button>
    {:else if row.phase === "cancelled"}
      <button class="oc-btn oc-btn--sm" tabindex={tab} onclick={() => onretry(row)}>{t("queue.again")}</button>
      <button class="oc-btn oc-btn--quiet oc-btn--sm" tabindex={tab} aria-label={t("queue.removeFile", { file: name })} onclick={() => onremove(row.id)}>{t("queue.remove")}</button>
    {:else if row.phase === "complete"}
      {#if !row.expanded}<button class="oc-btn oc-btn--sm" tabindex={tab} onclick={() => onopen(row.id)}>{t("result.open")}</button>{/if}
      <button
        class="oc-btn oc-btn--icon oc-btn--sm"
        tabindex={tab}
        aria-label={row.expanded ? t("result.collapse") : t("queue.expand")}
        aria-expanded={row.expanded}
        onclick={() => ontoggle(row.id)}
      ><Icon name={row.expanded ? "chevup" : "chevdown"} size="md" /></button>
    {:else if row.phase === "failed"}
      {#if failure.retry}<button class="oc-btn oc-btn--sm" tabindex={tab} onclick={() => onretry(row)}>{t("queue.again")}</button>{/if}
      <button class="oc-btn oc-btn--quiet oc-btn--sm" tabindex={tab} aria-label={t("queue.removeFile", { file: name })} onclick={() => onremove(row.id)}>{t("queue.remove")}</button>
    {/if}
  </div>

  {#if row.phase === "running" && !row.stalled}
    <StageList {row} />
  {:else if row.phase === "complete" && row.expanded}
    {#if report !== null}
      <ResultPanel
        {row}
        {report}
        noReader={row.noReader}
        onopen={() => onopen(row.id)}
        onshow={() => onshow(row.id)}
        ondetails={() => ondetails(row.id)}
        onpage={onpage === null ? null : (page) => onpage(row.id, page)}
      />
    {:else if row.report === "unavailable"}
      <div class="oc-banner oc-banner--warn" role="status"><Icon name="alert" size="md" /><span class="oc-banner__text">{t("result.reportUnavailable")}</span></div>
    {/if}
  {/if}
</li>
