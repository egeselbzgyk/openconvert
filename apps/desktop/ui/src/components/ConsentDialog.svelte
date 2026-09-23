<script lang="ts">
  // The non-loopback consent dialog (settings.html, "Non-loopback consent"; UI_UX §2.4, D10). It
  // names the host, says plainly that document text will leave this computer, and lists what the
  // consent is to. Allow is the only way a consent is recorded — by the Rust side, for the saved
  // endpoint's own host — and Cancel records nothing.
  import { t } from "../lib/locale.svelte";
  import Dialog from "./Dialog.svelte";

  let {
    host,
    url,
    model,
    keyFile,
    onallow,
    oncancel,
  }: {
    host: string;
    url: string;
    model: string;
    keyFile: string | null;
    onallow: () => void;
    oncancel: () => void;
  } = $props();
</script>

<Dialog title={t("consent.title", { host })} confirm={t("consent.allow", { host })} {oncancel} onconfirm={onallow}>
  <p>{t("consent.body", { host })}</p>
  <dl class="oc-dialog__facts">
    <div class="oc-dialog__fact"><dt>{t("consent.host")}</dt><dd class="oc-mono">{host}</dd></div>
    <div class="oc-dialog__fact"><dt>{t("consent.baseUrl")}</dt><dd class="oc-mono">{url}</dd></div>
    {#if model !== ""}<div class="oc-dialog__fact"><dt>{t("consent.model")}</dt><dd class="oc-mono">{model}</dd></div>{/if}
    {#if keyFile !== null}<div class="oc-dialog__fact"><dt>{t("consent.keyFile")}</dt><dd class="oc-mono">{keyFile}</dd></div>{/if}
  </dl>
  <p class="oc-dialog__small">{t("consent.small")}</p>
</Dialog>
