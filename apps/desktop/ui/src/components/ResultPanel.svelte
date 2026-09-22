<script lang="ts">
  // The expanded completed row (route `result`, result.html §1): where the book is, what is in
  // it, whether it validated, its warnings with page links, the quality facts, and what can be
  // done next. A rendering of report.json; nothing summarised on the way.
  import { pluralCount } from "../lib/labels";
  import type { Row } from "../lib/jobstate";
  import { t } from "../lib/locale.svelte";
  import { llmDecisions, type Report } from "../lib/report";
  import Icon from "./Icon.svelte";
  import QualityFacts from "./QualityFacts.svelte";
  import ValidationLine from "./ValidationLine.svelte";

  let {
    row,
    report,
    noReader,
    onopen,
    onshow,
    ondetails,
    onpage = null,
  }: {
    row: Row;
    report: Report;
    noReader: boolean;
    onopen: () => void;
    onshow: () => void;
    ondetails: () => void;
    onpage?: ((page: string) => void) | null;
  } = $props();

  const base = (path: string) => path.split(/[\\/]/).pop() ?? path;
  const folder = (path: string) => path.slice(0, Math.max(path.length - base(path).length - 1, 0));
  const original = $derived(base(row.input).replace(/\.pdf$/i, ".epub"));
  const decisions = $derived(llmDecisions(report));
  const counts = $derived(
    [
      pluralCount("count.pages", report.input.pages),
      pluralCount("count.sections", report.document.sections),
      pluralCount("count.images", report.document.figures),
      pluralCount("count.tables", report.document.tables),
      pluralCount("count.notes", report.document.notes),
    ].join(" · "),
  );
</script>

<div class="oc-output">
  <div class="oc-output__file">
    <span class="oc-output__name">{base(row.output)}</span>
    <span class="oc-output__path">{folder(row.output)}</span>
  </div>
  <button class="oc-btn oc-btn--primary" onclick={onopen}>{t("result.open")}</button>
  <button class="oc-btn" onclick={onshow}>{t("result.show")}</button>
</div>
{#if row.renamed}
  <div class="oc-banner oc-banner--info" role="status"><Icon name="info" size="md" /><span class="oc-banner__text">{t("result.renamed", { original, name: base(row.output) })}</span></div>
{/if}
{#if noReader}
  <div class="oc-banner oc-banner--info" role="status"><Icon name="info" size="md" /><span class="oc-banner__text">{t("result.noReader", { folder: folder(row.output) })}</span><button class="oc-btn oc-btn--sm" onclick={onshow}>{t("result.show")}</button></div>
{/if}
<div class="oc-summary">
  <span class="oc-summary__counts">{counts}</span>
  <span>{t("result.deterministic")}{#if decisions !== null} · {t("result.aiDecisions", { n: decisions })}{/if}</span>
</div>
<ValidationLine {report} {onpage} />
<QualityFacts {report} />
<div class="oc-actions">
  <button class="oc-btn oc-btn--sm" onclick={ondetails}>{t("action.details")}</button>
</div>
