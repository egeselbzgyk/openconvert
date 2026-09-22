/**
 * The window's entry point until the Svelte app lands (Phase 12 P12.6): the startup handshake,
 * now run by the Rust side (`startup_status`), rendered as one line.
 */

import { invoke } from "@tauri-apps/api/core";

interface Hello {
  engine_version: string;
  ir_version: number;
  protocol: number;
  pdfium_version: string;
}

async function main(): Promise<void> {
  const target = document.querySelector("#engine");
  if (target === null) return;
  try {
    const hello = await invoke<Hello>("startup_status");
    target.textContent = `engine ${hello.engine_version} · ir ${hello.ir_version} · protocol ${hello.protocol} · pdfium ${hello.pdfium_version}`;
  } catch (error) {
    target.textContent = JSON.stringify(error);
  }
}

void main();
