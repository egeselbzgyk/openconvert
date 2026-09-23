<script lang="ts">
  // Route `settings` (settings.html; `models` is its Models section). What part A of Phase 12 can
  // honour is live: the document preset, the two resource caps, the app language, the versions,
  // the third-party notices. What needs a model manager or a provider (Phase 9 / 11) is drawn as
  // the design draws it, disabled, and says in words that this build cannot do it yet — never a
  // number or a row it cannot back with real data.
  import CopyCommand from "../../components/CopyCommand.svelte";
  import Dialog from "../../components/Dialog.svelte";
  import NumberWithUnit from "../../components/NumberWithUnit.svelte";
  import RadioGroup from "../../components/RadioGroup.svelte";
  import Toggle from "../../components/Toggle.svelte";
  import type { Preset, Settings, UiConfig } from "../../lib/backend";
  import type { Hello } from "../../lib/events";
  import { detectLocale, LOCALES, type Locale } from "../../lib/i18n";
  import { setLanguage, t } from "../../lib/locale.svelte";
  import { tesseractCommand } from "../../lib/ocr";
  import notices from "../../../THIRD-PARTY-NOTICES.txt?raw";

  export type Section = "ai" | "models" | "provider" | "presets" | "advanced" | "packs" | "language" | "network" | "about";

  let {
    settings,
    config,
    hello,
    section = $bindable("ai"),
    onsave,
    onreport = null,
  }: {
    settings: Settings;
    config: UiConfig;
    hello: Hello | null;
    section?: Section;
    onsave: (next: Settings) => void;
    onreport?: (() => void) | null;
  } = $props();

  const SECTIONS: Section[] = ["ai", "models", "provider", "presets", "advanced", "packs", "language", "network", "about"];
  const PRESETS: Preset[] = ["auto", "novel", "academic", "textbook", "poetry", "scanned"];
  /** Bytes per GB as the Advanced field counts them (a unit, not a tunable). */
  const GIB = 1024 * 1024 * 1024;

  let licenses = $state(false);
  const aiHelp = `oc-ai-help`;
  const systemLanguage = detectLocale(typeof navigator === "undefined" ? [] : (navigator.languages ?? []));
  /** Each language named in itself (design, Settings › Language). */
  const OWN_NAME: Record<Locale, string> = { en: "English", de: "Deutsch", tr: "Türkçe" };
  const command = $derived(tesseractCommand(config.os));

  function save(change: Partial<Settings>) {
    onsave({ ...settings, ...change });
  }
</script>

