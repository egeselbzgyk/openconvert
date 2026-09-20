"""CLI entry point for the offline evaluation tooling.

`eval/` is never a build or runtime dependency of the shipped product (D1).
"""

from __future__ import annotations

import json
from pathlib import Path

import typer

from oc_eval.corpus import download as download_mod
from oc_eval.corpus import lint as lint_mod
from oc_eval.corpus import manifest as manifest_mod

app = typer.Typer(help="OpenConvert offline evaluation tooling.")
corpus_app = typer.Typer(help="Corpus manifest, download and lint.")
app.add_typer(corpus_app, name="corpus")
gt_app = typer.Typer(help="Ground truth from sources that state their own structure.")
app.add_typer(gt_app, name="ground-truth")

# `eval/src/oc_eval/__main__.py` -> repository root.
REPO_ROOT = Path(__file__).resolve().parents[3]
DEFAULT_MANIFEST = REPO_ROOT / "corpus" / "manifest.json"
DEFAULT_DOWNLOAD_DIR = REPO_ROOT / "corpus" / "downloads"


@app.command()
def version() -> None:
    """Print the eval tooling version."""
    print("oc-eval 0.1.0")


@corpus_app.command("download")
def corpus_download(
    manifest: Path = typer.Option(DEFAULT_MANIFEST, help="Manifest to read entries from."),
    dest: Path = typer.Option(DEFAULT_DOWNLOAD_DIR, help="Where verified files are placed."),
    mirror_base: str = typer.Option("", help="Mirror URL prefix, tried before the source."),
    only: list[str] = typer.Option([], help="Restrict to these manifest ids."),
) -> None:
    """Fetch corpus files, verifying each against the manifest's sha256 before use."""
    entries = json.loads(manifest.read_text(encoding="utf-8"))["files"]
    wanted = set(only)
    entries = [entry for entry in entries if not wanted or entry["id"] in wanted]
    entries = [entry for entry in entries if not entry.get("local_eval_only")]
    entries = [entry for entry in entries if _is_remote(entry)]

    reused = fetched = 0
    for entry in entries:
        try:
            got = download_mod.fetch_entry(entry, dest, mirror_base=mirror_base or None)
        except (download_mod.ChecksumMismatch, download_mod.NoSourceAvailable) as failure:
            typer.echo(f"FAIL {failure}", err=True)
            raise typer.Exit(code=1) from failure
        reused += int(got.reused)
        fetched += int(not got.reused)
    typer.echo(f"{fetched} fetched, {reused} already present, into {dest}", err=True)


@corpus_app.command("lint")
def corpus_lint(
    manifest: Path = typer.Option(DEFAULT_MANIFEST, help="Manifest to lint."),
) -> None:
    """Check the manifest against TEST_CORPUS §7.1's rules. Exits 1 on any finding."""
    findings = lint_mod.lint(manifest_mod.load(manifest))
    for finding in findings:
        typer.echo(str(finding), err=True)
    if findings:
        typer.echo(f"{len(findings)} findings", err=True)
        raise typer.Exit(code=1)
    typer.echo("corpus lint: clean", err=True)


@corpus_app.command("harvest")
def corpus_harvest(
    manifest: Path = typer.Option(DEFAULT_MANIFEST, help="Manifest to merge admissions into."),
    dest: Path = typer.Option(DEFAULT_DOWNLOAD_DIR, help="Where admitted files are kept."),
    plan: str = typer.Option("", help="source=count pairs; empty means TEST_CORPUS §7.6."),
    write: bool = typer.Option(False, help="Write the manifest. Without it, nothing is saved."),
    max_file_mb: int = typer.Option(40, help="Skip any single file larger than this."),
) -> None:
    """Fetch, probe and admit real-world documents into the frozen holdout (TEST_CORPUS §7.6).

    Reaches the network. Without `--write` it downloads, probes and reports, and leaves the
    manifest alone — which is how a sourcing run is inspected before it is committed to.
    """
    from oc_eval.corpus import harvest as harvest_mod
    from oc_eval.corpus.sources import registry

    existing = manifest_mod.load(manifest)
    known = {item.id for item in existing.entries}
    report = harvest_mod.HarvestReport()

    for source_name, want in harvest_mod.plan_from(plan or registry.DEFAULT_PLAN):
        source = registry.SOURCES.get(source_name)
        if source is None:
            typer.echo(f"unknown source {source_name!r}", err=True)
            raise typer.Exit(code=2)
        before = len(report.admitted)
        held = known | {str(e["id"]) for e in report.admitted}
        harvest_mod.admit(
            # Over-ask. A source generator is lazy and `admit` stops at `want`, so the extra
            # costs nothing on a first run and is the only thing that lets a top-up reach past
            # the documents an earlier harvest already took.
            source(harvest_mod.ask_for(want, already_held=len(held))),
            dest_dir=dest,
            known_ids=held,
            report=report,
            max_file_bytes=max_file_mb * 1024 * 1024,
            want=want,
        )
        typer.echo(f"{source_name:12} admitted {len(report.admitted) - before}/{want}", err=True)

    typer.echo(
        f"admitted {len(report.admitted)}, rejected {len(report.rejected)} "
        f"({report.reasons}), {report.bytes_downloaded / 1e6:.1f} MB",
        err=True,
    )

    merged = harvest_mod.merge(existing, report.admitted)
    typer.echo(
        f"manifest would hold {len(merged.entries)} entries, "
        f"{merged.holdout_document_count} holdout documents, "
        f"ours share {merged.ours_share:.3f}",
        err=True,
    )
    if write:
        manifest_mod.dump(merged, manifest)
        typer.echo(f"wrote {manifest}", err=True)


