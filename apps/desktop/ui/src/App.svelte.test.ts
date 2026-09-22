import { flushSync, mount, unmount } from "svelte";
import { afterEach, describe, expect, it } from "vitest";

import App from "./App.svelte";
import { FakeBackend, settle } from "./test/fake-backend";

let app: ReturnType<typeof mount> | null = null;
afterEach(() => {
  if (app !== null) unmount(app);
  app = null;
  document.body.innerHTML = "";
});

describe("app", () => {
  // 12.3 — a hello with v: 2 shows a blocking error, not a degraded UI.
  it("protocol_version_mismatch_hard_errors", async () => {
    const backend = new FakeBackend();
    app = mount(App, { target: document.body, props: { backend, clock: () => 0 } });
    await settle();
    backend.change({ id: "job-1", input: "/b/a.pdf", output: "/b/a.epub", renamed: false, state: "running" });
    flushSync();
    expect(document.querySelector(".oc-queue")).not.toBeNull();

    backend.line("job-1", { v: 2, t: "hello", protocol: 2, engine_version: "9.9.9" });
    flushSync();

    const dialog = document.querySelector('[role="alertdialog"]');
    expect(dialog).not.toBeNull();
    expect(dialog?.textContent).toContain("can't start its converter");
    expect(document.querySelector(".oc-queue"), "no half-working queue behind it").toBeNull();
    expect(document.activeElement?.textContent, "focus starts on Copy details").toBe("Copy details");
  });

  it("a stale converter at start-up blocks the app with both versions named", async () => {
    const backend = new FakeBackend();
    backend.startupError = { kind: "stale_engine", detail: { engine: "0.0.9", app: "0.1.0" } };
    app = mount(App, { target: document.body, props: { backend, clock: () => 0 } });
    await settle();
    flushSync();
    const dialog = document.querySelector('[role="alertdialog"]');
    expect(dialog?.textContent).toContain("doesn't match this app");
    expect(dialog?.textContent).toContain("0.0.9");
    expect(dialog?.textContent).toContain("0.1.0");
  });
});
