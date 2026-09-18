// What the specs share: where the unpacked containers are, and what the manifest says about them.
//
// The manifest is written by `cargo run -p xtask -- dom-fixtures`, which converts each fixture
// through the one pipeline the CLI drives and unpacks the container it produced. It holds the two
// things a browser cannot enumerate from inside one document: the spine, and an inventory of note
// references. The nav order is read in the browser by `order.spec.ts`, so that both sides of that
// comparison come from the parser a reader's software uses.

import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import path from "node:path";

/** `target/dom`, relative to this file. */
export const DOM_ROOT = path.resolve(import.meta.dirname, "../../../target/dom");

export interface NoteRef {
  from: string;
  to: string;
  fragment: string;
}

export interface Fixture {
  fixture: string;
  /** Content documents in spine order, as container paths. */
  spine: string[];
  noterefs: NoteRef[];
}

/**
 * Every fixture the DOM checks run over.
 *
 * Throws rather than skipping when the manifest is absent. A spec suite that quietly runs zero tests
 * is worse than one that fails: the job goes green and nobody learns that the fixtures were never
 * built (the same argument the `if: false` comment in ci.yml makes about a red job nobody trusts).
 */
export function fixtures(): Fixture[] {
  const manifest = path.join(DOM_ROOT, "manifest.json");
  let text: string;
  try {
    text = readFileSync(manifest, "utf8");
  } catch (cause) {
    throw new Error(
      `${manifest} is missing. Run \`cargo run -p xtask -- fixtures\` then ` +
        `\`cargo run -p xtask -- dom-fixtures\` before the DOM checks.`,
      { cause },
    );
  }
  const parsed = JSON.parse(text) as Fixture[];
  if (parsed.length === 0) {
    throw new Error(`${manifest} lists no fixtures`);
  }
  return parsed;
}

/** A `file://` URL for one document of one fixture. */
export function documentUrl(fixture: string, document: string): string {
  return pathToFileURL(path.join(DOM_ROOT, fixture, document)).href;
}
