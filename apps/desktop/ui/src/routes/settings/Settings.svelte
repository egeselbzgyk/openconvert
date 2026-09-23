<script lang="ts">
  // Route `settings` (settings.html; `models` is its Models section, and `firstrun` is that section
  // opened at the default model — `setup`). Live: AI assistance and its provider, the document
  // preset, the two resource caps, the app language, the versions, the third-party notices, and
  // the model manager and packs, whose rows are exactly what the Rust side's manager reports. What
  // this build cannot do is drawn as the design draws it and says so in words — never a number or a
  // row it cannot back with real data.
  import ConsentDialog from "../../components/ConsentDialog.svelte";
  import CopyCommand from "../../components/CopyCommand.svelte";
  import Dialog from "../../components/Dialog.svelte";
  import ModelRow from "../../components/ModelRow.svelte";
  import NumberWithUnit from "../../components/NumberWithUnit.svelte";
  import RadioGroup from "../../components/RadioGroup.svelte";
  import Toggle from "../../components/Toggle.svelte";
  import type {
    Backend,
    CacheUsage,
    CatalogKind,
    Detected,
    EndpointCheck,
    Preset,
    ProbeResult,
    Provider,
    Settings,
    UiConfig,
    UiError,
  } from "../../lib/backend";
  import { formatBytes } from "../../lib/bytes";
  import { defaultModel, type Catalog } from "../../lib/catalog.svelte";
  import type { Hello } from "../../lib/events";
  import { detectLocale, LOCALES, type Locale } from "../../lib/i18n";
  import { i18n, setLanguage, t, tn } from "../../lib/locale.svelte";
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
    cacheUsage = null,
    onclearcache = null,
    models = null,
    packs = null,
    setup = false,
    onback = null,
    onsetup = null,
    providers = null,
    onsaved = () => undefined,
    consent = $bindable(null),
    onconsented = () => undefined,
  }: {
    settings: Settings;
    config: UiConfig;
    hello: Hello | null;
    section?: Section;
    onsave: (next: Settings) => void;
    onreport?: (() => void) | null;
    /** What the engine's cache holds, asked for when Advanced opens. */
    cacheUsage?: (() => Promise<CacheUsage>) | null;
    onclearcache?: (() => Promise<void>) | null;
    /** The model manager's rows (`null` before they load). */
    models?: Catalog<"models"> | null;
    packs?: Catalog<"packs"> | null;
    /** Route `firstrun`: the Models section, opened at the default model with its licence shown. */
    setup?: boolean;
    /** "Back to the queue", offered once the first-run download is done. */
    onback?: (() => void) | null;
    /** AI assistance was turned on with the built-in provider and no model installed: open the
        model download (settings.html, "Turning it on opens the model download"). */
    onsetup?: (() => void) | null;
    /** Settings › Provider asks the engine through these (`openconvert provider …`). */
    providers?: Pick<Backend, "providerDetect" | "providerCheck" | "providerProbe" | "pickKeyFile" | "clearKeyFile" | "grantConsent"> | null;
    /** Settings the Rust side already saved (a key file picked, a consent granted). */
    onsaved?: (next: Settings) => void;
    /** The consent dialog, open for this endpoint check; set by the app when a job needed consent. */
    consent?: EndpointCheck | null;
    /** Allow was pressed in the dialog, and the consent is saved. */
    onconsented?: () => void;
  } = $props();

  /** Rows whose licence is shown, awaiting acceptance, by "kind:id". */
  let expanded = $state<Record<string, boolean>>({});
  const key = (kind: CatalogKind, id: string) => `${kind}:${id}`;
  const firstModel = $derived(defaultModel(models));

  function catalog(kind: CatalogKind): Catalog<"models"> | Catalog<"packs"> | null {
    return kind === "models" ? models : packs;
  }
  function expand(kind: CatalogKind, id: string) {
    expanded = { ...expanded, [key(kind, id)]: true };
    void catalog(kind)?.showLicense(id);
  }
  function collapse(kind: CatalogKind, id: string) {
    expanded = { ...expanded, [key(kind, id)]: false };
  }

  // The first-run route opens with the default model's licence shown (firstrun.html step 1).
  let opened = false;
  $effect(() => {
    if (!setup || opened || firstModel === undefined) return;
    opened = true;
    if (!firstModel.installed && firstModel.download.state === "idle") expand("models", firstModel.id);
  });

  const SECTIONS: Section[] = ["ai", "models", "provider", "presets", "advanced", "packs", "language", "network", "about"];
  const PRESETS: Preset[] = ["auto", "novel", "academic", "textbook", "poetry", "scanned"];
  /** Bytes per GB as the Advanced field counts them (a unit, not a tunable). */
  const GIB = 1024 * 1024 * 1024;
  /** Bytes per MB as the cache size is written (a unit, not a tunable). */
  const MB = 1024 * 1024;

  let licenses = $state(false);
  const aiHelp = `oc-ai-help`;
  const systemLanguage = detectLocale(typeof navigator === "undefined" ? [] : (navigator.languages ?? []));
  /** Each language named in itself (design, Settings › Language). */
  const OWN_NAME: Record<Locale, string> = { en: "English", de: "Deutsch", tr: "Türkçe" };
  const command = $derived(tesseractCommand(config.os));

  let usage = $state<CacheUsage | null>(null);
  let clearing = $state(false);
  $effect(() => {
    if (section === "advanced" && cacheUsage !== null) void cacheUsage().then((found) => (usage = found));
  });
  const cacheSize = $derived(
    usage === null
      ? ""
      : new Intl.NumberFormat(i18n.locale, { style: "unit", unit: "megabyte", maximumFractionDigits: 1 }).format(
          usage.bytes / MB,
        ),
  );

  async function clearCache() {
    clearing = false;
    if (onclearcache === null) return;
    await onclearcache();
    if (cacheUsage !== null) usage = await cacheUsage();
  }

  function save(change: Partial<Settings>) {
    onsave({ ...settings, ...change });
  }

  /** The switch (UI_UX §2.4): saved at once; on, with the built-in provider and no model on this
      computer, it opens the download of the default one. */
  function toggleAi(on: boolean) {
    save({ aiEnabled: on });
    if (on && settings.provider === "builtin" && firstModel !== undefined && !firstModel.installed) onsetup?.();
  }
  // Settings › Provider (settings.html; UI_UX §2.4). Built-in and Ollama are chosen at once; a custom
  // endpoint only through "Use this endpoint…", which asks the engine whether the host is this
  // computer and, when it is not, shows the consent dialog that names it (D10).
  let customChosen = $state(false);
  const providerChoice = $derived<Provider>(customChosen ? "custom" : settings.provider);
  let detected = $state<Detected | null>(null);
  $effect(() => {
    if (section !== "provider" || providers === null || detected !== null) return;
    providers
      .providerDetect()
      .then((found) => (detected = found))
      .catch(() => (detected = { ollama: null }));
  });
  /** What the engine said about the custom endpoint when "Use this endpoint…" was pressed. */
  let checked = $state<EndpointCheck | null>(null);
  let endpointError = $state<string | null>(null);
  let probe = $state<{ state: "testing" } | { state: "done"; result: ProbeResult } | null>(null);
  const consented = $derived(
    settings.custom.consent !== null && checked !== null && settings.custom.consent.host === checked.host,
  );

  function chooseProvider(value: string) {
    probe = null;
    if (value === "custom") {
      customChosen = true;
      return;
    }
    customChosen = false;
    save({ provider: value as Provider });
  }

  async function useEndpoint() {
    endpointError = null;
    probe = null;
    if (providers === null) return;
    let answer: EndpointCheck;
    try {
      answer = await providers.providerCheck(settings.custom.endpoint);
    } catch {
      endpointError = t("provider.notUrl");
      return;
    }
    checked = answer;
    if (!answer.usable) {
      endpointError = t("ai.why.plain_http");
      return;
    }
    if (answer.requires_consent && settings.custom.consent?.host !== answer.host) {
      consent = answer;
      return;
    }
    customChosen = false;
    save({ provider: "custom" });
  }

  async function allow() {
    const asked = consent;
    consent = null;
    if (providers === null || asked === null) return;
    const granted = await providers.grantConsent();
    onsaved(granted);
    customChosen = false;
    onsave({ ...granted, provider: "custom" });
    onconsented();
  }

  async function testConnection() {
    if (providers === null) return;
    probe = { state: "testing" };
    try {
      probe = { state: "done", result: await providers.providerProbe() };
    } catch (error) {
      probe = null;
      if ((error as UiError).kind === "consent_required") void useEndpoint();
      else endpointError = t("provider.notUrl");
    }
  }

  async function pickKey() {
    if (providers !== null) onsaved(await providers.pickKeyFile());
  }
  async function clearKey() {
    if (providers !== null) onsaved(await providers.clearKeyFile());
  }

  const aiState = $derived(
    settings.aiEnabled ? t("settings.ai.on", { provider: t(`provider.name.${settings.provider}`) }) : t("settings.ai.off"),
  );
