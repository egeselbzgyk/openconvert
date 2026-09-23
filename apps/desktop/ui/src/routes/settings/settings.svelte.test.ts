import { flushSync, mount, unmount } from "svelte";
import { afterEach, describe, expect, it } from "vitest";

import App from "../../App.svelte";
import { setLanguage } from "../../lib/locale.svelte";
import { CONFIG, FakeBackend, settle } from "../../test/fake-backend";

let app: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (app !== null) unmount(app);
  app = null;
  document.body.innerHTML = "";
  setLanguage("en");
});

const button = (text: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === text);
const nav = (text: string) => [...document.querySelectorAll<HTMLAnchorElement>(".oc-nav__item")].find((a) => a.textContent === text);

async function openSettings(backend = new FakeBackend()) {
  app = mount(App, { target: document.body, props: { backend, clock: () => 0 } });
  await settle();
  flushSync();
  button("Settings")?.click();
  flushSync();
  return backend;
}

describe("settings route", () => {
  it("the language applies at once, is saved, and names each language in itself", async () => {
    const backend = await openSettings();
    nav("Language")?.click();
    flushSync();
    const select = document.querySelector<HTMLSelectElement>('select[aria-labelledby="oc-language-label"]');
    expect([...(select?.options ?? [])].map((o) => o.textContent)).toEqual(["System (English)", "English", "Deutsch", "Türkçe"]);
    if (select !== null) {
      select.value = "de";
      select.dispatchEvent(new Event("change", { bubbles: true }));
    }
    flushSync();
    expect(document.querySelector(".oc-header")?.textContent).toContain("Einstellungen");
    expect(document.querySelector(".oc-app")?.getAttribute("lang")).toBe("de");
    expect(backend.saved.language).toBe("de");
  });

  it("presets and caps are saved; an invalid cap is refused with a message", async () => {
    const backend = await openSettings();
    nav("Presets")?.click();
    flushSync();
    const preset = document.querySelector<HTMLSelectElement>('select[aria-labelledby="oc-preset-label"]');
    if (preset !== null) {
      preset.value = "academic";
      preset.dispatchEvent(new Event("change", { bubbles: true }));
    }
    expect(backend.saved.preset).toBe("academic");

    nav("Advanced")?.click();
    flushSync();
    const pages = document.querySelector<HTMLInputElement>('input[aria-label="Max pages, pages"]');
    expect(pages?.value, "the shipped cap, from thresholds.toml").toBe(String(CONFIG.maxPages));
    if (pages !== null) {
      pages.value = "5000";
      pages.dispatchEvent(new Event("change", { bubbles: true }));
    }
    expect(backend.saved.maxPages).toBe(5000);
    if (pages !== null) {
      pages.value = "0";
      pages.dispatchEvent(new Event("change", { bubbles: true }));
    }
    flushSync();
    expect(pages?.getAttribute("aria-invalid")).toBe("true");
    expect(document.querySelector(".oc-field__error")?.textContent).toContain("at least 1");
    expect(backend.saved.maxPages, "an invalid entry is not saved").toBe(5000);
  });

  // PHASE 12 part B2: the switch is live — off by default (D17), saved at once, and with the
  // built-in provider and no model installed it opens the default model's download.
  it("the AI switch is off by default, is saved, and opens the model download when no model is installed", async () => {
    const backend = await openSettings();
    const ai = () => document.querySelector<HTMLButtonElement>('[role="switch"]');
    expect(ai()?.disabled).toBe(false);
    expect(ai()?.getAttribute("aria-checked")).toBe("false");
    expect(document.querySelector(".oc-setting__help")?.textContent).toContain("opens the model download");
    // No task has passed its evaluation in this build (`aiTasksEnabled: 0`): the page says the switch
    // changes no book, rather than promise it.
    expect(document.querySelector(".oc-settings__body")?.textContent).toContain("no decision has passed its evaluation");

    ai()?.click();
    await settle();
    flushSync();
    expect(backend.saved.aiEnabled).toBe(true);
    expect(document.querySelector(".oc-settings__title")?.textContent, "the default model's download").toBe("Set up AI assistance");

    // Installed: turning it on stays on the page and says who is asked.
    backend.models = { unavailable: null, rows: backend.models.rows.map((row) => ({ ...row, installed: row.is_default })) };
    backend.saved = { ...backend.saved, aiEnabled: false };
    unmount(app!);
    await openSettings(backend);
    ai()?.click();
    await settle();
    flushSync();
    expect(ai()?.getAttribute("aria-checked")).toBe("true");
    expect(document.querySelector(".oc-setting__help")?.textContent).toBe("On: the installed model is asked where the rules alone cannot decide.");
  });

  it("the card names all four tasks; the notices open in a dialog", async () => {
    await openSettings();
    expect(document.querySelector(".oc-card")?.textContent, "all four tasks are named").toContain("metadata");

    nav("About & updates")?.click();
    flushSync();
    expect(document.querySelector(".oc-settings__body")?.textContent).toContain("engine 0.1.0");
    button("Licenses")?.click();
    flushSync();
    expect(document.querySelector('[role="dialog"] .oc-license')?.textContent).toContain("Lucide");
    expect(document.activeElement?.textContent).toBe("Close");
  });

  it("the cache says what it holds, and clearing it asks once and deletes it", async () => {
    const backend = await openSettings();
    nav("Advanced")?.click();
    await settle();
    flushSync();
    const body = () => document.querySelector(".oc-settings__body")?.textContent ?? "";
    expect(body()).toContain("312 MB for 14 books. It holds text from your documents so rebuilds take seconds.");

    button("Clear cache…")?.click();
    flushSync();
    const dialog = document.querySelector('[role="dialog"]');
    expect(dialog?.textContent).toContain("Clear the cache?");
    expect(dialog?.textContent).toContain("(312 MB, 14 books)");
    expect(document.activeElement?.textContent, "opens on the safe button").toBe("Cancel");
    [...(dialog?.querySelectorAll("button") ?? [])].find((b) => b.textContent === "Clear cache")?.click();
    await settle();
    flushSync();
    expect(backend.calls).toContainEqual(["clearCache", null]);
    expect(body()).toContain("Empty.");
    expect(button("Clear cache…")?.disabled, "nothing left to clear").toBe(true);
  });

  // PHASE 11's hand-off, UI_UX §2.4: Ollama is found on this computer and lists its models; a custom
  // endpoint off this computer is used only after the consent dialog that names its host, and says
  // plainly that document text leaves the computer (D10).
  it("the provider screen: Ollama as detected, and a remote endpoint only through the consent dialog that names it", async () => {
    const backend = await openSettings();
    nav("Provider")?.click();
    await settle();
    flushSync();
    const radio = (text: string) =>
      [...document.querySelectorAll<HTMLElement>('[role="radio"]')].find((r) => r.textContent?.trim().startsWith(text));
    expect(radio("Ollama")?.textContent).toContain("Detected on this computer");
    radio("Ollama")?.click();
    flushSync();
    expect(backend.saved.provider).toBe("ollama");
    const models = document.querySelector<HTMLSelectElement>("#oc-ollama-model");
    expect([...(models?.options ?? [])].map((o) => o.value)).toEqual(["", "qwen3:1.7b", "llama3.2:3b"]);

    radio("Custom endpoint")?.click();
    flushSync();
    expect(backend.saved.provider, "not chosen until it is checked").toBe("ollama");
    const url = document.querySelector<HTMLInputElement>("input.oc-input--mono");
    if (url !== null) {
      url.value = "https://llm.example.org/v1";
      url.dispatchEvent(new Event("change", { bubbles: true }));
    }
    await settle();
    flushSync();
    button("Use this endpoint…")?.click();
    await settle();
    flushSync();
    const dialog = document.querySelector('[role="dialog"]');
    expect(dialog?.querySelector("h2")?.textContent).toBe("Send document text to llm.example.org?");
    expect(dialog?.textContent).toContain("text from the documents you convert will leave this computer and be sent to llm.example.org");
    expect(document.activeElement?.textContent, "opens on the safe button").toBe("Cancel");

    button("Cancel")?.click();
    flushSync();
    expect(backend.saved.custom.consent, "Cancel records nothing").toBeNull();
    expect(backend.saved.provider).toBe("ollama");

    button("Use this endpoint…")?.click();
    await settle();
    flushSync();
    button("Allow llm.example.org")?.click();
    await settle();
    flushSync();
    expect(backend.calls).toContainEqual(["grantConsent", "llm.example.org"]);
    expect(backend.saved.provider).toBe("custom");
    expect(backend.saved.custom.consent?.host).toBe("llm.example.org");
    expect(document.querySelector('[role="dialog"]')).toBeNull();

    // Test connection asks the engine, and says what it found in the user's language.
    button("Test connection")?.click();
    await settle();
    flushSync();
    expect(document.querySelector(".oc-settings__body")?.textContent).toContain(
      "No answer: the endpoint did not answer the capability probe",
    );
  });

  it("an endpoint on this computer needs no consent, and plain http off it is refused", async () => {
    const backend = await openSettings();
    nav("Provider")?.click();
    await settle();
    flushSync();
    [...document.querySelectorAll<HTMLElement>('[role="radio"]')].find((r) => r.textContent?.trim().startsWith("Custom"))?.click();
    flushSync();
    const url = () => document.querySelector<HTMLInputElement>("input.oc-input--mono");
    const set = async (value: string) => {
      const input = url();
      if (input === null) return;
      input.value = value;
      input.dispatchEvent(new Event("change", { bubbles: true }));
      await settle();
      flushSync();
    };
    await set("http://llm.example.org/v1");
    button("Use this endpoint…")?.click();
    await settle();
    flushSync();
    expect(document.querySelector(".oc-field__error")?.textContent).toBe(
      "The custom endpoint is not on this computer and does not use https.",
    );
    expect(document.querySelector('[role="dialog"]')).toBeNull();

    await set("http://127.0.0.1:1234/v1");
    button("Use this endpoint…")?.click();
    await settle();
    flushSync();
    expect(document.querySelector('[role="dialog"]'), "no dialog for this computer").toBeNull();
    expect(backend.saved.provider).toBe("custom");
    expect(document.querySelector(".oc-settings__body")?.textContent).toContain("127.0.0.1 is this computer, so nothing leaves it.");
  });

  // The Network log section is the design's (visible by default); what it lists is PHASE 14 detail
  // 12's audit log. Empty, the page says which connections would be recorded; with lines, they are
  // listed with their purpose in words.
  it("the network log says when nothing has connected, and lists what the audit log holds", async () => {
    const backend = await openSettings();
    nav("Network log")?.click();
    await settle();
    flushSync();
    const body = () => document.querySelector(".oc-settings__body")?.textContent ?? "";
    expect(body()).toContain("No connections yet");
    expect(body()).toContain("with AI assistance on — each time a conversion asks the provider you chose");
    expect(document.querySelector(".oc-table")).toBeNull();
    unmount(app!);
    app = null;

    backend.network = {
      state: "entries",
      entries: [{ ts: "2026-09-22T10:14:00Z", host: "huggingface.co", purpose: "download", bytes: 1_181_116_006, outcome: "ok" }],
    };
    await openSettings(backend);
    nav("Network log")?.click();
    await settle();
    flushSync();
    const cells = [...document.querySelectorAll(".oc-table td")].map((cell) => cell.textContent);
    expect(cells).toContain("huggingface.co");
    expect(cells).toContain("Model or pack download");
  });
});
