<script lang="ts">
  // "Edit metadata" (result.html §2, MetadataEditor in components.md): title, authors as a list,
  // language. Only what the user changes is sent; "Fix and rebuild" keeps it with the book's other
  // corrections and rebuilds from cache (A12.4b).
  import { tick } from "svelte";

  import Icon from "../../components/Icon.svelte";
  import type { MetadataPatch } from "../../lib/backend";
  import { metadataDraft, metadataPatch } from "../../lib/corrections";
  import { i18n, t } from "../../lib/locale.svelte";
  import { focusOnMount } from "../../lib/motion";
  import type { Report } from "../../lib/report";

  let {
    report,
    file,
    onsave,
    oncancel,
  }: {
    report: Report;
    /** The EPUB's file name, for the heading. */
    file: string;
    onsave: (patch: MetadataPatch) => void;
    oncancel: () => void;
  } = $props();

  // The editor starts from the book as it is; later changes to `report` do not reset a draft.
  // svelte-ignore state_referenced_locally
  let draft = $state(metadataDraft(report));
  const patch = $derived(metadataPatch(report, draft));
  let authorList = $state<HTMLUListElement | null>(null);
  let addButton = $state<HTMLButtonElement | null>(null);

  /** The languages v1 has warnings in, plus whatever the book already says it is. */
  const LANGUAGES = ["en", "de", "tr"] as const;
  const options = $derived([...new Set([report.document.language, ...LANGUAGES])]);
  const userLanguage = $derived(
    report.decisions.some((decision) => decision.method === "user" && decision.kind === "metadata_language"),
  );

  function languageName(tag: string): string {
    if (tag === "und") return t("editor.undetermined");
    let name = tag;
    try {
      name = new Intl.DisplayNames([i18n.locale], { type: "language" }).of(tag) ?? tag;
    } catch {
      // A tag the platform cannot name is shown as itself.
    }
    return tag === report.document.language && !userLanguage
      ? t("editor.detected", { language: name, tag })
      : `${name} (${tag})`;
  }

  const inputs = () => [...(authorList?.querySelectorAll<HTMLInputElement>("input") ?? [])];

  async function addAuthor() {
    draft.authors.push("");
    await tick();
    inputs().at(-1)?.focus();
  }

  async function removeAuthor(index: number) {
    draft.authors.splice(index, 1);
    await tick();
    // Focus stays in the list: on the author now at that place, else the last, else "Add author".
    const remaining = inputs();
    (remaining[index] ?? remaining.at(-1) ?? addButton)?.focus();
  }

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (patch !== null) onsave(patch);
  }
</script>

<!-- A form so Enter in a field submits; it never submits anywhere (form-action 'none'). -->
<main class="oc-editor">
<form class="oc-editor__form" onsubmit={submit}>
  <h2 class="oc-settings__title" tabindex="-1" use:focusOnMount>{t("editor.metaTitle", { file })}</h2>
  <p class="oc-field__help oc-editor__help">{t("editor.metaHelp")}</p>
  <label class="oc-field">
    <span class="oc-field__label">{t("editor.title")}</span>
    <input class="oc-input" bind:value={draft.title} />
  </label>
  <div class="oc-field">
    <span class="oc-field__label" id="editor-authors">{t("editor.authors")}</span>
    <ul class="oc-authors" aria-labelledby="editor-authors" bind:this={authorList}>
      {#each draft.authors as _, index (index)}
        <li>
          <input class="oc-input" bind:value={draft.authors[index]} aria-label={t("editor.author", { n: index + 1 })} />
          <button
            type="button"
            class="oc-btn oc-btn--icon oc-btn--sm"
            aria-label={t("editor.removeAuthor", { n: index + 1 })}
            onclick={() => void removeAuthor(index)}
          ><Icon name="x" /></button>
        </li>
      {/each}
    </ul>
    <button type="button" class="oc-btn oc-btn--quiet oc-btn--sm oc-editor__add" bind:this={addButton} onclick={() => void addAuthor()}>
      <Icon name="plus" />{t("editor.addAuthor")}
    </button>
  </div>
  <div class="oc-field">
    <label class="oc-field__label" for="editor-language">{t("editor.language")}</label>
    <select id="editor-language" class="oc-select oc-editor__select" bind:value={draft.language}>
      {#each options as tag (tag)}
        <option value={tag}>{languageName(tag)}</option>
      {/each}
    </select>
  </div>
  <div class="oc-actions">
    <button type="submit" class="oc-btn oc-btn--primary" disabled={patch === null}>{t("action.fixRebuild")}</button>
    <button type="button" class="oc-btn oc-btn--quiet" onclick={oncancel}>{t("editor.cancel")}</button>
  </div>
</form>
</main>
