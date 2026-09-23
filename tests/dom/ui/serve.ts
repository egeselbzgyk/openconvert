// The built UI (`apps/desktop/ui/dist`), served the way the webview is: every response carries the
// Content-Security-Policy the app ships (`tauri.conf.json`), so a style attribute, an inline script
// or a request the policy refuses fails here exactly as it would in the window (Phase 12 detail 5:
// "verify this under the shipped CSP, not only in a dev server").

import { createServer, type Server } from "node:http";
import { readFileSync, statSync } from "node:fs";
import path from "node:path";

const APP = path.resolve(import.meta.dirname, "../../../apps/desktop");
export const DIST = path.join(APP, "ui/dist");

/** The policy the app ships, read from its Tauri configuration. */
export function shippedCsp(): string {
  const config = JSON.parse(readFileSync(path.join(APP, "src-tauri/tauri.conf.json"), "utf8"));
  const csp = config?.app?.security?.csp;
  if (typeof csp !== "string" || csp === "") throw new Error("tauri.conf.json ships no CSP");
  return csp;
}

const TYPES: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
  ".json": "application/json",
  ".woff2": "font/woff2",
};

/** Serve `DIST` on a free loopback port. Throws, naming the fix, when the UI has not been built. */
export async function serveUi(): Promise<{ url: string; close: () => Promise<void> }> {
  try {
    statSync(path.join(DIST, "index.html"));
  } catch {
    throw new Error(`no built UI at ${DIST}; run \`npm run build\` in apps/desktop/ui first`);
  }
  const csp = shippedCsp();
  const server: Server = createServer((request, response) => {
    const url = new URL(request.url ?? "/", "http://localhost");
    const relative = url.pathname === "/" ? "index.html" : decodeURIComponent(url.pathname.slice(1));
    const file = path.resolve(DIST, relative);
    if (!file.startsWith(DIST + path.sep)) {
      response.writeHead(403).end();
      return;
    }
    let body: Buffer;
    try {
      body = readFileSync(file);
    } catch {
      response.writeHead(404, { "Content-Security-Policy": csp }).end();
      return;
    }
    response.writeHead(200, {
      "Content-Type": TYPES[path.extname(file)] ?? "application/octet-stream",
      "Content-Security-Policy": csp,
    });
    response.end(body);
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (address === null || typeof address === "string") throw new Error("no port");
  return {
    url: `http://127.0.0.1:${address.port}/`,
    close: () => new Promise<void>((resolve) => server.close(() => resolve())),
  };
}
