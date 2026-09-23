/**
 * Sizes as the locale writes them: "1.1 GB", "1,1 GB", "638 MB". Binary units, as Settings ›
 * Advanced counts the cache and the memory cap.
 */

import type { Locale } from "./i18n";

/** Bytes per KB, MB, GB (units, not tunables). */
const KIB = 1024;
const MIB = KIB * KIB;
const GIB = MIB * KIB;

export function formatBytes(bytes: number, locale: Locale): string {
  const [unit, divisor, digits] =
    bytes >= GIB
      ? (["gigabyte", GIB, 1] as const)
      : bytes >= MIB
        ? (["megabyte", MIB, 0] as const)
        : bytes >= KIB
          ? (["kilobyte", KIB, 0] as const)
          : (["byte", 1, 0] as const);
  return new Intl.NumberFormat(locale, { style: "unit", unit, maximumFractionDigits: digits }).format(bytes / divisor);
}
