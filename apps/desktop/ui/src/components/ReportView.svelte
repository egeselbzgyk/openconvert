<script lang="ts">
  // The conversion report as a readable page (route `report`, report.html §1–2): a rendering of
  // report.json — the same data the engine's own invariant checks produced — never a second
  // summary that could drift from it (UI_UX §5).
  import { formatPercent, formatValue } from "../lib/i18n";
  import { i18n, t, tn } from "../lib/locale.svelte";
  import { pluralCount } from "../lib/labels";
  import { llmDecisions, stepTimings, totalMs, warningsByPage, type Report } from "../lib/report";
  import ValidationLine from "./ValidationLine.svelte";
  import WarningLine from "./WarningLine.svelte";

  let {
    report,
    appVersion,
    input,
    output,
    onpage = null,
  }: {
    report: Report;
    appVersion: string;
    input: string;
    output: string;
    onpage?: ((page: string) => void) | null;
  } = $props();

  const base = (path: string) => path.split(/[\\/]/).pop() ?? path;
  const seconds = (ms: number) => t("report.seconds", { s: formatValue(Math.round(ms / 100) / 10, i18n.locale) });
  const steps = $derived(stepTimings(report));
  const total = $derived(totalMs(report));
  const findings = $derived(report.validation.tier1.findings.length);
  const imagePages = $derived(report.page_classes["image-only"] ?? 0);
  const decisions = $derived(llmDecisions(report));
  const language = $derived.by(() => {
    if (report.document.language === "und") return t("report.unknownLanguage");
    try {
      return new Intl.DisplayNames([i18n.locale], { type: "language" }).of(report.document.language) ?? report.document.language;
    } catch {
      return report.document.language;
    }
  });
  const c0 = $derived(report.conservation.c0_chars);
</script>

<div class="oc-report">
  <div class="oc-report__title">
    <span class="oc-report__file">{base(input)}</span>
    <span class="oc-report__arrow">→ {base(output)}</span>
  </div>
  <div class="oc-report__cols">
    <div class="oc-report__col">
      <section class="oc-panel">
        <h2 class="oc-panel__title">{t("report.contents")}</h2>
        <div class="oc-report__chips">
          <span class="oc-chip">{pluralCount("count.pages", report.input.pages)}</span>
          <span class="oc-chip">{pluralCount("count.sections", report.document.sections)}</span>
          <span class="oc-chip">{pluralCount("count.images", report.document.figures)}</span>
          <span class="oc-chip">{pluralCount("count.tables", report.document.tables)}</span>
          <span class="oc-chip">{pluralCount("count.notes", report.document.notes)}</span>
        </div>
      </section>
      <section class="oc-panel">
        <h2 class="oc-panel__title">{t("report.made")}</h2>
        <div>{t("report.deterministicComplete")}{#if decisions !== null} · {t("result.aiDecisions", { n: decisions })}{/if}</div>
        {#if decisions === null}<div class="oc-panel__muted">{t("report.aiOff")}</div>{/if}
      </section>
      <section class="oc-panel">
        <h2 class="oc-panel__title">{t("report.checks")}</h2>
        <ValidationLine report={{ ...report, warnings: [] }} />
        <dl class="oc-kv">
          <div class="oc-kv__row">
            <dt>{t("report.tier1")}</dt>
            <dd>{report.validation.tier1.valid ? tn("report.tier1Passed", findings) : tn("report.tier1Failed", findings)}</dd>
          </div>
          <div class="oc-kv__row"><dt>{t("facts.epubcheck")}</dt><dd>{t("report.epubcheckNotRun")}</dd></div>
        </dl>
      </section>
      <section class="oc-panel">
        <h2 class="oc-panel__title">{t("report.warnings")}</h2>
        {#if report.warnings.length === 0}
          <div class="oc-empty"><span class="oc-empty__title">{t("report.noWarnings")}</span><span>{t("report.noWarningsLine")}</span></div>
        {:else}
          <ul class="oc-validation__list oc-report__warnings">
            {#each warningsByPage(report) as warning, index (index)}
              <WarningLine {warning} {onpage} />
            {/each}
          </ul>
        {/if}
      </section>
    </div>
    <div class="oc-report__col">
      <section class="oc-panel">
        <h2 class="oc-panel__title">{t("report.time")} <span class="oc-panel__aside">· {t("report.timeTotal", { s: seconds(total) })}</span></h2>
        <div class="oc-timeline" aria-hidden="true">
          {#each steps as step, index (index)}<span class="oc-timeline__seg" style:flex={String(Math.max(step.ms, 1))}></span>{/each}
        </div>
        <table class="oc-table">
          <thead><tr><th>{t("report.step")}</th><th class="is-num">{t("report.timeColumn")}</th></tr></thead>
          <tbody>
            {#each steps as step, index (index)}<tr><td>{t(step.label)}</td><td class="is-num">{seconds(step.ms)}</td></tr>{/each}
          </tbody>
        </table>
      </section>
      <section class="oc-panel">
        <h2 class="oc-panel__title">{t("report.ledger")} <span class="oc-panel__aside">· {t("report.ledgerNote")}</span></h2>
        {#if report.conservation.per_reason.length === 0}
          <p class="oc-panel__muted oc-report__para">{t("report.ledgerEmpty")}</p>
        {:else}
          <table class="oc-table">
            <thead><tr><th>{t("report.reason")}</th><th class="is-num">{t("report.characters")}</th><th class="is-num">{t("report.share")}</th></tr></thead>
            <tbody>
              {#each report.conservation.per_reason as total (total.reason)}
                <tr>
                  <td>{t(`reason.${total.reason}`)} <span class="oc-mono oc-report__raw">{total.reason}</span></td>
                  <td class="is-num">{formatValue(total.removed_chars + total.added_chars, i18n.locale)}</td>
                  <td class="is-num">
                    {#if total.added_chars > 0 && total.removed_chars > 0}
                      {t("report.paired")}
                    {:else}
                      {formatPercent(c0 > 0 ? total.net_removed / c0 : 0, i18n.locale, 2)}
                      <span class="oc-mono oc-report__raw">{t("report.shareRaw", { x: formatValue(Number(total.measured.toFixed(4)), i18n.locale) })}</span>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      </section>
      <section class="oc-panel">
        <h2 class="oc-panel__title">{t("report.details")}</h2>
        <dl class="oc-kv">
          <div class="oc-kv__row"><dt>{t("report.docType")}</dt><dd>{t(`doctype.${report.document.classification}`)}</dd></div>
          <div class="oc-kv__row"><dt>{t("report.preset")}</dt><dd>{t(`preset.${report.document.preset}`)}</dd></div>
          <div class="oc-kv__row"><dt>{t("report.language")}</dt><dd>{language}</dd></div>
          <div class="oc-kv__row"><dt>{t("report.images")}</dt><dd>{imagePages === 0 ? t("report.noPages") : tn("report.imagePages", imagePages)}</dd></div>
          <div class="oc-kv__row">
            <dt>{t("report.versions")}</dt>
            <dd class="oc-mono">{t("report.versionsLine", { app: appVersion, engine: report.engine.version, ir: report.engine.ir_version, pdfium: report.engine.pdfium_version })}</dd>
          </div>
        </dl>
      </section>
    </div>
  </div>
</div>
