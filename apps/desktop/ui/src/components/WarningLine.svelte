<script lang="ts">
  // One warning (components.md, WarningLine): a severity icon that is an image with a name, the
  // page link, and the sentence rendered from `code` + `args` through the locale's template —
  // never free text (R10 §6.20).
  import { t, tw } from "../lib/locale.svelte";
  import { pageNumber, type ReportWarning } from "../lib/report";
  import Icon from "./Icon.svelte";
  import PageLink from "./PageLink.svelte";

  let { warning, onpage = null }: { warning: ReportWarning; onpage?: ((page: string) => void) | null } = $props();

  const icon = $derived(warning.severity === "error" ? "xcircle" : warning.severity === "warn" ? "alert" : "info");
  const page = $derived(pageNumber(warning.page));
</script>

<li class="oc-warning oc-warning--{warning.severity}">
  <Icon name={icon} label={t(`severity.${warning.severity}`)} />
  {#if page !== null}<PageLink {page} {onpage} />{/if}
  <span class="oc-warning__text">{tw(warning.code, warning.args)}</span>
</li>
