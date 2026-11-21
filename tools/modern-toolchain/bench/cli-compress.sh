#!/usr/bin/env bash
# CLI codecs on one file: exact size and user-CPU ms (compress, decompress), best of 3. One JSON line per codec.
#   bash cli-compress.sh <file> [scratch-dir]
# User CPU time, not wall time: the nodes are shared and loaded.
set -euo pipefail
file=${1:?file}
scratch=${2:-$(mktemp -d)}
mkdir -p "$scratch"
input_size=$(stat -c %s "$file")
cpu_ms() { # runs the command, prints user+sys CPU ms
  local TIMEFORMAT='%3U %3S' out
  out=$( { time "$@" > /dev/null 2>&1; } 2>&1 )
  awk -v t="$out" 'BEGIN { split(t, a, " "); printf "%d", (a[1] + a[2]) * 1000 }'
}
best_of() { local best=999999999 value; for _ in 1 2 3; do value=$(cpu_ms "$@"); [ "$value" -lt "$best" ] && best=$value; done; echo "$best"; }

codec() { # name, compress command (stdin file -> stdout), decompress command
  local name=$1 compress=$2 decompress=$3
  local packed=$scratch/packed
  bash -c "$compress" < "$file" > "$packed"
  local size compress_ms decompress_ms
  size=$(stat -c %s "$packed")
  compress_ms=$(best_of bash -c "$compress < '$file'")
  decompress_ms=$(best_of bash -c "$decompress < '$packed'")
  bash -c "$decompress" < "$packed" | cmp -s - "$file" && verdict=roundtrip-ok || verdict=ROUNDTRIP-MISMATCH
  printf '{"file":"%s","input":%s,"codec":"%s","size":%s,"compress_cpu_ms":%s,"decompress_cpu_ms":%s,"check":"%s"}\n' "$(basename "$file")" "$input_size" "$name" "$size" "$compress_ms" "$decompress_ms" "$verdict"
}

codec "gzip -9" "gzip -9" "gzip -d"
codec "xz -9" "xz -9 -T1" "xz -d"
codec "zstd -3" "zstd -3 -T1 -q" "zstd -d -q"
codec "zstd -19" "zstd -19 -T1 -q" "zstd -d -q"
codec "zstd -19 --long=27" "zstd -19 --long=27 -T1 -q" "zstd -d --long=27 -q"
codec "brotli -q 11" "brotli -q 11 -c" "brotli -d -c"
codec "brotli -q 5" "brotli -q 5 -c" "brotli -d -c"
