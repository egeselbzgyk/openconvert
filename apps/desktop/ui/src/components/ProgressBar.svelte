<script lang="ts">
  // Determinate only: the width is a real `progress{done,total}` and nothing else. Not a live
  // region — the stage label carries the announcement (components.md, ProgressBar).
  let {
    done,
    total,
    label,
    valuetext,
  }: { done: number; total: number; label: string; valuetext: string } = $props();

  const share = $derived(total > 0 ? Math.min(done, total) / total : 0);
</script>

<div
  class="oc-progress"
  role="progressbar"
  aria-label={label}
  aria-valuemin={0}
  aria-valuemax={total}
  aria-valuenow={done}
  aria-valuetext={valuetext}
>
  <div class="oc-progress__fill" style:width="{share * 100}%"></div>
</div>
