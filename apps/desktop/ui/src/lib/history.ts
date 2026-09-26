/**
 * How an earlier run's book is described on the main page ("Previous conversions"): its name, why
 * it failed in the queue row's own sentences, when it finished and how long it took — every word
 * and number in the user's language.
 */

import type { HistoryEntry } from "./backend";
import type { Locale } from "./i18n";
import { DEADLINE_CAP } from "./jobstate";

/** What a row is called: the book's title, or its PDF's file name. */
export function entryName(entry: HistoryEntry): string {
  return entry.title ?? entry.inputName;
}

/** The sentence for why a conversion failed: the key the queue row uses for the same code. */
export function failureKey(entry: HistoryEntry): string {
  switch (entry.errorCode) {
    case "E_PDF":
      return "error.notPdf";
    case "E_PASSWORD_REQUIRED":
      return "error.password";
    case "E_LIMIT_EXCEEDED":
      return entry.errorCap === DEADLINE_CAP ? "error.deadline" : "error.limit";
    case "E_INPUT":
      return "error.unreadable";
    case "E_START":
      return "error.start";
    default:
      return "error.failed";
  }
}

/** Units, not tunables. */
const MS_PER_SECOND = 1000;
const SECONDS_PER_MINUTE = 60;
const SECONDS_PER_HOUR = 3600;

/** "1 min, 23 sec" / "1 Min., 23 Sek." / "1 dk, 23 sn": the two largest units that are not zero. */
export function durationText(ms: number, locale: Locale): string {
  const total = Math.round(ms / MS_PER_SECOND);
  const hours = Math.floor(total / SECONDS_PER_HOUR);
  const minutes = Math.floor((total % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE);
  const seconds = total % SECONDS_PER_MINUTE;
  const unit = (value: number, name: "hour" | "minute" | "second") =>
    new Intl.NumberFormat(locale, { style: "unit", unit: name, unitDisplay: "short" }).format(value);
  const parts =
    hours > 0
      ? [unit(hours, "hour"), ...(minutes > 0 ? [unit(minutes, "minute")] : [])]
      : minutes > 0
        ? [unit(minutes, "minute"), ...(seconds > 0 ? [unit(seconds, "second")] : [])]
        : [unit(seconds, "second")];
  return new Intl.ListFormat(locale, { type: "unit", style: "short" }).format(parts);
}

/** When it finished, as the locale writes a date and a time. */
export function whenText(iso: string, locale: Locale): string {
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(new Date(iso));
}
