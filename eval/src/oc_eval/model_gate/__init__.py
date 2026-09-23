"""The nine-gate model promotion test (D9, IMPLEMENTATION_PLAN PHASE 9 detail 9).

Which model may ever become the default is decided here and nowhere else: a model is promoted only
when all nine gates pass on both reference machines against the pinned llama.cpp build, and any
failure keeps it `experimental`. The machine-readable result files under
`eval/results/model_gate/` are the source of truth; `docs/MODEL_GATE.md` is generated from them.
"""
