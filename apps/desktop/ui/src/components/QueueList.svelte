<script lang="ts">
  // The queue (components.md, QueueList): one Tab stop into the list, ↑/↓ between rows (roving
  // tabindex), Home/End; `role="list"` labelled with the file count. Forty rows scroll inside the
  // window while the drop strip and the header stay put (queue.html §3). It is this session's
  // section of the main page; earlier runs' books follow it (HistoryList), and then it takes only
  // the height its rows need.
  import type { Snippet } from "svelte";

  import { rovingTarget } from "../lib/a11y";
  import type { Row } from "../lib/jobstate";
  import { t, tn } from "../lib/locale.svelte";

  let {
    rows,
    onremoveall,
    fit = false,
    row: rowSnippet,
  }: {
    rows: Row[];
    onremoveall: () => void;
    /** Another section follows: take the rows' height, scrolling only when the window is full. */
    fit?: boolean;
    row: Snippet<[Row, boolean]>;
  } = $props();

  let active = $state(0);
  let list = $state<HTMLUListElement | null>(null);

  const converting = $derived(rows.filter((row) => row.phase === "running" || row.phase === "cancelling").length);
  const waiting = $derived(rows.filter((row) => row.phase === "queued").length);
  const current = $derived(Math.min(active, Math.max(rows.length - 1, 0)));

  function keydown(event: KeyboardEvent) {
    if (!(event.target instanceof HTMLLIElement)) return;
    const next = rovingTarget(event.key, current, rows.length);
    if (next === null) return;
    event.preventDefault();
    active = next;
    list?.querySelectorAll<HTMLElement>(":scope > li")[next]?.focus();
  }

  function focusin(event: FocusEvent) {
    const items = [...(list?.querySelectorAll<HTMLElement>(":scope > li") ?? [])];
    const index = items.findIndex((item) => item.contains(event.target as Node));
    if (index >= 0) active = index;
  }
</script>

<div class="oc-queue__head">
  <span class="oc-queue__title">{t("queue.session")}</span>
  <span class="oc-queue__count">{tn("queue.count", rows.length, { r: converting, w: waiting })}</span>
  <span class="oc-queue__spacer"></span>
  {#if waiting > 1}
    <button class="oc-btn oc-btn--quiet oc-btn--sm" onclick={onremoveall}>{t("queue.removeAll")}</button>
  {/if}
</div>
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<ul
  class="oc-queue oc-queue--scroll"
  class:oc-queue--fit={fit}
  aria-label={tn("queue.listLabel", rows.length)}
  bind:this={list}
  onkeydown={keydown}
  onfocusin={focusin}
>
  {#each rows as row, index (row.id)}
    {@render rowSnippet(row, index === current)}
  {/each}
</ul>
