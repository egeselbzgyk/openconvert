"""Deterministic-only against deterministic+LLM, per task, per assertion category, per language.

PHASE 10 detail 7 (R9 §C.7/C.8, D9 G7). Each task is run over the corpus twice and every assertion
is scored both ways; the discordant pairs decide. `b` is an assertion only the LLM path passed,
`c` one only the deterministic path passed — a **false repair**: the LLM broke something that was
right. A task ships enabled-by-opt-in for a language only if it is non-inferior overall there and
its false-repair rate `c / n` is at most `ai_eval.false_repair_max` in every category.

`docs/AI_EVALUATION.md` is rendered from `eval/data/ai_eval/outcomes.jsonl` and never edited by
hand; `python -m oc_eval.compare --render` writes it, `--check` holds it equal, `--gate` is the CI
gate of row 10.18.
"""
