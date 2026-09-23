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
});
