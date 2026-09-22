<script lang="ts">
  import { SPRITE } from "./lib/icons";
  import { i18n, t } from "./lib/locale.svelte";
</script>

<!-- The icon sprite, once, invisible; every icon is a <use> into it. -->
<div class="oc-sprite" aria-hidden="true">{@html SPRITE}</div>

<div class="oc-app" lang={i18n.locale}>
  <main class="oc-main"></main>
  <header class="oc-header">
    <span class="oc-header__title">
      <svg class="oc-logo" aria-hidden="true"><use href="#oc-logo-sm"></use></svg>{t("app.title")}
    </span>
  </header>
</div>
