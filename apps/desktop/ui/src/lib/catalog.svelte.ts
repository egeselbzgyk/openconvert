/**
 * The models and packs screens' state: the Rust side's rows, kept current by its events
 * (`model-changed`, `pack-changed`), and the licence texts the rows show before a download.
 *
 * Nothing here computes a number: every size, RAM figure, CPU word and byte count is a field of
 * the row the model manager sent (Phase 12 detail 9 — "renders exactly the ModelReadiness fields").
 */

import type { Backend, CatalogKind, CatalogRow, LicenseView, ModelRow, UiError } from "./backend";

export class Catalog<K extends CatalogKind> {
  /** `null` until loaded. */
  rows = $state<Array<CatalogRow<K>> | null>(null);
  /** Why nothing can be downloaded in this build, when nothing can. */
  unavailable = $state<string | null>(null);
  /** Licence texts by id, fetched when a row shows one. */
  licenses = $state<Record<string, LicenseView>>({});
  /** The last refusal from the Rust side, by row id, for the row to say. */
  errors = $state<Record<string, string>>({});

  constructor(
    private readonly backend: Backend,
    readonly kind: K,
  ) {}

  async load(): Promise<void> {
    const view = await this.backend.catalog(this.kind);
    this.unavailable = view.unavailable;
    this.rows = view.rows as Array<CatalogRow<K>>;
  }

  /** A row the Rust side announced. */
  apply(row: CatalogRow<K>): void {
    if (this.rows === null) return;
    this.rows = this.rows.map((known) => (known.id === row.id ? row : known));
  }

  row(id: string): CatalogRow<K> | undefined {
    return this.rows?.find((row) => row.id === id);
  }

  async showLicense(id: string): Promise<void> {
    if (id in this.licenses) return;
    const license = await this.backend.license(this.kind, id);
    this.licenses = { ...this.licenses, [id]: license };
  }

  /** "Accept licence and download": the acceptance, then the download. */
  async acceptAndDownload(id: string): Promise<void> {
    await this.run(id, async () => {
      await this.backend.acceptLicense(this.kind, id);
      await this.backend.download(this.kind, id);
    });
  }

  async download(id: string): Promise<void> {
    await this.run(id, () => this.backend.download(this.kind, id));
  }

  async cancel(id: string): Promise<void> {
    await this.run(id, () => this.backend.cancelDownload(this.kind, id));
  }

  async remove(id: string): Promise<void> {
    await this.run(id, () => this.backend.removeDownload(this.kind, id));
  }

  private async run(id: string, work: () => Promise<void>): Promise<void> {
    const { [id]: _, ...rest } = this.errors;
    this.errors = rest;
    try {
      await work();
    } catch (error) {
      this.errors = { ...this.errors, [id]: (error as UiError).kind ?? "io" };
    }
  }
}

/** The registry's default model, if the registry is usable and names one. */
export function defaultModel(models: Catalog<"models"> | null): ModelRow | undefined {
  return models?.rows?.find((row) => row.is_default);
}
