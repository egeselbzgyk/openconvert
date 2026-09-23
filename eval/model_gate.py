#!/usr/bin/env python3
"""The nine-gate model promotion test (D9, IMPLEMENTATION_PLAN PHASE 9 detail 9).

    eval/model_gate.py --model qwen3-1.7b-q4_k_m --machine L \\
        --llama-server vendor/llama-server/b10456/.../llama-server --llama-bench .../llama-bench \\
        --model-path ~/.local/share/openconvert/models/qwen3-1.7b-q4_k_m/Qwen3-1.7B-Q4_K_M.gguf \\
        --deterministic-seconds 142.0 --g7-pairs pairs.jsonl --g9-sha256 <hex>
    eval/model_gate.py --render-table
    eval/model_gate.py --emit-registry

Prints the pass/fail table, writes eval/results/model_gate/<model>__<build>__<machine>.json, and
exits non-zero unless all nine gates pass.
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "src"))

from oc_eval.model_gate.cli import main

if __name__ == "__main__":
    raise SystemExit(main())
