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

/** The report's step timings, in the user-facing steps (UI_UX §5 "Timing per stage"). */
export function stepTimings(report: Report): Array<{ label: string; ms: number }> {
  const steps: Array<{ label: string; ms: number }> = [];
  for (const [stage, ms] of report.timings_ms) {
    if (stage === "total") continue;
    const label = STAGE_LABEL[stage] ?? stage;
    const last = steps[steps.length - 1];
    if (last !== undefined && last.label === label) {
      last.ms += ms;
    } else {
      steps.push({ label, ms });
    }
  }
  return steps;
}

/** The total the report records, or the sum of its steps. */
export function totalMs(report: Report): number {
  const total = report.timings_ms.find(([stage]) => stage === "total");
  return total?.[1] ?? stepTimings(report).reduce((sum, step) => sum + step.ms, 0);
}

/**
 * Engine stages as the report times them → the string key of the user-facing step. The driver
 * times `epub`, `validate` and `repair` as one call, so the report cannot split them and the view
 * does not pretend to.
 */
const STAGE_LABEL: Readonly<Record<string, string>> = {
  inspect: "stage.analyzing",
  ingest: "stage.analyzing",
  text: "stage.extracting",
  furniture: "stage.extracting",
  layout: "stage.reconstructing",
  paragraphs: "stage.reconstructing",
  structure: "stage.reconstructing",
  document: "stage.reconstructing",
  epub: "stage.building",
  validate: "stage.checking",
  repair: "stage.repairing",
  "epub+validate+repair": "stage.buildingChecking",
  report: "stage.checking",
};

/** Warnings grouped by page, pages in order, page-less ones first (UI_UX §5). */
export function warningsByPage(report: Report): ReportWarning[] {
  return [...report.warnings].sort((a, b) => (a.page?.index ?? -1) - (b.page?.index ?? -1));
}
