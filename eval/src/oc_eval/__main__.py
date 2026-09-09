"""CLI entry point for the offline evaluation tooling.

`eval/` is never a build or runtime dependency of the shipped product (D1).
"""

import typer

app = typer.Typer(help="OpenConvert offline evaluation tooling.")


@app.command()
def version() -> None:
    """Print the eval tooling version."""
    print("oc-eval 0.1.0")


if __name__ == "__main__":
    app()
