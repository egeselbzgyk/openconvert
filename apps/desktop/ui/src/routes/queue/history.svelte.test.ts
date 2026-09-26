import { flushSync, mount, unmount } from "svelte";
import { afterEach, describe, expect, it } from "vitest";

import App from "../../App.svelte";
import { setLanguage } from "../../lib/locale.svelte";
import { FakeBackend, historyEntries, settle } from "../../test/fake-backend";

let app: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (app !== null) unmount(app);
  app = null;
  document.body.innerHTML = "";
  setLanguage("en");
});

async function start(backend = new FakeBackend()) {
  app = mount(App, { target: document.body, props: { backend, clock: () => 0 } });
  await settle();
  flushSync();
  return backend;
}

function withHistory(): FakeBackend {
  const backend = new FakeBackend();
  backend.earlier = historyEntries();
  return backend;
}

const section = () => document.querySelector<HTMLElement>(".oc-history");
const toggle = () => section()?.querySelector<HTMLButtonElement>("button[aria-expanded]") ?? null;
const rows = () => [...document.querySelectorAll<HTMLElement>(".oc-history__list > li")];
const buttonIn = (root: Element | undefined, text: string) =>
  [...(root?.querySelectorAll<HTMLButtonElement>("button") ?? [])].find((b) => b.textContent?.trim() === text);
const status = () => document.querySelector('[role="status"][aria-live="polite"]')?.textContent ?? "";