<div class="oc-settings">
  <nav class="oc-nav" aria-label={t("settings.sections")}>
    {#each SECTIONS as item (item)}
      <a
        class="oc-nav__item"
        href="#{item}"
        aria-current={section === item ? "page" : undefined}
        onclick={(event) => {
          event.preventDefault();
          section = item;
        }}>{t(`settings.nav.${item}`)}</a
      >
    {/each}
  </nav>
  <div class="oc-settings__body">
    <h2 class="oc-settings__title">{t(`settings.nav.${section}`)}</h2>

    {#if section === "ai"}
      <div class="oc-card">
        <div class="oc-card__title">{t("settings.ai.cardTitle")}</div>
        <p>{t("settings.ai.cardBody")}</p>
        <p class="oc-settings__hint">{t("settings.ai.cardNote")}</p>
      </div>
      <div class="oc-setting">
        <div class="oc-setting__text">
          <div class="oc-setting__label">{t("settings.ai.label")}</div>
          <div class="oc-setting__help" id={aiHelp}>{t("settings.ai.unavailable")}</div>
        </div>
        <Toggle checked={false} label={t("settings.ai.label")} disabled describedby={aiHelp} />
      </div>
    {:else if section === "models"}
      <p class="oc-settings__hint">{t("settings.models.hint")}</p>
      <div class="oc-empty"><span class="oc-empty__title">{t("settings.models.none")}</span><span>{t("settings.models.noneLine")}</span></div>
    {:else if section === "provider"}
      <div class="oc-setting oc-setting--stack">
        <RadioGroup
          label={t("settings.nav.provider")}
          value="builtin"
          disabled
          options={[
            { value: "builtin", label: t("settings.provider.builtin"), hint: t("settings.provider.builtinHint") },
            { value: "ollama", label: t("settings.provider.ollama"), hint: t("settings.provider.ollamaHint") },
            { value: "custom", label: t("settings.provider.custom"), hint: t("settings.provider.customHint") },
          ]}
        />
        <span class="oc-field__help">{t("settings.provider.unavailable")}</span>
      </div>
    {:else if section === "presets"}
      <p class="oc-settings__hint">{t("settings.presets.hint")}</p>
      <div class="oc-setting">
        <div class="oc-setting__text">
          <div class="oc-setting__label" id="oc-preset-label">{t("settings.presets.type")}</div>
          <div class="oc-setting__help">{t("settings.presets.typeHelp")}</div>
        </div>
        <select
          class="oc-select"
          aria-labelledby="oc-preset-label"
          value={settings.preset}
          onchange={(event) => save({ preset: (event.currentTarget as HTMLSelectElement).value as Preset })}
        >
          {#each PRESETS as preset (preset)}
            <option value={preset}>{preset === "auto" ? t("settings.presets.auto") : t(`preset.${preset}`)}</option>
          {/each}
        </select>
      </div>
    {:else if section === "advanced"}
      <div class="oc-setting">
        <div class="oc-setting__text">
          <div class="oc-setting__label">{t("settings.advanced.maxPages")}</div>
          <div class="oc-setting__help">{t("settings.advanced.maxPagesHelp")}</div>
        </div>
        <NumberWithUnit
          label={t("settings.advanced.maxPages")}
          unit={t("settings.advanced.pages")}
          value={settings.maxPages ?? config.maxPages}
          min={1}
          help={t("settings.advanced.maxPagesHelp")}
          onchange={(value) => save({ maxPages: value === config.maxPages ? null : value })}
        />
      </div>
      <div class="oc-setting">
        <div class="oc-setting__text">
          <div class="oc-setting__label">{t("settings.advanced.maxMemory")}</div>
          <div class="oc-setting__help">{t("settings.advanced.maxMemoryHelp")}</div>
        </div>
        <NumberWithUnit
          label={t("settings.advanced.maxMemory")}
          unit={t("settings.advanced.gb")}
          value={Math.round((settings.maxMemoryBytes ?? config.maxMemoryBytes) / GIB)}
          min={1}
          help={t("settings.advanced.maxMemoryHelp")}
          onchange={(value) => save({ maxMemoryBytes: value * GIB === config.maxMemoryBytes ? null : value * GIB })}
        />
      </div>
    {:else if section === "packs"}
      <div class="oc-model oc-model--unavailable">
        <div class="oc-model__head">
          <span class="oc-model__name">{t("settings.packs.ocr")}</span><span class="oc-model__spacer"></span>
          <span class="oc-model__state">{t("settings.packs.notAvailable")}</span>
        </div>
        {#if command !== null}<CopyCommand {command} revertMs={config.copiedRevertMs} />{/if}
        <div class="oc-model__facts"><span class="oc-model__note">{t("settings.packs.ocrNote")}</span></div>
      </div>
      <div class="oc-model oc-model--unavailable">
        <div class="oc-model__head">
          <span class="oc-model__name">{t("settings.packs.validation")}</span><span class="oc-model__tier">EPUBCheck</span>
          <span class="oc-model__spacer"></span>
          <span class="oc-model__state">{t("settings.packs.notAvailable")}</span>
        </div>
        <div class="oc-model__facts"><span class="oc-model__note">{t("settings.packs.validationNote")}</span></div>
      </div>
    {:else if section === "language"}
      <div class="oc-setting">
        <div class="oc-setting__text">
          <div class="oc-setting__label" id="oc-language-label">{t("settings.language")}</div>
          <div class="oc-setting__help">{t("settings.language.help")}</div>
        </div>
        <select
          class="oc-select"
          aria-labelledby="oc-language-label"
          value={settings.language ?? "system"}
          onchange={(event) => {
            const choice = (event.currentTarget as HTMLSelectElement).value;
            const language = choice === "system" ? null : (choice as Locale);
            setLanguage(language ?? "system");
            save({ language });
          }}
        >
          <option value="system">{t("settings.language.system", { language: OWN_NAME[systemLanguage] })}</option>
          {#each LOCALES as locale (locale)}<option value={locale} lang={locale}>{OWN_NAME[locale]}</option>{/each}
        </select>
      </div>
      <p class="oc-settings__hint">{t("settings.language.note")}</p>
    {:else if section === "network"}
      <p class="oc-settings__hint">{t("settings.network.hint")}</p>
      <div class="oc-empty"><span class="oc-empty__title">{t("settings.network.none")}</span><span>{t("settings.network.noneLine")}</span></div>
    {:else}
      <div class="oc-setting">
        <div class="oc-setting__text">
          <div class="oc-setting__label">{t("settings.about.report")}</div>
          <div class="oc-setting__help">{t("settings.about.reportHelp")}</div>
        </div>
        <button class="oc-btn oc-btn--sm" disabled={onreport === null} onclick={() => onreport?.()}>{t("settings.about.reportAction")}</button>
      </div>
      <div class="oc-setting">
        <div class="oc-setting__text">
          <div class="oc-setting__label">{t("settings.about.versions")}</div>
          <div class="oc-setting__help oc-mono">
            {t("settings.about.versionsLine", {
              app: config.appVersion,
              engine: hello?.engine_version ?? t("startup.unknown"),
              protocol: hello?.protocol ?? t("startup.unknown"),
              ir: hello?.ir_version ?? t("startup.unknown"),
              pdfium: hello?.pdfium_version ?? t("startup.unknown"),
            })}
          </div>
        </div>
        <button class="oc-btn oc-btn--quiet oc-btn--sm" onclick={() => (licenses = true)}>{t("settings.about.licenses")}</button>
      </div>
      <div class="oc-setting">
        <div class="oc-setting__text">
          <div class="oc-setting__label">{t("settings.about.updates")}</div>
          <div class="oc-setting__help">{t("settings.about.updatesHelp")}</div>
        </div>
      </div>
    {/if}
  </div>
</div>

{#if licenses}
  <Dialog title={t("settings.about.licensesTitle")} confirm={t("dialog.close")} cancel={null} oncancel={() => (licenses = false)} onconfirm={() => (licenses = false)} wide>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="oc-license" tabindex="0" aria-label={t("settings.about.licensesTitle")}>{notices}</div>
  </Dialog>
{/if}
