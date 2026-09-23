"""`python -m oc_eval.compare --render | --check | --gate` (PHASE 10 rows 10.17, 10.18)."""

from __future__ import annotations

import argparse
import sys

from oc_eval.compare import false_repair, render


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="python -m oc_eval.compare")
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--render", action="store_true", help="write docs/AI_EVALUATION.md")
    mode.add_argument("--check", action="store_true", help="fail if the file is not the render")
    mode.add_argument("--gate", action="store_true", help="row 10.18: the false-repair gate")
    args = parser.parse_args(argv)

    if args.render:
        render.DOC_PATH.write_text(render.render_committed(), encoding="utf-8")
        return 0
    if args.check:
        current = render.DOC_PATH.read_text(encoding="utf-8") if render.DOC_PATH.exists() else ""
        if current != render.render_committed():
            print(f"{render.DOC_PATH} is stale: run python -m oc_eval.compare --render")
            return 1
        return 0
    rows = false_repair.table(false_repair.load(render.OUTCOMES_PATH))
    verdict = false_repair.gate(
        rows, {task: false_repair.enabled_languages(task) for task in false_repair.TASKS}
    )
    for failure in verdict.failures:
        print(failure)
    return 0 if verdict.passed else 1


if __name__ == "__main__":
    sys.exit(main())
