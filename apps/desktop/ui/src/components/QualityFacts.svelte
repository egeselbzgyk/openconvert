<script lang="ts">
  // The headline quality facts (Phase 12 detail 6): character retention, image parity, note
  // linkage, EPUBCheck — each read off report.json, none computed here.
  import { formatPercent } from "../lib/i18n";
  import { i18n, t } from "../lib/locale.svelte";
  import { notesLinked, type Report } from "../lib/report";

  let { report }: { report: Report } = $props();

  const notes = $derived(notesLinked(report));
  // Image parity is "every image extraction found is in the container" (Tier 1, OC-IMAGE-PARITY):
  // when it holds, all of them are there; when it does not, the report says only that it failed.
  const extracted = $derived(report.document.images_extracted);
  const images = $derived(
    extracted === 0
      ? t("facts.none")
      : report.validation.structural.image_parity
        ? t("facts.of", { a: extracted, b: extracted })
        : t("facts.mismatch"),
  );
</script>

<div class="oc-facts">
  <div class="oc-fact">
    <div class="oc-fact__value">{formatPercent(report.conservation.retention, i18n.locale, 1)}</div>
    <div class="oc-fact__label">{t("facts.retention")}</div>
  </div>
  <div class="oc-fact">
    <div class="oc-fact__value">{images}</div>
    <div class="oc-fact__label">{t("facts.images")}</div>
  </div>
  <div class="oc-fact">
    <div class="oc-fact__value">{notes.of === 0 ? t("facts.none") : t("facts.of", { a: notes.linked, b: notes.of })}</div>
    <div class="oc-fact__label">{notes.of === 0 ? t("facts.notesNone") : t("facts.notes")}</div>
  </div>
  <div class="oc-fact oc-fact--wide">
    <div class="oc-fact__value">{t("facts.notRun")}</div>
    <div class="oc-fact__label">{t("facts.epubcheck")} · {t("facts.packMissing")}</div>
  </div>
</div>
