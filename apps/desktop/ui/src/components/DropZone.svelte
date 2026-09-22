<script lang="ts">
  // The drop target (components.md, DropZone). Its one Tab stop is "Select PDF…"; the native drop
  // itself is Tauri's, which delivers absolute paths (Phase 12 detail 2). While something is being
  // dragged over the window the zone says what a drop would do — how many PDFs, and which files
  // would be skipped — before anything happens.
  import { t, tn } from "../lib/locale.svelte";
  import Icon from "./Icon.svelte";

  let {
    strip,
    dragging,
    onselect,
  }: {
    strip: boolean;
    dragging: { pdfs: number; skipped: string[] } | null;
    onselect: () => void;
  } = $props();
</script>

<div
  class="oc-dropzone"
  class:oc-dropzone--strip={strip}
  class:is-dragover={dragging !== null && dragging.skipped.length === 0}
  class:is-dragover-mixed={dragging !== null && dragging.skipped.length > 0}
  role="group"
  aria-label={t("drop.label")}
>
  {#if dragging !== null}
    {#if !strip}<span class="oc-tile oc-tile--lg oc-tile--accent"><Icon name="fileplus" size="xl" /></span>{/if}
    <span class="oc-dropzone__title">{tn("drop.release", dragging.pdfs)}</span>
    {#if dragging.skipped.length > 0}
      <span class="oc-dropzone__skip"><Icon name="alert" />{tn("drop.skipMany", dragging.skipped.length, { files: dragging.skipped.join(", ") })}</span>
    {:else}
      <span class="oc-dropzone__hint">{t("drop.each")}</span>
    {/if}
  {:else if strip}
    <span class="oc-tile oc-tile--accent"><Icon name="fileplus" size="md" /></span>
    <span class="oc-dropzone__title">{t("drop.more")}</span>
    <button class="oc-btn oc-btn--sm" onclick={onselect}>{t("drop.select")}</button>
  {:else}
    <span class="oc-tile oc-tile--lg oc-tile--accent"><Icon name="fileplus" size="xl" /></span>
    <span class="oc-dropzone__title">{t("drop.title")}</span>
    <span class="oc-dropzone__or">{t("drop.or")} <button class="oc-btn oc-btn--primary" onclick={onselect}>{t("drop.select")}</button></span>
    <span class="oc-dropzone__hint">{t("drop.hint")}</span>
  {/if}
</div>
