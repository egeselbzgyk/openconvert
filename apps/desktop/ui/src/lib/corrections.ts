/**
 * The editors' arithmetic (result.html §2–3): what the user changed, relative to the book its
 * report describes, as the patch "Fix and rebuild" sends — and, afterwards, what the rebuilt
 * book's report says was applied.
 *
 * A patch carries only differences. The Rust side keeps them with the book's earlier corrections
 * (`src-tauri/src/corrections.rs`), and the engine applies the lot to the book as `structure` left
 * it, so an unchanged field is never sent back as if the user had set it.
 */

import type { CorrectionPatch, MetadataPatch, TocPatch } from "./backend";
import type { Report, ReportWarning, TocEntry } from "./report";

/** The metadata editor's fields. */
export interface MetadataDraft {
  title: string;
  authors: string[];
  language: string;
}

export function metadataDraft(report: Report): MetadataDraft {
  return {
    title: report.document.title ?? "",
    authors: [...report.document.authors],
    language: report.document.language,
  };
}

const sameList = (a: readonly string[], b: readonly string[]) =>
  a.length === b.length && a.every((item, index) => item === b[index]);

/** What differs from the book; `null` when nothing does. A blank title is no title change. */
export function metadataPatch(report: Report, draft: MetadataDraft): MetadataPatch | null {
  const patch: MetadataPatch = {};
  const title = draft.title.trim();
  if (title !== "" && title !== (report.document.title ?? "")) patch.title = title;
  const authors = draft.authors.map((author) => author.trim()).filter((author) => author !== "");
  if (!sameList(authors, report.document.authors)) patch.authors = authors;
  if (draft.language !== report.document.language) patch.language = draft.language;
  return Object.keys(patch).length === 0 ? null : patch;
}

/** One heading as the TOC editor holds it. */
export type TocDraft = TocEntry;

export function tocDraft(report: Report): TocDraft[] {
  return report.document.toc.map((entry) => ({ ...entry }));
}

/** Whether a heading differs from the one the report lists under its id. */
export function headingChanged(report: Report, entry: TocDraft): boolean {
  const original = report.document.toc.find((heading) => heading.heading === entry.heading);
  if (original === undefined) return false;
  const title = entry.title.trim();
  return (title !== "" && title !== original.title) || entry.level !== original.level;
}

/** Every renamed or re-levelled heading, with only what changed about it. */
export function tocPatch(report: Report, draft: readonly TocDraft[]): TocPatch[] {
  const patches: TocPatch[] = [];
  for (const entry of draft) {
    const original = report.document.toc.find((heading) => heading.heading === entry.heading);
    if (original === undefined) continue;
    const patch: TocPatch = { heading: entry.heading };
    const title = entry.title.trim();
    if (title !== "" && title !== original.title) patch.title = title;
    if (entry.level !== original.level) patch.level = entry.level;
    if (patch.title !== undefined || patch.level !== undefined) patches.push(patch);
  }
  return patches;
}

export function patchOf(metadata: MetadataPatch | null, toc: TocPatch[]): CorrectionPatch {
  return metadata === null ? { toc } : { metadata, toc };
}

/** What the user's corrections did to a rebuilt book, from its decisions (method `user`). */
export interface Applied {
  title: boolean;
  /** How many authors the book now names, when the list was corrected. */
  authors: number | null;
  language: boolean;
  /** How many headings were renamed or re-levelled. */
  headings: number;
}

export function applied(report: Report): Applied | null {
  const user = report.decisions.filter((decision) => decision.method === "user");
  if (user.length === 0) return null;
  const authors = user.find((decision) => decision.kind === "metadata_authors");
  const headings = new Set(
    user
      .filter((decision) => decision.kind.startsWith("toc_"))
      .map((decision) => decision.subject ?? decision.chosen),
  );
  return {
    title: user.some((decision) => decision.kind === "metadata_title"),
    authors:
      authors === undefined
        ? null
        : authors.chosen.split("; ").filter((author) => author !== "").length,
    language: user.some((decision) => decision.kind === "metadata_language"),
    headings: headings.size,
  };
}

/** The warning that says saved corrections were refused, when there is one (ARCHITECTURE §4.6). */
export function refusedCorrections(report: Report): ReportWarning | null {
  return report.warnings.find((warning) => warning.code.startsWith("W_OVERRIDES_")) ?? null;
}
