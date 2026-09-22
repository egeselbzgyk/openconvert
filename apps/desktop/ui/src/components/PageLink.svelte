<script lang="ts">
  // "p. 34" (components.md, PageLink): opens the preview at that page; disabled — and described as
  // such — when there is no preview to open.
  import { t } from "../lib/locale.svelte";

  let { page, onpage = null }: { page: string; onpage?: ((page: string) => void) | null } = $props();
  const noPreview = `oc-nopreview-${Math.random().toString(36).slice(2)}`;
</script>

{#if onpage === null}
  <button class="oc-pagelink" disabled aria-describedby={noPreview}>{t("page.short", { n: page })}</button>
  <span class="oc-sr-only" id={noPreview}>{t("page.noPreview")}</span>
{:else}
  <button class="oc-pagelink" aria-label={t("page.goto", { n: page })} onclick={() => onpage(page)}>{t("page.short", { n: page })}</button>
{/if}
