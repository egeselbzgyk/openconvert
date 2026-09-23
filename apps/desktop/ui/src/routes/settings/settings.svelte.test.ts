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

  it("what this build cannot do is disabled and says so; the notices open in a dialog", async () => {
    await openSettings();
    const ai = document.querySelector<HTMLButtonElement>('[role="switch"]');
    expect(ai?.disabled).toBe(true);
    expect(ai?.getAttribute("aria-checked")).toBe("false");
    expect(document.querySelector(".oc-card")?.textContent, "all four tasks are named").toContain("metadata");

    nav("About & updates")?.click();
    flushSync();
    expect(document.querySelector(".oc-settings__body")?.textContent).toContain("engine 0.1.0");
    button("Licenses")?.click();
    flushSync();
    expect(document.querySelector('[role="dialog"] .oc-license')?.textContent).toContain("Lucide");
    expect(document.activeElement?.textContent).toBe("Close");
  });
});
