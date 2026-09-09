/**
 * The hello-Tauri window (Phase 0 First Milestone item 5).
 *
 * It spawns the staged engine, reads the `hello` event, and shows what it found. A version
 * mismatch is shown as plainly as a success: an app running against a stale sidecar
 * produces failures much later that look like conversion bugs (A0.7, RT A5.7).
 */

import { describeEngine, handshake, type Spawner } from "./engine";

const APP_VERSION = "0.1.0";

/**
 * The real spawner, over Tauri's sidecar `Command`. Imported lazily so that opening this
 * module outside Tauri - in a test, or in `vite dev` - does not fail at import time.
 */
async function tauriSpawner(): Promise<Spawner> {
  const { Command } = await import("@tauri-apps/plugin-shell");
  return {
    async run(args: string[]) {
      const output = await Command.sidecar("bin/openconvert", args).execute();
      return { code: output.code, stdout: output.stdout, stderr: output.stderr };
    },
  };
}

async function main(): Promise<void> {
  const target = document.querySelector("#engine");
  if (target === null) return;
  try {
    const hello = await handshake(await tauriSpawner(), APP_VERSION);
    target.textContent = describeEngine(hello);
  } catch (error) {
    target.textContent = error instanceof Error ? error.message : String(error);
  }
}

void main();
