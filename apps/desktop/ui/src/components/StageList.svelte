<script lang="ts">
  // The user-facing stages as a checklist (components.md, StageList): done · current · pending ·
  // repair, the current one carrying `aria-current="step"` and — when the engine counts — its bar.
  import { stepState, visibleSteps, type Row } from "../lib/jobstate";
  import { countText, countValue, stepLabel } from "../lib/labels";
  import { t } from "../lib/locale.svelte";
  import { pulse } from "../lib/motion";
  import ProgressBar from "./ProgressBar.svelte";

  let { row }: { row: Row } = $props();
</script>

<ol class="oc-steps" aria-label={t("queue.stages")}>
  {#each visibleSteps(row) as step (step)}
    {@const state = stepState(row, step)}
    {@const count = row.progress !== null && row.progress.step === step && state !== "done" ? row.progress : null}
    <li class="oc-step oc-step--{state}" aria-current={state === "current" || state === "repair" ? "step" : undefined}>
      <div class="oc-step__line">
        {#if state === "done"}
          <span class="oc-step__mark"><svg class="oc-icon oc-step__check" aria-hidden="true"><use href="#i-check"></use></svg></span>
        {:else if state === "current" || state === "repair"}
          <span class="oc-step__mark oc-pulse" use:pulse={row.beats}></span>
        {:else}
          <span class="oc-step__mark"></span>
        {/if}
        <span class="oc-step__label">{stepLabel(step)}</span>
        {#if count !== null}<span class="oc-step__count">{countText(count)}</span>{/if}
      </div>
      {#if count !== null}
        <ProgressBar done={count.done} total={count.total} label={stepLabel(step)} valuetext={countValue(count)} />
      {/if}
    </li>
  {/each}
</ol>
