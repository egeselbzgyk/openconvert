#!/usr/bin/env bash
# Headless implementation loop. One Claude Code run = one work item.
# Usage:  ./scripts/loop.sh            # runs until COMPLETE / BLOCKED / MAX_ITER
#         MAX_ITER=5 ./scripts/loop.sh # short run
#         touch .loop/STOP             # graceful stop from another terminal
set -uo pipefail
cd "$(dirname "$0")/.."
mkdir -p .loop
MAX_ITER="${MAX_ITER:-200}"
MODEL="${MODEL:-opus}"
LOG=".loop/log-$(date +%Y%m%d-%H%M%S).md"

for ((i=1; i<=MAX_ITER; i++)); do
  if grep -q '^STATUS: COMPLETE' PROGRESS.md; then echo "✓ COMPLETE"; exit 0; fi
  if grep -q '^STATUS: BLOCKED'  PROGRESS.md; then echo "⚠ BLOCKED — read PROGRESS.md ## Blocked"; exit 2; fi
  if [ -f .loop/STOP ]; then rm -f .loop/STOP; echo "■ stopped by request"; exit 0; fi

  echo "=== iteration $i · $(date -u +%H:%M:%SZ) ===" | tee -a "$LOG"
  claude -p "$(cat .claude/loop-prompt.md)" \
      --model "$MODEL" \
      --permission-mode acceptEdits \
      --max-turns 150 \
      2>&1 | tee -a "$LOG"
  rc=$?
  if [ $rc -ne 0 ]; then echo "claude exited $rc — pausing loop" | tee -a "$LOG"; exit $rc; fi

  # commit any stray changes to PROGRESS.md so the next iteration starts clean
  git add -A PROGRESS.md >/dev/null 2>&1 || true
  git diff --cached --quiet || git commit -q -m "progress: iteration $i" || true
  sleep 2
done
echo "reached MAX_ITER=$MAX_ITER"
