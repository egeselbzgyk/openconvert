<script lang="ts">
  // A number with its unit (components.md, NumberWithUnit): the unit is part of the label ("Max
  // memory, GB"), ↑/↓ step natively, an invalid entry says so and is not saved.
  import { t } from "../lib/locale.svelte";

  let {
    label,
    unit,
    value,
    min,
    help,
    onchange,
  }: {
    label: string;
    unit: string;
    value: number;
    min: number;
    help: string;
    onchange: (value: number) => void;
  } = $props();

  const id = `oc-number-${Math.random().toString(36).slice(2)}`;
  let invalid = $state(false);

  function input(event: Event) {
    const text = (event.currentTarget as HTMLInputElement).value;
    const number = Number(text);
    invalid = !(Number.isInteger(number) && number >= min);
    if (!invalid) onchange(number);
  }
</script>

<div class="oc-unit">
  <input
    class="oc-input"
    type="number"
    {min}
    step="1"
    {value}
    aria-label="{label}, {unit}"
    aria-invalid={invalid || undefined}
    aria-describedby="{id}-help{invalid ? ` ${id}-error` : ''}"
    onchange={input}
  />
  <span class="oc-unit__label">{unit}</span>
</div>
<span class="oc-sr-only" id="{id}-help">{help}</span>
{#if invalid}<span class="oc-field__error" id="{id}-error">{t("settings.advanced.invalid", { min })}</span>{/if}
