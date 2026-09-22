/**
 * Strings, numbers and warnings in the user's language: EN, DE, TR (UI_UX §6).
 *
 * UI strings live in `locales/{en,de,tr}.json`, keyed as `docs/design/handoff/docs/strings.md`
 * keys them; warnings come from the engine's templates ({@link TEMPLATES}). Both are bundled,
 * never fetched (CSP `connect-src 'none'`).
 *
 * **There is no fallback to English.** A key missing from German renders as a visible marker, not
 * as the English sentence: A12.3 asks for "a localised sentence, never an English fallback", and
 * `every_warning_code_has_every_locale` makes a missing key a test failure long before a user
 * could see the marker.
 */

import deStrings from "../../locales/de.json";
import enStrings from "../../locales/en.json";
import trStrings from "../../locales/tr.json";
import { TEMPLATES } from "./warnings";

export const LOCALES = ["en", "de", "tr"] as const;
export type Locale = (typeof LOCALES)[number];

export type Args = Readonly<Record<string, unknown>>;

export const STRINGS: Readonly<Record<Locale, Readonly<Record<string, string>>>> = {
  en: enStrings,
  de: deStrings,
  tr: trStrings,
};

/** The first of the user's languages the app ships, by primary subtag; English otherwise. */
export function detectLocale(preferred: readonly string[]): Locale {
  for (const tag of preferred) {
    const primary = tag.split(/[-_]/)[0]?.toLowerCase();
    const found = LOCALES.find((locale) => locale === primary);
    if (found !== undefined) return found;
  }
  return "en";
}

/** A value as the locale writes it: numbers through `Intl.NumberFormat` (design decision 18). */
export function formatValue(value: unknown, locale: Locale): string {
  if (typeof value === "number") return new Intl.NumberFormat(locale).format(value);
  if (typeof value === "string") return value;
  if (Array.isArray(value)) return value.map((item) => formatValue(item, locale)).join(", ");
  if (value === null || value === undefined) return "";
  if (typeof value === "boolean") return String(value);
  return JSON.stringify(value);
}

/** A share as the locale writes a percentage: "62 %", "62 %", "%62". */
export function formatPercent(share: number, locale: Locale, fractionDigits = 0): string {
  return new Intl.NumberFormat(locale, {
    style: "percent",
    maximumFractionDigits: fractionDigits,
    minimumFractionDigits: fractionDigits,
  }).format(share);
}

/**
 * Fill `{name}` slots. A slot with no argument stays visible as `{name}`: a sentence missing its
 * number has stopped being a factual claim, and a brace is a bug report where a blank is a mystery
 * (the engine's `render` makes the same choice).
 */
export function fill(template: string, args: Args, locale: Locale): string {
  return template.replace(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g, (slot, name: string) =>
    Object.prototype.hasOwnProperty.call(args, name) ? formatValue(args[name], locale) : slot,
  );
}

/** A UI string. */
export function translate(locale: Locale, key: string, args: Args = {}): string {
  const template = STRINGS[locale][key];
  return template === undefined ? `⟦${key}⟧` : fill(template, args, locale);
}

/**
 * A UI string that depends on a count: `key.one`, `key.other`, … chosen by the locale's plural
 * rules, with the count as `{n}`. Every locale carries the same suffixes, so the key-parity gate
 * still holds.
 */
export function translatePlural(locale: Locale, key: string, count: number, args: Args = {}): string {
  const category = new Intl.PluralRules(locale).select(count);
  const table = STRINGS[locale];
  const template = table[`${key}.${category}`] ?? table[`${key}.other`];
  return template === undefined ? `⟦${key}⟧` : fill(template, { n: count, ...args }, locale);
}

/** A warning's sentence from its code and arguments, or `null` for a code no table knows. */
export function renderWarning(locale: Locale, code: string, args: Args = {}): string | null {
  const template = TEMPLATES[locale].get(code);
  return template === undefined ? null : fill(template, args, locale);
}
