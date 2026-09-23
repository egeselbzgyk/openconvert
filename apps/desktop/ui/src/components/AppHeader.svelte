<script lang="ts">
  // The header is last in the DOM and drawn first (`order: -1`), so Tab goes drop zone → queue →
  // Settings (design decision 4/5). On the other routes it carries a way back and the route title.
  import { t } from "../lib/locale.svelte";
  import Icon from "./Icon.svelte";

  let {
    title = null,
    back = null,
    onsettings = null,
    extra,
  }: {
    title?: string | null;
    back?: { label: string; onclick: () => void } | null;
    onsettings?: (() => void) | null;
    extra?: import("svelte").Snippet;
  } = $props();
</script>

<header class="oc-header">
  {#if back !== null}<button class="oc-btn oc-btn--sm" onclick={back.onclick}>{back.label}</button>{/if}
  <span class="oc-header__title">
    {#if title === null}<svg class="oc-logo" aria-hidden="true"><use href="#oc-logo-sm"></use></svg>{t("app.title")}{:else}{title}{/if}
  </span>
  <span class="oc-header__spacer"></span>
  {@render extra?.()}
  {#if onsettings !== null}
    <button class="oc-btn oc-btn--sm" onclick={onsettings}><Icon name="gear" size="md" />{t("app.settings")}</button>
  {/if}
</header>
