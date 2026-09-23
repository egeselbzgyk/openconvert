<script lang="ts">
  // FirstRunCard (components.md, queue.html §1, UI_UX §3): the AI opt-in, offered once, near the
  // drop zone, never as a modal. Its costs are the default model's `ModelReadiness` — the download
  // size and the RAM estimate — never the mockup's numbers. "Set up AI assistance" opens the model
  // manager at the default model (route `firstrun`); "Not now" hides the card and it does not come
  // back to the main window (design decision 10).
  import type { ModelRow } from "../lib/backend";
  import { formatBytes } from "../lib/bytes";
  import { i18n, t } from "../lib/locale.svelte";

  let { model, onsetup, onnotnow }: { model: ModelRow; onsetup: () => void; onnotnow: () => void } = $props();
</script>

<section class="oc-card oc-card--firstrun" aria-label={t("settings.ai.label")}>
  <p>{t("firstrun.body")}</p>
  <div class="oc-card__costs">
    <span>{t("firstrun.costs")}</span>
    <span class="oc-chip">{t("firstrun.cost.download", { size: formatBytes(model.size_bytes, i18n.locale) })}</span>
    <span class="oc-chip">{t("firstrun.cost.ram", { size: formatBytes(model.ram_estimate_bytes, i18n.locale) })}</span>
    <span class="oc-chip">{t("firstrun.cost.time")}</span>
  </div>
  <div class="oc-card__actions">
    <button class="oc-btn" onclick={onsetup}>{t("firstrun.setup")}</button>
    <button class="oc-btn oc-btn--quiet" onclick={onnotnow}>{t("firstrun.notnow")}</button>
  </div>
</section>
