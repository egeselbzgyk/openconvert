<script lang="ts">
  // A radio group (components.md, RadioGroup): `role="radiogroup"` / `role="radio"`, one Tab stop
  // with a roving tabindex, ↑/↓ move and select.
  import { rovingTarget } from "../lib/a11y";

  let {
    label,
    options,
    value,
    disabled = false,
    onchange = () => undefined,
  }: {
    label: string;
    options: Array<{ value: string; label: string; hint?: string }>;
    value: string;
    disabled?: boolean;
    onchange?: (value: string) => void;
  } = $props();

  let group = $state<HTMLDivElement | null>(null);
  const selected = $derived(Math.max(options.findIndex((option) => option.value === value), 0));

  function keydown(event: KeyboardEvent) {
    if (disabled) return;
    const next = rovingTarget(event.key, selected, options.length);
    if (next === null) return;
    event.preventDefault();
    const option = options[next];
    if (option !== undefined) onchange(option.value);
    group?.querySelectorAll<HTMLElement>('[role="radio"]')[next]?.focus();
  }
</script>

<div class="oc-radios" role="radiogroup" aria-label={label} aria-disabled={disabled || undefined} bind:this={group} tabindex="-1" onkeydown={keydown}>
  {#each options as option, index (option.value)}
    <div
      class="oc-radio"
      class:is-disabled={disabled}
      role="radio"
      aria-checked={option.value === value}
      aria-disabled={disabled || undefined}
      tabindex={index === selected ? 0 : -1}
      onclick={() => !disabled && onchange(option.value)}
      onkeydown={(event) => {
        if (!disabled && event.key === " ") {
          event.preventDefault();
          onchange(option.value);
        }
      }}
    >
      <span class="oc-radio__dot"></span>
      <span>{option.label}{#if option.hint}<br /><span class="oc-radio__hint">{option.hint}</span>{/if}</span>
    </div>
  {/each}
</div>