describe("previous conversions", () => {
  it("earlier runs' books are listed in a section of their own, which opens and closes and remembers it", async () => {
    const backend = await start(withHistory());
    expect(toggle()?.textContent).toContain("Previous conversions");
    expect(toggle()?.textContent).toContain("2 books");
    expect(toggle()?.getAttribute("aria-expanded")).toBe("true");
    const list = document.getElementById(toggle()?.getAttribute("aria-controls") ?? "");
    expect(list?.getAttribute("aria-label")).toBe("Previous conversions, 2 books");
    expect(rows()).toHaveLength(2);

    const [done, failed] = rows();
    expect(done?.textContent).toContain("Moby-Dick; or, The Whale");
    expect(done?.textContent).toContain("Converted");
    expect(done?.textContent).toContain("214 pages");
    expect(done?.getAttribute("aria-label")).toBe("Moby-Dick; or, The Whale, Converted");
    expect(failed?.textContent).toContain("atlas.pdf");
    expect(failed?.textContent).toContain("Not converted");
    expect(failed?.textContent, "a stage that ran out of time says so").toContain("ran longer than the time limit");

    toggle()?.click();
    await settle();
    flushSync();
    expect(toggle()?.getAttribute("aria-expanded")).toBe("false");
    expect(list?.hidden, "collapsed").toBe(true);
    expect(backend.saved.historyOpen, "the choice is saved").toBe(false);

    // A restart opens it as it was left.
    unmount(app!);
    await start(backend);
    expect(toggle()?.getAttribute("aria-expanded")).toBe("false");
    toggle()?.click();
    await settle();
    flushSync();
    expect(backend.saved.historyOpen).toBe(true);
    expect(rows()).toHaveLength(2);
  });

  it("an earlier book opens in the reader and shows in its folder, by entry; one never written cannot be opened", async () => {
    const backend = await start(withHistory());
    const [done, failed] = rows();
    buttonIn(done, "Open in reader")?.click();
    buttonIn(done, "Show in folder")?.click();
    await settle();
    expect(backend.calls).toContainEqual(["historyOpen", "s1-job-2"]);
    expect(backend.calls).toContainEqual(["historyShow", "s1-job-2"]);
    expect(buttonIn(failed, "Open in reader"), "no book was written").toBeUndefined();

    // Moved away since: the row says so, and stops offering to open it.
    backend.earlierGone = true;
    buttonIn(rows()[0], "Open in reader")?.click();
    await settle();
    flushSync();
    expect(rows()[0]?.textContent).toContain("The book is no longer where it was saved.");
    expect(buttonIn(rows()[0], "Open in reader")).toBeUndefined();
  });

  it("a row is removed from the list by its button or Delete; the whole list is cleared after asking once", async () => {
    const backend = await start(withHistory());
    rows()[0]?.querySelector<HTMLButtonElement>('button[aria-label="Remove Moby-Dick; or, The Whale from this list"]')?.click();
    await settle();
    flushSync();
    expect(backend.calls).toContainEqual(["historyRemove", "s1-job-2"]);
    expect(rows()).toHaveLength(1);
    expect(status()).toContain("removed from the list");

    backend.earlier = historyEntries();
    unmount(app!);
    await start(backend);
    rows()[1]?.focus();
    rows()[1]?.dispatchEvent(new KeyboardEvent("keydown", { key: "Delete", bubbles: true }));
    await settle();
    flushSync();
    expect(backend.calls).toContainEqual(["historyRemove", "s1-job-1"]);

    buttonIn(section() ?? undefined, "Clear list…")?.click();
    flushSync();
    const dialog = document.querySelector('[role="dialog"]');
    expect(dialog?.textContent).toContain("Clear the list of 1 previous conversion?");
    expect(document.activeElement?.textContent, "opens on the safe button").toBe("Cancel");
    buttonIn(dialog ?? undefined, "Clear list")?.click();
    await settle();
    flushSync();
    expect(backend.calls).toContainEqual(["historyClear", null]);
    expect(section(), "nothing left to list").toBeNull();
  });

  it("arrows move between earlier books: one Tab stop into the list", async () => {
    await start(withHistory());
    expect(rows().map((row) => row.tabIndex)).toEqual([0, -1]);
    rows()[0]?.focus();
    rows()[0]?.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    flushSync();
    expect(document.activeElement).toBe(rows()[1]);
    expect(rows().map((row) => row.tabIndex)).toEqual([-1, 0]);
  });

  it("this session's conversions come first, under their own heading", async () => {
    const backend = await start(withHistory());
    expect(document.querySelector(".oc-queue__title")?.textContent, "no session rows yet").toBe("Previous conversions");
    backend.change({ id: "job-1", input: "/b/new.pdf", output: "/b/new.epub", renamed: false, unlocked: false, rebuild: false, state: "running" });
    flushSync();
    const titles = [...document.querySelectorAll(".oc-queue__title")].map((title) => title.textContent);
    expect(titles).toEqual(["This session", "Previous conversions"]);
  });

  it("without earlier books there is no such section", async () => {
    await start();
    expect(section()).toBeNull();
  });

  it("the header's folder button opens the OpenConvert folder", async () => {
    const backend = await start();
    const button = document.querySelector<HTMLButtonElement>('.oc-header button[aria-label="Open the OpenConvert folder"]');
    expect(button).not.toBeNull();
    expect(button?.querySelector("use")?.getAttribute("href")).toBe("#i-folder");
    button?.click();
    await settle();
    expect(backend.calls).toContainEqual(["openLibrary", null]);
    // Drop zone → queue → header: the folder button comes before Settings, both last in the DOM.
    const header = [...document.querySelectorAll(".oc-header button")].map((b) => b.getAttribute("aria-label") ?? b.textContent?.trim());
    expect(header).toEqual(["Open the OpenConvert folder", "Settings"]);
  });

  it("the drop zone says where the books will be saved", async () => {
    const backend = await start();
    expect(document.querySelector(".oc-dropzone")?.textContent).toContain("Each becomes an EPUB in the OpenConvert folder.");
    unmount(app!);
    backend.saved = { ...backend.saved, saveToLibrary: false };
    await start(backend);
    expect(document.querySelector(".oc-dropzone")?.textContent).toContain("Each becomes an EPUB next to the original.");
  });

  it("is in the user's language", async () => {
    const backend = withHistory();
    backend.saved = { ...backend.saved, language: "tr" };
    await start(backend);
    expect(toggle()?.textContent).toContain("Önceki dönüştürmeler");
    expect(rows()[0]?.textContent).toContain("214 sayfa");
    expect(document.querySelector('.oc-header button[aria-label="OpenConvert klasörünü aç"]')).not.toBeNull();
  });
});
