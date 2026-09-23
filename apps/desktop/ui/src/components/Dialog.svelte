<script lang="ts">
  // A modal dialog (components.md, Dialog): flat scrim, focus trapped, opens on the safe button,
  // Esc cancels, focus returns to the opener. `role="dialog"`, labelled by its title.
  import type { Snippet } from "svelte";

  import { trapFocus } from "../lib/a11y";
  import { t } from "../lib/locale.svelte";
  import { focusOnMount } from "../lib/motion";

  let {
    title,
    confirm,
    danger = false,
    wide = false,
    cancel = t("dialog.cancel"),
    onconfirm,
    oncancel,
    children,
  }: {
    title: string;
    confirm: string;
    danger?: boolean;
    wide?: boolean;
    /** The safe button's label; `null` for a dialog with one button, which is then the safe one. */
    cancel?: string | null;
    onconfirm: () => void;
    oncancel: () => void;
    children?: Snippet;
  } = $props();

  const id = `oc-dialog-${Math.random().toString(36).slice(2)}`;
</script>

<div class="oc-scrim" aria-hidden="true"></div>
<div class="oc-dialog" class:oc-dialog--wide={wide} role="dialog" aria-modal="true" aria-labelledby={id} use:trapFocus={oncancel}>
  <div class="oc-dialog__head"><h2 class="oc-dialog__title" {id}>{title}</h2></div>
  {@render children?.()}
  <div class="oc-actions oc-actions--end">
    {#if cancel !== null}
      <button class="oc-btn" use:focusOnMount onclick={oncancel}>{cancel}</button>
      <button class="oc-btn" class:oc-btn--danger={danger} class:oc-btn--primary={!danger} onclick={onconfirm}>{confirm}</button>
    {:else}
      <button class="oc-btn oc-btn--primary" use:focusOnMount onclick={onconfirm}>{confirm}</button>
    {/if}
  </div>
</div>
