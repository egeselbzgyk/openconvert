<script lang="ts">
  // The only blocking screens (firstrun.html §2): a converter this app cannot trust. No drop zone,
  // no half-working UI; both versions named, what is safe said, a next step given. Focus starts on
  // "Copy details" (components.md, BlockingError).
  import type { Blocking } from "../lib/jobs.svelte";
  import { t } from "../lib/locale.svelte";
  import { focusOnMount } from "../lib/motion";
  import Icon from "./Icon.svelte";

  let {
    blocking,
    appVersion,
    onquit,
  }: { blocking: Blocking; appVersion: string; onquit: () => void } = $props();

  const title = $derived(t(blocking.kind === "version" ? "startup.version" : "startup.protocol"));
  const unknown = $derived(t("startup.unknown"));

  const body = $derived.by(() => {
    switch (blocking.kind) {
      case "protocol":
        return t("startup.protocolBody", { app: blocking.app, engine: blocking.engine ?? unknown });
      case "ir":
        return t("startup.irBody", { app: blocking.app, engine: blocking.engine ?? unknown });
      case "version":
        return t("startup.versionBody", { app: blocking.app, engine: blocking.engine });
      case "missing":
        return t("startup.missingBody");
    }
  });
  const hint = $derived(
    blocking.kind === "version"
      ? t("startup.versionHint", { app: blocking.app })
      : t("startup.reinstall", { version: appVersion }),
  );
  const facts = $derived.by(() => {
    switch (blocking.kind) {
      case "protocol":
        return [
          t("startup.withProtocol", { version: appVersion, protocol: blocking.app }),
          t("startup.withProtocol", { version: unknown, protocol: blocking.engine ?? unknown }),
        ];
      case "ir":
        return [
          t("startup.withIr", { version: appVersion, ir: blocking.app }),
          t("startup.withIr", { version: unknown, ir: blocking.engine ?? unknown }),
        ];
      case "version":
        return [blocking.app, blocking.engine];
      case "missing":
        return [appVersion, blocking.detail];
    }
  });

  let copied = $state(false);
  async function copy() {
    const details = [title, body, `${t("startup.app")}: ${facts[0]}`, `${t("startup.converter")}: ${facts[1]}`];
    try {
      await navigator.clipboard.writeText(details.join("\n"));
      copied = true;
    } catch {
      copied = false;
    }
  }
</script>

<div class="oc-blocking" role="alertdialog" aria-modal="true" aria-labelledby="oc-blocking-title">
  <div class="oc-blocking__box">
    <span class="oc-tile oc-tile--lg oc-tile--err"><Icon name="xcircle" size="xl" /></span>
    <h1 class="oc-blocking__title" id="oc-blocking-title">{title}</h1>
    <p>{body}</p>
    <p class="oc-blocking__hint">{hint}</p>
    <dl class="oc-dialog__facts">
      <div class="oc-dialog__fact"><dt>{t("startup.app")}</dt><dd class="oc-mono">{facts[0]}</dd></div>
      <div class="oc-dialog__fact"><dt>{t("startup.converter")}</dt><dd class="oc-mono">{facts[1]}</dd></div>
    </dl>
    <div class="oc-actions">
      <button class="oc-btn oc-btn--primary" use:focusOnMount onclick={copy}>{copied ? t("command.copied") : t("startup.copyDetails")}</button>
      <button class="oc-btn oc-btn--quiet" onclick={onquit}>{t("startup.quit")}</button>
    </div>
    <span class="oc-sr-only" role="status">{copied ? t("command.copied") : ""}</span>
  </div>
</div>
