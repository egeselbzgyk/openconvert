/**
 * The app's current language, as reactive state: changing it re-renders every string at once,
 * without a restart (design, Settings › Language).
 */

import {
  detectLocale,
  renderWarning,
  translate,
  translatePlural,
  type Args,
  type Locale,
} from "./i18n";

function systemLocale(): Locale {
  const languages = typeof navigator === "undefined" ? [] : navigator.languages ?? [];
  return detectLocale(languages);
}

export const i18n = $state<{ choice: Locale | "system"; locale: Locale }>({
  choice: "system",
  locale: systemLocale(),
});

/** Follow the system, or pin a language. */
export function setLanguage(choice: Locale | "system"): void {
  i18n.choice = choice;
  i18n.locale = choice === "system" ? systemLocale() : choice;
  if (typeof document !== "undefined") document.documentElement.lang = i18n.locale;
}

/** A UI string in the current language. */
export function t(key: string, args?: Args): string {
  return translate(i18n.locale, key, args);
}

/** A warning in the current language; the code itself if no template exists (never English). */
export function tw(code: string, args?: Args): string {
  return renderWarning(i18n.locale, code, args) ?? code;
}

/** A UI string that depends on a count (`key.one` / `key.other`), in the current language. */
export function tn(key: string, count: number, args?: Args): string {
  return translatePlural(i18n.locale, key, count, args);
}