@corpus_app.command("stats")
def corpus_stats(
    manifest: Path = typer.Option(DEFAULT_MANIFEST, help="Manifest to summarise."),
) -> None:
    """Per-stratum counts. D18 reports per stratum, never in aggregate."""
    loaded = manifest_mod.load(manifest)
    for stratum, items in loaded.by_stratum().items():
        holdout = sum(1 for item in items if item.holdout)
        tagged = sum(1 for item in items if item.tagged)
        typer.echo(f"{stratum:20} n={len(items):4}  holdout={holdout:4}  tagged={tagged:4}")
    typer.echo(
        f"{'TOTAL':20} n={len(loaded.entries):4}  "
        f"holdout-documents={loaded.holdout_document_count:4}  "
        f"ours-share={loaded.ours_share:.3f}"
    )


@gt_app.command("from-xhtml")
def gt_from_xhtml(
    sources: list[Path] = typer.Argument(..., help="XHTML files, in spine order."),
    title: str = typer.Option("", help="The book's title."),
    out: Path = typer.Option(Path("-"), help="Where to write the ground truth JSON."),
    assertions: bool = typer.Option(False, help="Write assertions instead of ground truth."),
) -> None:
    """Standard Ebooks XHTML to ground truth (TEST_CORPUS §5.1)."""
    from oc_eval.ground_truth import from_xhtml, schema

    truth = from_xhtml.ground_truth(sources, title=title or sources[0].stem)
    payload = schema.to_assertions(truth) if assertions else schema.to_json(truth)
    _emit(payload, out)


@gt_app.command("from-structtree")
def gt_from_structtree(
    source: Path = typer.Argument(..., help="A tagged PDF."),
    out: Path = typer.Option(Path("-"), help="Where to write the ground truth JSON."),
    assertions: bool = typer.Option(False, help="Write assertions instead of ground truth."),
) -> None:
    """A tagged PDF's own structure tree as ground truth (TEST_CORPUS §5.4)."""
    from oc_eval.ground_truth import from_structtree, schema

    try:
        truth = from_structtree.ground_truth(source)
    except from_structtree.NoStructTree as failure:
        typer.echo(str(failure), err=True)
        raise typer.Exit(code=1) from failure
    payload = schema.to_assertions(truth) if assertions else schema.to_json(truth)
    _emit(payload, out)


@gt_app.command("from-latex")
def gt_from_latex(
    source: Path = typer.Argument(..., help="A LaTeX source file."),
    title: str = typer.Option("", help="The paper's title."),
    out: Path = typer.Option(Path("-"), help="Where to write the ground truth JSON."),
    assertions: bool = typer.Option(False, help="Write assertions instead of ground truth."),
) -> None:
    """arXiv LaTeX to ground truth (TEST_CORPUS §5.2). Refuses a paper it cannot read."""
    from oc_eval.ground_truth import from_latex, schema

    text = source.read_text(encoding="utf-8", errors="replace")
    if not from_latex.looks_parseable(text):
        typer.echo(
            f"{source.name}: no sectioning commands this reader can follow — "
            "TEST_CORPUS §5.2's curated subset excludes it",
            err=True,
        )
        raise typer.Exit(code=1)
    truth = from_latex.ground_truth(text, title=title or source.stem)
    payload = schema.to_assertions(truth) if assertions else schema.to_json(truth)
    _emit(payload, out)


def _emit(payload: object, out: Path) -> None:
    text = json.dumps(payload, indent=2, ensure_ascii=False) + "\n"
    if str(out) == "-":
        typer.echo(text)
        return
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text, encoding="utf-8")
    typer.echo(f"wrote {out}", err=True)


def _is_remote(entry: dict[str, object]) -> bool:
    source = entry.get("source")
    url = source.get("url", "") if isinstance(source, dict) else ""
    return isinstance(url, str) and url.startswith("http")


if __name__ == "__main__":
    app()
