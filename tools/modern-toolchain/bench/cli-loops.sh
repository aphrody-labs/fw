#!/usr/bin/env bash
# The agents' search and transform loops, legacy tool against its modern replacement, with hyperfine 2 (hardware counters).
#   bash cli-loops.sh <tree-dir> <jsonl-file> <text-file>
# One JSON line per command: median instructions (independent of node load), CPU ms and wall ms.
set -euo pipefail
tree=${1:?tree dir}
jsonl=${2:?jsonl file}
text=${3:?text file}
out=$(mktemp)
pair() { # label, legacy command, modern command
  hyperfine -N -w 2 -r 7 --ignore-failure --export-json "$out" "$2" "$3" > /dev/null
  bun -e 'const [file, label] = process.argv.slice(1); for (const e of (await Bun.file(file).json()).results) console.log(JSON.stringify({step: label, command: e.command, instructions: e.summary.instructions?.median ?? null, cpu_ms: +(e.summary.time_cpu.median * 1000).toFixed(1), wall_ms: +(e.summary.time_wall_clock.median * 1000).toFixed(1)}))' "$out" "$1"
}
pair "content search" "grep -rIl --exclude-dir=.git fn_main_never_matches_xyz $tree" "rg -l fn_main_never_matches_xyz $tree"
pair "content search, many hits" "grep -rIl --exclude-dir=.git serde $tree" "rg -l serde $tree"
pair "file search" "find $tree -name *.toml -not -path */.git/*" "fd -e toml . $tree"
pair "json lines" "jq -c . $jsonl" "jaq -c . $jsonl"
pair "text replace" "sed s/serde/serde/g $text" "sd serde serde $text"
rm -f "$out"
