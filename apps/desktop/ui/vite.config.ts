import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

// The app is served by Tauri from `dist/` under the CSP in `src-tauri/tauri.conf.json`. Nothing
// here may produce an inline script or style: Svelte's component CSS is emitted as a file, and
// dynamic values are written through the CSSOM with `style:` (IMPLEMENTATION_PLAN Phase 12,
// "Visual design" 5).
export default defineConfig({
  plugins: [svelte()],
  server: {
    port: 5173,
    strictPort: true,
    // The warning templates are the engine's own files (src/lib/warnings.ts).
    fs: { allow: [".", "../../../crates/oc-core/src/warnings"] },
  },
  build: {
    // WebKitGTK 2.44 is the Linux floor (UI_UX §7); Safari 15 is a conservative stand-in.
    target: ["es2021", "safari15", "chrome105"],
    // No module-preload polyfill: it is an inline script on some Vite versions, and every
    // webview Tauri 2 supports has native module preload.
    modulePreload: { polyfill: false },
  },
  resolve: process.env.VITEST ? { conditions: ["browser"] } : undefined,
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
  },
});
