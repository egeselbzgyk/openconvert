<script lang="ts">
  // One book of an earlier run ("Previous conversions"): what it is, how and when it ended, and what
  // can be done with it — open it in the reader, show it in its folder, convert a failed one again,
  // or take it off the list (the book itself is never touched). Its keyboard is QueueRow's: the
  // list's roving tabindex reaches the row and its buttons, Delete takes it off the list.
  import type { HistoryEntry } from "../lib/backend";
  import { durationText, entryName, failureKey, whenText } from "../lib/history";
  import { i18n, t, tn } from "../lib/locale.svelte";
  import Icon from "./Icon.svelte";

  let {
    entry,
    active = true,
    noReader = false,
    onopen,
    onshow,
    onagain,
    onremove,
  }: {
    entry: HistoryEntry;
    /** The roving-tabindex row: the one Tab lands on (HistoryList). */
    active?: boolean;
    /** "Open in reader" found no reader on this computer. */
    noReader?: boolean;
    onopen: (id: string) => void;
    onshow: (id: string) => void;
    onagain: (entry: HistoryEntry) => void;
    onremove: (id: string) => void;
  } = $props();

  const name = $derived(entryName(entry));
  /** A book was written — whether or not it is still there. */
  const written = $derived(entry.status !== "failed");
  const gone = $derived(written && !entry.outputExists);
  const status = $derived(
    entry.status === "complete" ? t("history.converted") : entry.status === "invalid" ? t("history.invalid") : t("history.failed"),
  );
  const tab = $derived(active ? 0 : -1);
  const base = (path: string) => path.split(/[\\/]/).pop() ?? path;
  const folder = (path: string) => path.slice(0, Math.max(path.length - base(path).length - 1, 0));

  function keydown(event: KeyboardEvent) {
    if (event.target !== event.currentTarget) return;
    if (event.key === "Delete") {
      event.preventDefault();
      onremove(entry.id);
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<li
  class="oc-row oc-row--compact"
  class:oc-row--gone={gone}
  aria-label={t("row.label", { file: name, status })}
  data-entry={entry.id}
  tabindex={tab}
  onkeydown={keydown}
>
  <div class="oc-row__line">
    {#if entry.status === "complete"}
      <span class="oc-tile oc-tile--ok"><Icon name="check" size="md" /></span>
    {:else if entry.status === "invalid"}
      <span class="oc-tile oc-tile--err"><Icon name="xcircle" size="md" /></span>
    {:else}
      <span class="oc-tile oc-tile--err"><Icon name="x" size="md" /></span>
    {/if}

    <div class="oc-row__text">
      <span class="oc-row__name">{name}</span>
      <span class="oc-row__status">
        <b>{status}</b>
        <span class="oc-num">· {whenText(entry.finishedAt, i18n.locale)}</span>
        {#if entry.title !== null}<span>· {entry.inputName}</span>{/if}
        {#if entry.pages !== null}<span class="oc-num">· {tn("history.pages", entry.pages)}</span>{/if}
        {#if written && entry.durationMs !== null}<span class="oc-num">· {t("history.took", { duration: durationText(entry.durationMs, i18n.locale) })}</span>{/if}
        {#if !written}<span>· {t(failureKey(entry))}</span>{/if}
        {#if gone}<span>· {t("history.gone")}</span>{/if}
      </span>
    </div>

    {#if written && !gone}
      <button class="oc-btn oc-btn--sm" tabindex={tab} onclick={() => onopen(entry.id)}>{t("result.open")}</button>
    {:else if !written}
      <button class="oc-btn oc-btn--sm" tabindex={tab} onclick={() => onagain(entry)}>{t("queue.again")}</button>
    {/if}
    <button class="oc-btn oc-btn--sm" tabindex={tab} onclick={() => onshow(entry.id)}>{t("result.show")}</button>
    <button
      class="oc-btn oc-btn--icon oc-btn--sm"
      tabindex={tab}
      aria-label={t("history.removeFile", { file: name })}
      title={t("history.removeFile", { file: name })}
      onclick={() => onremove(entry.id)}><Icon name="trash" size="md" /></button
    >
  </div>
  {#if noReader && !gone}
    <div class="oc-row__detail" role="status"><span class="oc-field__help">{t("result.noReader", { folder: folder(entry.output) })}</span></div>
  {/if}
</li>
