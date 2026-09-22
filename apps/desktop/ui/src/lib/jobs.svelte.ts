/**
 * The queue as reactive state: every row a projection of `job-changed` and `engine-line`
 * (UI_UX §8). The rules are `jobstate.ts`'s; this only holds the result where Svelte can see it.
 */

import { parseLine, ProtocolError, PROTOCOL_VERSION } from "./events";
import {
  applyEvent,
  applyView,
  checkHeartbeat,
  newRow,
  type JobView,
  type Row,
} from "./jobstate";

/** Why the app cannot run at all (the only blocking screens: firstrun.html §2). */
export type Blocking =
  | { kind: "protocol"; app: number; engine: number | null }
  | { kind: "ir"; app: number; engine: number | null }
  | { kind: "version"; app: string; engine: string }
  | { kind: "missing"; detail: string };

export class JobStore {
  rows = $state<Row[]>([]);
  blocking = $state<Blocking | null>(null);
  /** The clock the rows were last judged against, for "no signal for {s} seconds". */
  now = $state(0);

  constructor(private readonly heartbeatTimeoutMs: number) {}

  /** A queue change from the Rust side. */
  view(view: JobView): void {
    const index = this.rows.findIndex((row) => row.id === view.id);
    if (index < 0) {
      this.rows.push(newRow(view));
    } else {
      this.rows[index] = applyView(this.rows[index] as Row, view);
    }
  }

  /** Replace the whole queue with the Rust side's rows, in its order. */
  load(views: JobView[]): void {
    const known = new Map(this.rows.map((row) => [row.id, row]));
    this.rows = views.map((view) => {
      const row = known.get(view.id);
      return row === undefined ? newRow(view) : applyView(row, view);
    });
  }

  /** Forget rows the Rust side no longer has. */
  keepOnly(ids: ReadonlySet<string>): void {
    this.rows = this.rows.filter((row) => ids.has(row.id));
  }

  /**
   * One line from job `job`'s engine. A protocol this UI does not speak stops the app with a
   * blocking error — not a row marked failed, not a UI that carries on guessing (test 12.3).
   */
  line(job: string, text: string, now: number): void {
    let event;
    try {
      event = parseLine(text);
    } catch (error) {
      if (error instanceof ProtocolError) {
        this.blocking = { kind: "protocol", app: PROTOCOL_VERSION, engine: error.engine };
        return;
      }
      throw error;
    }
    if (event === null) return;
    const index = this.rows.findIndex((row) => row.id === job);
    if (index < 0) return;
    this.rows[index] = applyEvent(this.rows[index] as Row, event, now);
  }

  /** Judge every row's heartbeat against `now`. */
  tick(now: number): void {
    this.now = now;
    for (const [index, row] of this.rows.entries()) {
      const next = checkHeartbeat(row, now, this.heartbeatTimeoutMs);
      if (next !== row) this.rows[index] = next;
    }
  }
}
