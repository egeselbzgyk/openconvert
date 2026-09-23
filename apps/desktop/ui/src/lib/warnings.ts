/**
 * Warning templates: the engine's own tables, compiled into the bundle (R10 §6.20).
 *
 * The engine never sends prose: a `warning` event is a `code` and its `args`, and the sentence is
 * chosen here, in the user's language. The sentences are **the engine's templates** —
 * `crates/oc-core/src/warnings/templates_{en,de,tr}.toml`, the files the CLI renders from — read
 * at build time, so there is one text per code per locale in the whole repository and the
 * argument names cannot drift between the two front ends. `xtask ci-lint` holds those files and
 * the code registry in agreement; `every_warning_code_has_every_locale` holds this bundle to them.
 */

import en from "../../../../../crates/oc-core/src/warnings/templates_en.toml?raw";
import de from "../../../../../crates/oc-core/src/warnings/templates_de.toml?raw";
import tr from "../../../../../crates/oc-core/src/warnings/templates_tr.toml?raw";

import type { Locale } from "./i18n";

/**
 * Parse the one TOML shape the template files use: `CODE = "basic string"` lines, comments and
 * blank lines. Anything else is an error, not a skip — a template the parser silently dropped
 * would reach a user as a missing sentence.
 */
export function parseTemplates(text: string): Map<string, string> {
  const table = new Map<string, string>();
  text.split("\n").forEach((raw, index) => {
    const line = raw.trim();
    if (line === "" || line.startsWith("#")) return;
    const match = /^([A-Z][A-Z0-9_]*)\s*=\s*"((?:[^"\\]|\\.)*)"\s*(?:#.*)?$/.exec(line);
    if (match === null) {
      throw new Error(`templates line ${index + 1} is not \`CODE = "text"\`: ${line}`);
    }
    const [, code, body] = match as unknown as [string, string, string];
    table.set(code, unescape(body));
  });
  return table;
}

/** TOML basic-string escapes. */
function unescape(body: string): string {
  return body.replace(/\\(u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8}|.)/g, (_, escape: string) => {
    switch (escape[0]) {
      case "n":
        return "\n";
      case "t":
        return "\t";
      case '"':
        return '"';
      case "\\":
        return "\\";
      case "u":
      case "U":
        return String.fromCodePoint(Number.parseInt(escape.slice(1), 16));
      default:
        throw new Error(`unsupported escape \\${escape}`);
    }
  });
}

/** Every locale's table, keyed by warning code. */
export const TEMPLATES: Readonly<Record<Locale, Map<string, string>>> = {
  en: parseTemplates(en),
  de: parseTemplates(de),
  tr: parseTemplates(tr),
};
