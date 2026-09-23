<script lang="ts">
  // "Review TOC" (result.html §3, TocEditor in components.md): each heading renamed or moved to
  // another level, labelled with its page; a changed heading is marked. What changed goes to "Fix
  // and rebuild", which keeps it with the book's other corrections and rebuilds from cache.
  import type { TocPatch } from "../../lib/backend";
  import { headingChanged, tocDraft, tocPatch } from "../../lib/corrections";
  import { t, tn } from "../../lib/locale.svelte";
  import { focusOnMount } from "../../lib/motion";
  import type { Report } from "../../lib/report";

  let {
    report,
    file,
    onsave,
    oncancel,
  }: {
    report: Report;
    file: string;
    onsave: (patch: TocPatch[]) => void;
    oncancel: () => void;
  } = $props();

  /** XHTML's six heading levels, `h1`–`h6`. */
  const LEVELS = [1, 2, 3, 4, 5, 6] as const;

  // svelte-ignore state_referenced_locally
  let draft = $state(tocDraft(report));
  const patch = $derived(tocPatch(report, draft));

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (patch.length > 0) onsave(patch);
  }
</script>

<main class="oc-editor">
<form class="oc-editor__form" onsubmit={submit}>
  <h2 class="oc-settings__title" tabindex="-1" use:focusOnMount>{t("editor.tocTitle", { file })}</h2>
  <p class="oc-field__help oc-editor__help">{t("editor.tocHelp")}</p>
  {#if draft.length === 0}
    <p class="oc-panel__muted">{t("editor.noHeadings")}</p>
  {:else}
    <ol class="oc-toc" aria-label={t("editor.tocLabel")}>
      {#each draft as entry (entry.heading)}
        <li class="oc-toc__item" class:is-changed={headingChanged(report, entry)} data-level={entry.level}>
          <input class="oc-input" bind:value={entry.title} aria-label={t("editor.heading", { page: entry.page })} />
          <select class="oc-select oc-toc__level" bind:value={entry.level} aria-label={t("editor.level")}>
            {#each LEVELS as level (level)}
              <option value={level}>{t("editor.levelN", { n: level })}</option>
            {/each}
          </select>
          <span class="oc-toc__page">{t("editor.page", { page: entry.page })}</span>
        </li>
      {/each}
    </ol>
  {/if}
  <div class="oc-actions">
    <button type="submit" class="oc-btn oc-btn--primary" disabled={patch.length === 0}>
      {patch.length === 0 ? t("action.fixRebuild") : tn("editor.rebuildN", patch.length)}
    </button>
    <button type="button" class="oc-btn oc-btn--quiet" onclick={oncancel}>{t("editor.cancel")}</button>
  </div>
</form>
</main>
