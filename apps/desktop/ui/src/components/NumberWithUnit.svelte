<script lang="ts">
  // A number with its unit (components.md, NumberWithUnit): the unit is part of the label ("Max
  // memory, GB"), ↑/↓ step natively, an invalid entry says so and is not saved. With a `max`, the
  // message names the whole range.
  import { t } from "../lib/locale.svelte";

  let {
    label,
    unit,
    value,
    min,
    max = null,
    help,
    onchange,
  }: {
    label: string;
    unit: string;
    value: number;
    min: number;
    max?: number | null;
    help: string;
    onchange: (value: number) => void;
  } = $props();

  const id = `oc-number-${Math.random().toString(36).slice(2)}`;
  let invalid = $state(false);
  /** The value this field last showed or sent. Another one arriving was set from outside (a
      reset): it replaces a refused entry, which then no longer needs its message. */
  let known: number | null = null;
  $effect(() => {
    const next = value;
    if (known !== null && next !== known) invalid = false;
    known = next;
  });

  function change(event: Event) {
    const text = (event.currentTarget as HTMLInputElement).value;
    const number = Number(text);
    invalid = !(Number.isInteger(number) && number >= min && (max === null || number <= max));
    if (invalid) return;
    known = number;
    onchange(number);
  }
</script>

<div class="oc-unit">
  <input
    class="oc-input"
    type="number"
    {min}
    max={max ?? undefined}
    step="1"
    {value}
    aria-label="{label}, {unit}"
    aria-invalid={invalid || undefined}
    aria-describedby="{id}-help{invalid ? ` ${id}-error` : ''}"
    onchange={change}
  />
  <span class="oc-unit__label">{unit}</span>
</div>
<span class="oc-sr-only" id="{id}-help">{help}</span>
{#if invalid}<span class="oc-field__error" id="{id}-error"
    >{max === null ? t("settings.advanced.invalid", { min }) : t("settings.advanced.invalidRange", { min, max })}</span
  >{/if}
