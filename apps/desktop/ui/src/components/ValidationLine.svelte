<script lang="ts">
  // "Validation: passed / passed with warnings / invalid" — never omitted, never softened
  // (UI_UX §2.3). Text and icon, never colour alone (components.md, ValidationLine).
  import { t } from "../lib/locale.svelte";
  import { verdict, type Report } from "../lib/report";
  import Icon from "./Icon.svelte";
  import WarningLine from "./WarningLine.svelte";

  let { report, onpage = null }: { report: Report; onpage?: ((page: string) => void) | null } = $props();

  const state = $derived(verdict(report));
  const head = $derived(
    state === "passed"
      ? t("validation.passed")
      : state === "warn"
        ? t("validation.warn", { n: report.warnings.length })
        : t("validation.invalid"),
  );
</script>

<div class="oc-validation oc-validation--{state}">
  <span class="oc-validation__head"><Icon name={state === "passed" ? "ccheck" : state === "warn" ? "alert" : "xcircle"} size="md" />{head}</span>
  {#if state === "invalid"}<span class="oc-validation__note">{t("validation.invalidNote")}</span>{/if}
  {#if report.warnings.length > 0}
    <ul class="oc-validation__list">
      {#each report.warnings as warning, index (index)}
        <WarningLine {warning} {onpage} />
      {/each}
    </ul>
  {/if}
</div>
