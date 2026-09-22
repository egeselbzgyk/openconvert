// The UI's own repository rules, the ones no compiler checks (IMPLEMENTATION_PLAN Phase 12).
//
// - CSP: no `style="…"` attribute in markup and no inline <script>/<style> in the built page —
//   the shipped policy is `style-src 'self'` and `script-src 'self'`, so either would be refused
//   at runtime (Phase 12, "Visual design" 5). Dynamic values go through `style:` (the CSSOM).
// - Privacy: no fetch / XMLHttpRequest / WebSocket / EventSource / sendBeacon, and no http(s)
//   URL in the source (D13.9). The webview has no network permission; the source says so too.
// - Colours live in tokens.css only: no colour literal in oc.css or in a component.
// - Turkish casing comes from the string, never from `text-transform` (strings.md).
// - No positive tabindex anywhere (design decision 5).
// - Every literal `t("key")` names a key that exists in locales/en.json.

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const findings = [];

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    return entry.isDirectory() ? walk(path) : [path];
  });
}

function report(path, line, rule, text) {
  findings.push(`${relative(root, path)}:${line}: ${rule}\n    ${text.trim()}`);
}

const english = JSON.parse(readFileSync(join(root, "locales/en.json"), "utf8"));
const sources = walk(join(root, "src")).filter(
  (path) => /\.(svelte|ts|css)$/.test(path) && !path.endsWith(".test.ts"),
);

const RULES = [
  { test: /\.svelte$/, pattern: /\sstyle="/, rule: "inline style attribute (CSP style-src 'self'); use a class or style:" },
  { test: /\.(svelte|ts)$/, pattern: /\b(fetch|XMLHttpRequest|WebSocket|EventSource|sendBeacon)\s*\(/, rule: "network API in the webview (D13.9)" },
  { test: /\.(svelte|ts)$/, pattern: /https?:\/\//, rule: "a URL in the UI source (D13.9)" },
  { test: /\.(svelte|ts)$/, pattern: /tabindex=["{]?\s*[1-9]/, rule: "positive tabindex (design decision 5)" },
  { test: /\.(svelte|css)$/, pattern: /text-transform/, rule: "text-transform (Turkish casing comes from the string)" },
  { test: /(oc\.css|\.svelte)$/, pattern: /#[0-9a-fA-F]{3,8}\b|rgba?\(/, rule: "colour literal outside tokens.css" },
];

for (const path of sources) {
  const lines = readFileSync(path, "utf8").split("\n");
  lines.forEach((text, index) => {
    for (const { test, pattern, rule } of RULES) {
      if (test.test(path) && pattern.test(text)) report(path, index + 1, rule, text);
    }
    for (const match of text.matchAll(/\bt\(\s*"([^"]+)"/g)) {
      if (!(match[1] in english)) report(path, index + 1, `unknown string key "${match[1]}"`, text);
    }
  });
}

// The built page, when there is one: nothing inline.
const built = join(root, "dist/index.html");
if (existsSync(built)) {
  const html = readFileSync(built, "utf8");
  if (/<script(?![^>]*\bsrc=)[^>]*>/.test(html)) report(built, 1, "inline <script> in the built page", "");
  if (/<style[\s>]/.test(html) || /\sstyle="/.test(html)) report(built, 1, "inline style in the built page", "");
}

if (findings.length > 0) {
  console.error(findings.join("\n"));
  console.error(`\nui lint: ${findings.length} finding(s)`);
  process.exit(1);
}
console.log(`ui lint: clean (${sources.length} files)`);
