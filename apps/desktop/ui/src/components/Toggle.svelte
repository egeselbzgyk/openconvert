<script lang="ts">
  // A switch (components.md, Toggle): `role="switch"`, `aria-checked`, Space toggles (native
  // button behaviour), the state also in words.
  import { t } from "../lib/locale.svelte";

  let {
    checked,
    label,
    disabled = false,
    onchange = () => undefined,
    describedby = undefined,
  }: {
    checked: boolean;
    label: string;
    disabled?: boolean;
    onchange?: (checked: boolean) => void;
    describedby?: string;
  } = $props();
</script>

<button
  class="oc-toggle"
  role="switch"
  aria-checked={checked}
  aria-label={label}
  aria-describedby={describedby}
  {disabled}
  onclick={() => onchange(!checked)}
><span class="oc-toggle__track"><span class="oc-toggle__thumb"></span></span>{checked ? t("toggle.on") : t("toggle.off")}</button>
