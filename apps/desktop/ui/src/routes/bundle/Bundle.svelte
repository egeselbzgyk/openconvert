<script lang="ts">
  // The diagnostic bundle's review (result.html §4): what was written, where, and what is in it —
  // shown after the native save dialog and before the user decides whether to share it. Nothing is
  // sent (SECURITY §10).
  import type { Bundle } from "../../lib/backend";
  import { formatValue } from "../../lib/i18n";
  import { i18n, t } from "../../lib/locale.svelte";
  import { focusOnMount } from "../../lib/motion";

  let { bundle, onshow, ondone }: { bundle: Bundle; onshow: () => void; ondone: () => void } = $props();

  const base = (path: string) => path.split(/[\\/]/).pop() ?? path;
  const folder = (path: string) => path.slice(0, Math.max(path.length - base(path).length - 1, 0));
  /** Bytes as kilobytes, one decimal under ten (a unit conversion). */
  const kb = (bytes: number) => {
    const value = bytes / 1024;
    return t("size.kb", { n: formatValue(Number(value.toFixed(value < 10 ? 1 : 0)), i18n.locale) });
  };
</script>

<main class="oc-editor">
    <h2 class="oc-settings__title">{t("bundle.saved")}</h2>
    <div class="oc-output">
      <div class="oc-output__file">
        <span class="oc-output__name">{base(bundle.path)}</span>
        <span class="oc-output__path">{t("bundle.where", { folder: folder(bundle.path), size: kb(bundle.bytes) })}</span>
      </div>
      <button class="oc-btn" onclick={onshow}>{t("result.show")}</button>
    </div>
    <p class="oc-field__help oc-bundle__para">{t("bundle.notSent")}</p>
    <table class="oc-table">
      <thead><tr><th>{t("bundle.file")}</th><th>{t("bundle.holds")}</th><th class="is-num">{t("bundle.size")}</th></tr></thead>
      <tbody>
        {#each bundle.entries as entry (entry.name)}
          <tr><td class="oc-mono">{entry.name}</td><td>{t(`bundle.what.${entry.name}`)}</td><td class="is-num">{kb(entry.bytes)}</td></tr>
        {/each}
      </tbody>
    </table>
    <p class="oc-field__help oc-bundle__para">{t("bundle.excluded")}</p>
    <div class="oc-actions">
      <button class="oc-btn oc-btn--primary" use:focusOnMount onclick={ondone}>{t("bundle.done")}</button>
    </div>
</main>
