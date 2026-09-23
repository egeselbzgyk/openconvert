<script lang="ts">
  // ModelRow / PackRow (components.md): `.oc-model`, one row of the models or packs screen. It
  // renders the manager's row and nothing else — size, RAM estimate, CPU expectation, licence and
  // licence path are the `ModelReadiness` fields (a pack's row has no RAM or CPU), and progress is
  // the bytes the download has actually received. The licence is always shown, in full, and
  // accepted before the first download (UI_UX §2.4); the Rust side refuses a download otherwise.
  import Icon from "./Icon.svelte";
  import ProgressBar from "./ProgressBar.svelte";
  import type { LicenseView, ModelRow, PackRow } from "../lib/backend";
  import { formatBytes } from "../lib/bytes";
  import { formatPercent } from "../lib/i18n";
  import { i18n, t } from "../lib/locale.svelte";

  let {
    row,
    license = null,
    expanded = false,
    error = null,
    onexpand,
    oncollapse,
    onaccept,
    ondownload,
    oncancel,
    onremove,
  }: {
    row: ModelRow | PackRow;
    /** The licence text, once fetched. */
    license?: LicenseView | null;
    /** The licence is shown, awaiting acceptance. */
    expanded?: boolean;
    /** The Rust side refused the last action, by `UiError` kind. */
    error?: string | null;
    onexpand: () => void;
    oncollapse: () => void;
    onaccept: () => void;
    ondownload: () => void;
    oncancel: () => void;
    onremove: () => void;
  } = $props();

  const model = $derived("tier" in row ? row : null);
  const pack = $derived("contents" in row ? row : null);
  const download = $derived(row.download);
  const bytes = (value: number) => formatBytes(value, i18n.locale);
  const size = $derived(bytes(row.size_bytes));
  const progress = $derived(
    download.state === "downloading" || download.state === "cancelling"
      ? {
          done: download.done,
          total: download.total,
          text: t("models.of", { done: bytes(download.done), total: bytes(download.total) }),
          percent: formatPercent(download.total > 0 ? download.done / download.total : 0, i18n.locale),
        }
      : null,
  );
  /** UI_UX §2's CPU words, in the user's language; anything else as the registry has it. */
  const CPU: Record<string, string> = {
    fast: "models.cpu.fast",
    moderate: "models.cpu.moderate",
    "slower, higher quality": "models.cpu.slower",
    "not yet measured": "models.cpu.unmeasured",
  };
  const cpu = $derived(model === null ? "" : CPU[model.cpu_expectation] !== undefined ? t(CPU[model.cpu_expectation] ?? "") : model.cpu_expectation);
  const TIERS: Record<string, string> = { small: "models.tier.small", quality: "models.tier.quality" };
  /** What a pack the app knows holds, in the user's language; the registry's words otherwise. */
  const PACKS: Record<string, string> = { validation: "packs.contents.validation" };
  const licenseId = $derived(`oc-license-${row.id}`);
  /** The refusals the Rust side can answer a row's action with; anything else is an I/O error. */
  const ERRORS = new Set(["license_not_accepted", "model_busy", "models_unavailable", "not_offered", "unknown_model"]);
</script>

<div class="oc-model" class:oc-model--failed={download.state === "failed"} data-row={row.id}>
  <div class="oc-model__head">
    <span class="oc-model__name">{row.display_name}</span>
    {#if model?.is_default}
      <span class="oc-tag oc-tag--default">{t("models.tag.default")}</span>
    {:else if model?.tier === "experimental"}
      <span class="oc-tag oc-tag--experimental">{t("models.tag.experimental")}</span>
    {:else if model !== null && TIERS[model.tier] !== undefined}
      <span class="oc-model__tier">{t(TIERS[model.tier] ?? "")}</span>
    {/if}
    <span class="oc-model__spacer"></span>
    {#if progress !== null}
      <span class="oc-model__state">
        {download.state === "cancelling" ? t("models.cancelling") : t("models.progress", { of: progress.text, percent: progress.percent })}
      </span>
      <button class="oc-btn oc-btn--sm" disabled={download.state === "cancelling"} onclick={oncancel}>{t("models.cancel")}</button>
    {:else if download.state === "failed"}
      <span class="oc-model__state oc-model__state--err"><Icon name="xcircle" />{t(`models.failed.${download.kind}`)}</span>
      <button class="oc-btn oc-btn--sm" onclick={ondownload}><Icon name="refresh" />{t("models.retry")}</button>
    {:else if row.installed}
      <span class="oc-model__state oc-model__state--ok"><Icon name="ccheck" />{t("models.installed")}</span>
      <button class="oc-btn oc-btn--danger oc-btn--sm" onclick={onremove}>{t("models.delete")}</button>
    {:else if expanded}
      <span class="oc-model__state">{t("models.licenseShown")}</span>
    {:else}
      <span class="oc-model__state">{t("models.notInstalled")}</span>
      <button class="oc-btn oc-btn--sm" onclick={onexpand}><Icon name="download" />{t("models.download")}</button>
    {/if}
  </div>

  {#if progress !== null}
    <ProgressBar done={progress.done} total={progress.total} label={t("models.downloading", { name: row.display_name })} valuetext={progress.text} />
  {/if}

  {#if expanded && !row.installed && progress === null && download.state !== "failed"}
    <details class="oc-card oc-card--license" open>
      <summary>{t("models.licenseText", { license: row.license })}</summary>
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <div class="oc-license" id={licenseId} tabindex="0" aria-label={t("models.licenseLabel", { license: row.license })}>{license?.text ?? ""}</div>
    </details>
    <div class="oc-actions">
      <button class="oc-btn oc-btn--primary" aria-describedby={licenseId} onclick={row.license_accepted ? ondownload : onaccept}>
        <Icon name="download" />{row.license_accepted ? t("models.downloadSize", { size }) : t("models.accept", { size })}
      </button>
      <button class="oc-btn oc-btn--quiet" onclick={oncollapse}>{t("firstrun.notnow")}</button>
    </div>
  {/if}

  <div class="oc-model__facts">
    <span>{size}</span>
    {#if model !== null}
      <span>{t("models.ram", { size: bytes(model.ram_estimate_bytes) })}</span>
      <span>{t("models.cpu", { cpu })}</span>
    {/if}
    {#if pack !== null}<span>{PACKS[pack.id] !== undefined ? t(PACKS[pack.id] ?? "") : pack.contents}</span>{/if}
    <span>{row.license_accepted ? t("models.licenseAccepted", { license: row.license }) : row.license}</span>
    {#if row.license_path !== null}<span class="oc-mono">{row.license_path}</span>{/if}
  </div>
  {#if model?.warn != null}
    <p class="oc-model__warn">{t("models.warn")}</p>
  {/if}
  {#if error !== null}
    <p class="oc-model__warn" role="alert">{t(`models.error.${ERRORS.has(error) ? error : "io"}`)}</p>
  {/if}
</div>
