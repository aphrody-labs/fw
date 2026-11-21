#!/usr/bin/env bash
# One TSV line per top-level tree of the scanned roots and of the work volume: path, MB, link target, git remote, HEAD,
# uncommitted files, unpushed commits, modification date. Run on a host by tools/recover.ts:
#   ssh <host> bash -s -- <work> <root>...      (the roots are ay.toml [recovery].scan, the work volume [paths.<role>].work)
work="${1:-}"
shift
trees=()
for r in "$@" "$work"; do
  [ -n "$r" ] && [ -d "$r" ] && for d in "$r"/*; do [ -e "$d" ] && trees+=("$d"); done
done
sizes=$(mktemp)
printf '%s\n' "${trees[@]}" | xargs -P 16 -I{} sh -c '[ -L "{}" ] || printf "%s\t%s\n" "$(timeout 25 du -sxm "{}" 2>/dev/null | cut -f1)" "{}"' > "$sizes"
for d in "${trees[@]}"; do
  if [ -L "$d" ]; then printf '%s\t0\t%s\t\t\t0\t0\t\n' "$d" "$(readlink "$d")"; continue; fi
  mb=$(awk -F'\t' -v p="$d" '$2==p{print $1}' "$sizes" | head -1); mb=${mb:--1}
  mt=$(stat -c %y "$d" 2>/dev/null | cut -d' ' -f1)
  rem=; head=; dirty=0; ahead=0
  if [ -d "$d/.git" ]; then
    rem=$(git -C "$d" remote get-url origin 2>/dev/null); head=$(git -C "$d" rev-parse --short HEAD 2>/dev/null)
    dirty=$(git -C "$d" status --porcelain 2>/dev/null | wc -l)
    ahead=$(git -C "$d" rev-list --count '@{u}..HEAD' 2>/dev/null || echo 0)
  fi
  printf '%s\t%s\t\t%s\t%s\t%s\t%s\t%s\n' "$d" "$mb" "$rem" "$head" "$dirty" "$ahead" "$mt"
done
rm -f "$sizes"
