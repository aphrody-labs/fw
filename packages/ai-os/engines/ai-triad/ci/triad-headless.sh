#!/usr/bin/env sh
set -eu
PROMPT="${1:-Run concise security and type-safety review of current git diff.}"

echo "[Triad CI] Running headless Claude, Codex, and Gemini checks..."
if command -v claude >/dev/null 2>&1; then
  claude --dangerously-skip-permissions -p "$PROMPT" --output-format json > triad-claude.json
fi
if command -v codex >/dev/null 2>&1; then
  codex exec "$PROMPT" --json --skip-git-repo-check > triad-codex.jsonl
fi
if command -v gemini >/dev/null 2>&1; then
  gemini -p "$PROMPT" -o json > triad-gemini.json
fi
echo "[Triad CI] Completed."
