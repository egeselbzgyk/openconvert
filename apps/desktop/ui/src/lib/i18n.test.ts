import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { fill, formatPercent, LOCALES, renderWarning, STRINGS, translate } from "./i18n";
import { TEMPLATES } from "./warnings";

/** The engine's warning-code registry: the list every locale must cover. */
function registeredCodes(): string[] {
  // Vitest runs from the UI package root.
  const path = resolve(process.cwd(), "../../../crates/oc-core/src/warnings/codes.rs");
  const text = readFileSync(path, "utf8");
  return [...text.matchAll(/code:\s*"(W_[A-Z0-9_]+)"/g)].map((match) => match[1] ?? "");
}

/** The `{slot}` names in a template, sorted. */
function slots(template: string): string[] {
  return [...template.matchAll(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g)].map((m) => m[1] ?? "").sort();
}

describe("i18n", () => {
  // 12.8 — a warning is a code and its arguments, rendered through each locale's template.
  it("warnings_are_localised_from_code_and_args", () => {
    const args = { rows: 3, columns: 4, reason: "merged cells" };
    const rendered = LOCALES.map((locale) => renderWarning(locale, "W_TABLE_AS_IMAGE", args));

    for (const [index, sentence] of rendered.entries()) {
      const locale = LOCALES[index] ?? "en";
      expect(sentence, locale).not.toBeNull();
      expect(sentence, locale).not.toMatch(/\{[a-z_]+\}/);
      expect(sentence, locale).toContain("3");
      expect(sentence, locale).toContain("4");
      expect(sentence, locale).toContain("merged cells");
      // Exactly the locale's own template, filled.
      expect(sentence).toBe(fill(TEMPLATES[locale].get("W_TABLE_AS_IMAGE") ?? "", args, locale));
    }
    // Three different sentences: German and Turkish are not English with numbers in.
    expect(new Set(rendered).size).toBe(3);

    // Numbers are the locale's (design decision 18): 1,842 · 1.842 · 1.842.
    const count = { count: 1842 };
    expect(renderWarning("en", "W_IMAGE_ONLY_PAGES", count)).toContain("1,842");
    expect(renderWarning("de", "W_IMAGE_ONLY_PAGES", count)).toContain("1.842");
    expect(renderWarning("tr", "W_IMAGE_ONLY_PAGES", count)).toContain("1.842");
    // An unknown code has no sentence rather than an English one.
    expect(renderWarning("de", "W_NOT_A_CODE", {})).toBeNull();
  });

  // 12.9 — CI gate: every warning code in every locale, and every UI key in every locale.
  it("every_warning_code_has_every_locale", () => {
    const codes = registeredCodes();
    expect(codes.length).toBeGreaterThan(0);
    for (const locale of LOCALES) {
      const table = TEMPLATES[locale];
      for (const code of codes) {
        expect(table.has(code), `${locale} has no template for ${code}`).toBe(true);
      }
      for (const code of table.keys()) {
        expect(codes, `${locale} has a template for unregistered ${code}`).toContain(code);
      }
      for (const code of codes) {
        expect(slots(table.get(code) ?? ""), `${locale} ${code} slots`).toEqual(
          slots(TEMPLATES.en.get(code) ?? ""),
        );
      }
    }

    const english = Object.keys(STRINGS.en).sort();
    expect(english.length).toBeGreaterThan(0);
    for (const locale of LOCALES) {
      expect(Object.keys(STRINGS[locale]).sort(), `${locale} keys`).toEqual(english);
      for (const key of english) {
        const text = STRINGS[locale][key] ?? "";
        expect(text.trim(), `${locale} ${key} is empty`).not.toBe("");
        expect(slots(text), `${locale} ${key} slots`).toEqual(slots(STRINGS.en[key] ?? ""));
      }
    }
    // The design's illustrative codes are not engine codes and must not linger as keys.
    expect(english.some((key) => key.startsWith("warn."))).toBe(false);
  });

  it("never falls back to English", () => {
    expect(translate("de", "no.such.key")).toBe("⟦no.such.key⟧");
    expect(translate("tr", "queue.cancel")).toBe("İptal");
  });

  it("formats percentages the way each locale writes them", () => {
    expect(formatPercent(0.62, "en")).toBe("62%");
    expect(formatPercent(0.62, "tr")).toBe("%62");
    expect(formatPercent(0.62, "de")).toMatch(/^62\s%$/);
  });

  it("leaves a slot with no argument visible", () => {
    expect(fill("{n} of {total}", { n: 3 }, "en")).toBe("3 of {total}");
  });
});
