<script lang="ts">
  // Route `preview` (report.html §3): the generated XHTML in a frame, with the book's chapters,
  // page steps from its page list, and a label that cannot be hidden — "approximate preview —
  // your reading device may differ" (Phase 12 detail 7). The frame is sandboxed: no script, an
  // opaque origin, no IPC. It is never used as a check (D7).
  import { onMount } from "svelte";

  import Icon from "../../components/Icon.svelte";
  import type { Backend, PreviewIndex } from "../../lib/backend";
  import type { Row } from "../../lib/jobstate";
  import { t, tw } from "../../lib/locale.svelte";
  import type { ReportWarning } from "../../lib/report";

  let {
    row,
    backend,
    page = null,
    warning = null,
  }: { row: Row; backend: Backend; page?: string | null; warning?: ReportWarning | null } = $props();

  let index = $state<PreviewIndex | null>(null);
  let base = $state<string | null>(null);
  let failed = $state(false);
  let at = $state(0);

  /** Every place the preview can stand on, in reading order: the page list, else the chapters. */
  const stops = $derived(
    index === null ? [] : index.pages.length > 0 ? index.pages.map((p) => ({ label: p.label, href: p.href })) : index.chapters.map((c) => ({ label: c.title, href: c.href })),
  );
  const current = $derived(stops[at] ?? null);
  const file = (href: string) => href.split("#")[0] ?? href;
  const chapter = $derived(
    index?.chapters.filter((c) => current !== null && file(c.href) === file(current.href))[0] ?? null,
  );
  const src = $derived.by(() => {
    if (base === null || current === null) return null;
    const [path = "", fragment] = current.href.split("#");
    const encoded = [row.id, ...path.split("/")].map(encodeURIComponent).join("/");
    return `${base}${encoded}${fragment === undefined ? "" : `#${encodeURIComponent(fragment)}`}`;
  });

  onMount(async () => {
    try {
      index = await backend.previewIndex(row.id);
      base = await backend.previewBase();
      const wanted = page === null ? -1 : stops.findIndex((stop) => stop.label === page);
      at = wanted >= 0 ? wanted : 0;
    } catch {
      failed = true;
    }
  });

  function go(href: string) {
    const found = stops.findIndex((stop) => file(stop.href) === file(href));
    at = found >= 0 ? found : at;
  }

  /** ←/→ step through the pages (components.md, PreviewView). */
  function keydown(event: KeyboardEvent) {
    if (event.key === "ArrowLeft" && at > 0) at -= 1;
    else if (event.key === "ArrowRight" && at < stops.length - 1) at += 1;
    else return;
    event.preventDefault();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="oc-preview" role="region" aria-label={t("action.preview")} onkeydown={keydown}>
  {#if failed}
    <p class="oc-panel__muted">{t("preview.unavailable")}</p>
  {:else if index !== null}
    <nav class="oc-preview__toc" aria-label={t("preview.chapters")}>
      {#each index.chapters as entry, number (number)}
        <a
          href="#oc-chapter-{number}"
          class="oc-preview__chapter oc-preview__chapter--{Math.min(entry.level, 3)}"
          aria-current={chapter !== null && entry.href === chapter.href ? "true" : undefined}
          onclick={(event) => {
            event.preventDefault();
            go(entry.href);
          }}>{entry.title}</a
        >
      {/each}
    </nav>
    <div class="oc-preview__stage">
      <div class="oc-preview__label" role="note"><Icon name="info" />{t("preview.label")}</div>
      <div class="oc-preview__bar">
        <button class="oc-btn oc-btn--icon oc-btn--sm" aria-label={t("preview.previous")} disabled={at === 0} onclick={() => (at -= 1)}><Icon name="chevleft" size="md" /></button>
        <span class="oc-preview__where oc-num">{current === null ? "" : t("preview.pageOf", { page: current.label, pages: stops.length })}</span>
        <button class="oc-btn oc-btn--icon oc-btn--sm" aria-label={t("preview.next")} disabled={at >= stops.length - 1} onclick={() => (at += 1)}><Icon name="chevright" size="md" /></button>
        <span class="oc-preview__spacer"></span>
        {#if warning !== null}<span class="oc-warning oc-warning--{warning.severity} oc-preview__warning">{tw(warning.code, warning.args)}</span>{/if}
      </div>
      {#if src !== null}
        <iframe class="oc-preview__page oc-preview__frame" sandbox="" {src} title={t("preview.frame", { chapter: chapter?.title ?? t("preview.book") })}></iframe>
      {/if}
    </div>
  {/if}
</div>
