/**
 * Words for the rows' states, from the current locale. Kept out of the components so the same
 * sentence is used for the visible text and for the row's accessible name.
 */

import { formatPercent } from "./i18n";
import { i18n, t, tn } from "./locale.svelte";
import { visibleSteps, type ProgressCount, type Row, type Step } from "./jobstate";

export function stepLabel(step: Step): string {
  return t(`stage.${step}`);
}

/** "132 of 214 pages · 62 %" — or without the unit when the engine counts something else. */
export function countText(progress: ProgressCount): string {
  const pct = formatPercent(progress.total > 0 ? progress.done / progress.total : 0, i18n.locale);
  const args = { done: progress.done, total: progress.total, pct };
  return progress.unit === "pages" ? t("progress.pages", args) : t("progress.count", args);
}

/** The progressbar's `aria-valuetext`: "132 of 214 pages". */
export function countValue(progress: ProgressCount): string {
  const args = { done: progress.done, total: progress.total };
  return progress.unit === "pages" ? t("progress.pagesValue", args) : t("progress.countValue", args);
}

/** "step 1 of 5". */
export function stepOf(row: Row): string {
  const steps = visibleSteps(row);
  const index = row.current === null ? 0 : steps.indexOf(row.current);
  return t("queue.stepOf", { i: index + 1, n: steps.length });
}

/** "12 images", "1 image" — a count with its noun, by the locale's plural rules. */
export function pluralCount(key: string, n: number): string {
  return tn(key, n);
}
