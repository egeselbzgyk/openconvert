<script lang="ts">
  // "Previous conversions" (the main page's second section, under this session's queue): the books
  // of earlier runs of the app, newest first. A disclosure — its header says how many books it holds
  // and is open or closed as the user left it (`aria-expanded`, saved in the settings) — around a
  // list reached as the queue is: one Tab stop into it, ↑/↓ between rows, Home/End. Clearing the
  // list asks once; no book is touched either way.
  import { rovingTarget } from "../lib/a11y";
  import type { HistoryEntry } from "../lib/backend";
  import { t, tn } from "../lib/locale.svelte";
  import Dialog from "./Dialog.svelte";
  import HistoryRow from "./HistoryRow.svelte";
  import Icon from "./Icon.svelte";

  let {
    entries,
    open,
    noReader = {},
    ontoggle,
    onopen,
    onshow,
    onagain,
    onremove,
    onclear,
  }: {
    entries: HistoryEntry[];
    open: boolean;
    /** Entries whose "Open in reader" found no reader, by id. */
    noReader?: Record<string, boolean>;
    ontoggle: () => void;
    onopen: (id: string) => void;
    onshow: (id: string) => void;
    onagain: (entry: HistoryEntry) => void;
    onremove: (id: string) => void;
    onclear: () => void;
  } = $props();

  let active = $state(0);
  let list = $state<HTMLUListElement | null>(null);
  let clearing = $state(false);
  const current = $derived(Math.min(active, Math.max(entries.length - 1, 0)));

  function keydown(event: KeyboardEvent) {
    if (!(event.target instanceof HTMLLIElement)) return;
    const next = rovingTarget(event.key, current, entries.length);
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

<section class="oc-history" class:is-open={open} aria-labelledby="oc-history-title">
  <div class="oc-queue__head oc-history__head">
    <button class="oc-history__toggle" aria-expanded={open} aria-controls="oc-history-list" onclick={ontoggle}>
      <Icon name={open ? "chevdown" : "chevright"} size="md" />
      <span class="oc-queue__title" id="oc-history-title">{t("history.title")}</span>
      <span class="oc-queue__count">{tn("history.count", entries.length)}</span>
    </button>
    <span class="oc-queue__spacer"></span>
    {#if open}
      <button class="oc-btn oc-btn--quiet oc-btn--sm" onclick={() => (clearing = true)}>{t("history.clear")}</button>
    {/if}
  </div>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <ul
    id="oc-history-list"
    class="oc-queue oc-queue--scroll oc-history__list"
    aria-label={tn("history.listLabel", entries.length)}
    hidden={!open}
    bind:this={list}
    onkeydown={keydown}
    onfocusin={focusin}
  >
    {#each entries as entry, index (entry.id)}
      <HistoryRow {entry} active={index === current} noReader={noReader[entry.id] ?? false} {onopen} {onshow} {onagain} {onremove} />
    {/each}
  </ul>
</section>

{#if clearing}
  <Dialog
    title={tn("history.clearTitle", entries.length)}
    confirm={t("history.clearConfirm")}
    danger
    oncancel={() => (clearing = false)}
    onconfirm={() => {
      clearing = false;
      onclear();
    }}
  >
    <p>{t("history.clearBody")}</p>
  </Dialog>
{/if}