</script>

<main class="oc-settings">
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
    <h2 class="oc-settings__title">{setup && section === "models" ? t("firstrun.title") : t(`settings.nav.${section}`)}</h2>

    {#if section === "ai"}
      <div class="oc-card">
        <div class="oc-card__title">{t("settings.ai.cardTitle")}</div>
        <p>{t("settings.ai.cardBody")}</p>
        <p class="oc-settings__hint">{t("settings.ai.cardNote")}</p>
      </div>
      {@render aiSwitch()}
      {#if config.aiTasksEnabled === 0}
        <p class="oc-settings__hint">{t("settings.ai.noTasks")}</p>
      {/if}
    {:else if section === "models"}
      {#if setup && firstModel !== undefined}
        <p class="oc-settings__hint">
          {t("firstrun.hint", {
            name: firstModel.display_name,
            license: firstModel.license,
            size: formatBytes(firstModel.size_bytes, i18n.locale),
            ram: formatBytes(firstModel.ram_estimate_bytes, i18n.locale),
          })}
        </p>
      {:else}
        <p class="oc-settings__hint">{t("settings.models.hint")}</p>
      {/if}
      {#if models !== null && models.unavailable !== null}
        <div class="oc-empty"><span class="oc-empty__title">{t("settings.models.none")}</span><span>{t("settings.models.noneLine")}</span></div>
      {:else if models !== null && models.rows !== null}
        {#each models.rows.filter((row) => !setup || row.is_default) as row (row.id)}
          {@render downloadable("models", row)}
        {/each}
        {#if setup && firstModel?.installed}
          {@render aiSwitch()}
          {#if onback !== null}
            <div class="oc-actions"><button class="oc-btn oc-btn--primary" onclick={onback}>{t("firstrun.back")}</button></div>
          {/if}
        {/if}
      {/if}
    {:else if section === "provider"}
      <div class="oc-setting oc-setting--stack">
        <RadioGroup
          label={t("settings.nav.provider")}
          value={providerChoice}
          onchange={chooseProvider}
          options={[
            { value: "builtin", label: t("settings.provider.builtin"), hint: t("settings.provider.builtinHint") },
            {
              value: "ollama",
              label: `${t("settings.provider.ollama")} · ${detected?.ollama ? t("provider.detected") : t("provider.notDetected")}`,
              hint: t("settings.provider.ollamaHint"),
            },
            { value: "custom", label: t("settings.provider.custom"), hint: t("settings.provider.customHint") },
          ]}
        />
        {#if providerChoice === "ollama"}
          <div class="oc-field">
            <label class="oc-field__label" for="oc-ollama-model">{t("provider.ollamaModel")}</label>
            <select
              id="oc-ollama-model"
              class="oc-select"
              disabled={!detected?.ollama}
              value={settings.ollamaModel ?? ""}
              onchange={(event) => {
                const model = (event.currentTarget as HTMLSelectElement).value;
                save({ ollamaModel: model === "" ? null : model });
              }}
            >
              <option value="">{t("provider.ollamaAny")}</option>
              {#each detected?.ollama?.models ?? [] as model (model)}<option value={model}>{model}</option>{/each}
            </select>
            <span class="oc-field__help">{detected?.ollama ? t("provider.ollamaModelHelp") : t("provider.ollamaNone")}</span>
          </div>
        {/if}
      </div>
      {#if providerChoice === "custom"}
        <div class="oc-setting oc-setting--stack">
          <div class="oc-setting__label">{t("provider.customTitle")}</div>
          <label class="oc-field">
            <span class="oc-field__label">{t("provider.baseUrl")}</span>
            <input
              class="oc-input oc-input--mono"
              value={settings.custom.endpoint}
              aria-invalid={endpointError !== null ? "true" : undefined}
              onchange={(event) => {
                checked = null;
                endpointError = null;
                save({ custom: { ...settings.custom, endpoint: (event.currentTarget as HTMLInputElement).value.trim() } });
              }}
            />
          </label>
          <label class="oc-field">
            <span class="oc-field__label">{t("provider.modelName")}</span>
            <input
              class="oc-input oc-input--mono"
              value={settings.custom.model}
              onchange={(event) => save({ custom: { ...settings.custom, model: (event.currentTarget as HTMLInputElement).value.trim() } })}
            />
          </label>
          <div class="oc-field">
            <span class="oc-field__label" id="oc-key-label">{t("provider.keyFile")}</span>
            <div class="oc-filepick">
              <span class="oc-filepick__value" class:is-set={settings.custom.apiKeyFile !== null} aria-labelledby="oc-key-label">{settings.custom.apiKeyFile ?? t("provider.keyFileNone")}</span>
              <button class="oc-btn oc-btn--sm" onclick={() => void pickKey()}>{t("provider.keyFileChange")}</button>
              {#if settings.custom.apiKeyFile !== null}<button class="oc-btn oc-btn--quiet oc-btn--sm" onclick={() => void clearKey()}>{t("provider.keyFileRemove")}</button>{/if}
            </div>
            <span class="oc-field__help">{t("provider.keyFileHelp")}</span>
          </div>
          <div class="oc-actions">
            <button class="oc-btn oc-btn--primary oc-btn--sm" disabled={settings.custom.endpoint === ""} onclick={() => void useEndpoint()}>{t("provider.use")}</button>
            {#if endpointError !== null}
              <span class="oc-field__error" role="alert">{endpointError}</span>
            {:else if checked !== null && !checked.loopback && consented}
              <span class="oc-field__help">{t("provider.consented", { host: checked.host })}</span>
            {:else if checked !== null && checked.loopback}
              <span class="oc-field__help">{t("provider.onComputer", { host: checked.host })}</span>
            {/if}
          </div>
        </div>
      {/if}
      {#if settings.provider !== "builtin" && !customChosen}
        <div class="oc-actions">
          <button class="oc-btn oc-btn--sm" disabled={probe?.state === "testing"} onclick={() => void testConnection()}>{t("provider.test")}</button>
          <span class="oc-field__help" role="status">
            {#if probe?.state === "testing"}{t("provider.testing")}{:else if probe?.state === "done" && probe.result.available}{t("provider.testOk", {
                kind: t(`provider.kind.${probe.result.provider}`),
                model: probe.result.model,
              })}{:else if probe?.state === "done" && !probe.result.available}{t("provider.testFailed", { reason: probe.result.reason })}{/if}
          </span>
        </div>
      {/if}
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
      {#if cacheUsage !== null}
        <div class="oc-setting">
          <div class="oc-setting__text">
            <div class="oc-setting__label">{t("settings.advanced.cache")}</div>
            <div class="oc-setting__help">
              {#if usage !== null && usage.books > 0}{tn("settings.advanced.cacheHelp", usage.books, { size: cacheSize })}{:else}{t("settings.advanced.cacheEmpty")}{/if}
            </div>
          </div>
          <button class="oc-btn oc-btn--danger oc-btn--sm" disabled={usage === null || usage.bytes === 0} onclick={() => (clearing = true)}>{t("settings.advanced.clear")}</button>
        </div>
      {/if}
    {:else if section === "packs"}
      <div class="oc-model oc-model--unavailable">
        <div class="oc-model__head">
          <span class="oc-model__name">{t("settings.packs.ocr")}</span><span class="oc-model__spacer"></span>
          <span class="oc-model__state">{t("settings.packs.notAvailable")}</span>
        </div>
        {#if command !== null}<CopyCommand {command} revertMs={config.copiedRevertMs} />{/if}
        <div class="oc-model__facts"><span class="oc-model__note">{t("settings.packs.ocrNote")}</span></div>
      </div>
      {#if packs !== null && packs.unavailable === null && packs.rows !== null}
        {#each packs.rows as row (row.id)}
          {@render downloadable("packs", row)}
        {/each}
      {:else}
        <!-- The pack registry this build ships pins no pack (packs.toml): nothing to download. -->
        <div class="oc-model oc-model--unavailable">
          <div class="oc-model__head">
            <span class="oc-model__name">{t("settings.packs.validation")}</span><span class="oc-model__tier">EPUBCheck</span>
            <span class="oc-model__spacer"></span>
            <span class="oc-model__state">{t("settings.packs.notAvailable")}</span>
          </div>
          <div class="oc-model__facts"><span class="oc-model__note">{t("settings.packs.validationNote")}</span></div>
        </div>
      {/if}
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
</main>

{#snippet aiSwitch()}
  <div class="oc-setting">
    <div class="oc-setting__text">
      <div class="oc-setting__label">{t("settings.ai.label")}</div>
      <div class="oc-setting__help" id={aiHelp}>{aiState}</div>
    </div>
    <Toggle checked={settings.aiEnabled} label={t("settings.ai.label")} describedby={aiHelp} onchange={toggleAi} />
  </div>
{/snippet}

{#snippet downloadable(kind: CatalogKind, row: NonNullable<Catalog<"models">["rows"]>[number] | NonNullable<Catalog<"packs">["rows"]>[number])}
  {@const owner = catalog(kind)}
  <ModelRow
    {row}
    license={owner?.licenses[row.id] ?? null}
    expanded={expanded[key(kind, row.id)] ?? false}
    error={owner?.errors[row.id] ?? null}
    onexpand={() => expand(kind, row.id)}
    oncollapse={() => collapse(kind, row.id)}
    onaccept={() => {
      collapse(kind, row.id);
      void owner?.acceptAndDownload(row.id);
    }}
    ondownload={() => {
      collapse(kind, row.id);
      void owner?.download(row.id);
    }}
    oncancel={() => void owner?.cancel(row.id)}
    onremove={() => void owner?.remove(row.id)}
  />
{/snippet}

{#if consent !== null}
  <ConsentDialog
    host={consent.host}
    url={consent.url}
    model={settings.custom.model}
    keyFile={settings.custom.apiKeyFile}
    onallow={() => void allow()}
    oncancel={() => (consent = null)}
  />
{/if}
{#if clearing && usage !== null}
  <!-- Clearing asks once (design decision 14): it deletes text, which cannot be undone. -->
  <Dialog
    title={t("settings.cache.title")}
    confirm={t("settings.cache.confirm")}
    danger
    oncancel={() => (clearing = false)}
    onconfirm={() => void clearCache()}
  >
    <p>{tn("settings.cache.body", usage.books, { size: cacheSize })}</p>
    <p class="oc-dialog__small">{t("settings.cache.small")}</p>
  </Dialog>
{/if}
{#if licenses}
  <Dialog title={t("settings.about.licensesTitle")} confirm={t("dialog.close")} cancel={null} oncancel={() => (licenses = false)} onconfirm={() => (licenses = false)} wide>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="oc-license" tabindex="0" aria-label={t("settings.about.licensesTitle")}>{notices}</div>
  </Dialog>
{/if}
