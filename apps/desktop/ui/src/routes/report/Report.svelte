<script lang="ts">
  // Route `report` (screen-map.md): the job's report.json rendered by ReportView.
  import ReportView from "../../components/ReportView.svelte";
  import type { Row } from "../../lib/jobstate";
  import { t } from "../../lib/locale.svelte";

  let {
    row,
    appVersion,
    onpage = null,
  }: { row: Row; appVersion: string; onpage?: ((page: string) => void) | null } = $props();
</script>

{#if row.report !== null && row.report !== "unavailable"}
  <ReportView report={row.report} {appVersion} input={row.input} output={row.output} {onpage} />
{:else}
  <main class="oc-main"><p class="oc-panel__muted">{t("result.reportUnavailable")}</p></main>
{/if}
