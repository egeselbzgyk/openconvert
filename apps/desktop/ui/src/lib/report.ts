/**
 * `report.json`, as the result panel and the report view read it (PIPELINE §13).
 *
 * Only the fields the UI renders are typed; every one of them is the engine's own. The panel and
 * the view are renderings of this file — no number shown is computed anywhere else — so the
 * helpers here only select and pair what the file says.
 */

export interface PageRef {
  index: number;
  label: string | null;
}

export interface ReportWarning {
  code: string;
  severity: "info" | "warn" | "error";
  args: Record<string, string>;
  blocks: string[];
  page: PageRef | null;
}

export interface ReasonTotal {
  reason: string;
  removed_chars: number;
  added_chars: number;
  net_removed: number;
  measured: number;
}

export interface Decision {
  stage: string;
  kind: string;
  chosen: string;
  alternatives: string[];
  method: "deterministic" | "llm" | "user";
  llm: { model_id: string; prompt_version: string } | null;
}

export interface Report {
  schema: string;
  status: "ok" | "invalid";
  engine: { version: string; ir_version: number; pdfium_version: string; prompt_version: string | null };
  input: { sha256: string; filename: string; pages: number; producer_family: string };
  document: {
    classification: string;
    preset: string;
    language: string;
    title: string | null;
    sections: number;
    figures: number;
    tables: number;
    notes: number;
    images_extracted: number;
  };
  timings_ms: Array<[string, number]>;
  conservation: {
    c_raw_chars: number;
    c0_chars: number;
    epub_chars: number;
    retention: number;
    per_reason: ReasonTotal[];
  };
  page_classes: Record<string, number>;
  validation: {
    tier1: { valid: boolean; findings: Array<{ id: string; severity: string }> };
    tier2_ran: boolean;
    tier3_ran: boolean;
    structural: { retention: number; image_parity: boolean; note_bijection: boolean };
  };
  repair: { status: string; iterations: number };
  warnings: ReportWarning[];
  decisions: Decision[];
}

/** The three verdicts D6/D13.7 produce (UI_UX §2.3). */
export type Verdict = "passed" | "warn" | "invalid";

export function verdict(report: Report): Verdict {
  if (report.status === "invalid") return "invalid";
  return report.warnings.length > 0 ? "warn" : "passed";
}

/** The page a warning points at, as a reader counts it: the printed label, else the page number. */
export function pageNumber(page: PageRef | null): string | null {
  if (page === null) return null;
  return page.label ?? String(page.index + 1);
}

/** Notes linked, "a of b": all of them when the bijection holds, else what `W_NOTE_UNMATCHED` says. */
export function notesLinked(report: Report): { linked: number; of: number } {
  if (report.validation.structural.note_bijection) {
    return { linked: report.document.notes, of: report.document.notes };
  }
  const unmatched = report.warnings.find((warning) => warning.code === "W_NOTE_UNMATCHED");
  const matched = Number(unmatched?.args.matched ?? Number.NaN);
  const notes = Number(unmatched?.args.notes ?? report.document.notes);
  return { linked: Number.isFinite(matched) ? matched : 0, of: notes };
}

/** How many decisions a model made — shown only when AI was on for the job (UI_UX §2.3). */
export function llmDecisions(report: Report): number | null {
  const count = report.decisions.filter((decision) => decision.method === "llm").length;
  return report.engine.prompt_version === null && count === 0 ? null : count;
}
