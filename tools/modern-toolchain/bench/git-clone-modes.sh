#!/usr/bin/env bash
# Clone modes of one repository, then the commands an agent runs on the clone. One JSON line per mode.
#   bash git-clone-modes.sh <url> <scratch-dir> [blame-path]
# MODES="blobless depth1" limits the modes; the commit-graph step is skipped for treeless clones (see below).
# Wall times in ms; size is du of .git in KiB. Run it on an idle node: clone time is network and CPU bound.
set -euo pipefail
url=${1:?url}
scratch=${2:?scratch dir}
blame_path=${3:-README.md}
mkdir -p "$scratch"
now() { date +%s%N; }
ms() { echo $(( ($(now) - $1) / 1000000 )); }

modes=${MODES:-"full blobless treeless depth1"}
[ -z "${MODES:-}" ] && command -v scalar > /dev/null && modes="$modes scalar"
for mode in $modes; do
  case $mode in
    full) flags=() ;;
    blobless) flags=(--filter=blob:none) ;;
    treeless) flags=(--filter=tree:0) ;;
    depth1) flags=(--depth=1) ;;
    scalar) flags=() ;;
  esac
  dir=$scratch/$mode
  rm -rf "$dir"
  start=$(now)
  if [ "$mode" = scalar ]; then
    # scalar clone: blobless, sparse cone (top-level files only), background maintenance registered, checkout included.
    scalar clone "$url" "$dir" > /dev/null
    dir=$dir/src
    clone_ms=$(ms "$start")
    checkout_ms=0
  else
    git clone -q --no-checkout "${flags[@]}" "$url" "$dir"
    clone_ms=$(ms "$start")
    start=$(now)
    git -C "$dir" checkout -q
    checkout_ms=$(ms "$start")
  fi
  size_kib=$(du -sk "$dir/.git" | cut -f1)
  files=$(git -C "$dir" ls-files | wc -l)
  start=$(now)
  commits=$(git -C "$dir" rev-list --count HEAD)
  revlist_ms=$(ms "$start")
  start=$(now)
  git -C "$dir" log --since=2026-11-01 --format=%h | wc -l > /dev/null
  log_ms=$(ms "$start")
  start=$(now)
  git -C "$dir" blame "$blame_path" > /dev/null
  blame_ms=$(ms "$start")
  # `commit-graph write --changed-paths` reads every tree: on a treeless clone it downloads them all (308 MB and more than 8 minutes
  # were observed before it was stopped), so the step is skipped there and reported as null.
  if [ "$mode" = treeless ]; then
    revlist_graph_ms=null
  else
    git -C "$dir" commit-graph write --reachable --changed-paths 2> /dev/null || true
    start=$(now)
    git -C "$dir" rev-list --count HEAD > /dev/null
    revlist_graph_ms=$(ms "$start")
  fi
  printf '{"mode":"%s","clone_ms":%s,"checkout_ms":%s,"git_kib":%s,"files":%s,"commits":%s,"rev_list_ms":%s,"rev_list_commit_graph_ms":%s,"log_since_ms":%s,"blame_ms":%s,"load":"%s"}\n' \
    "$mode" "$clone_ms" "$checkout_ms" "$size_kib" "$files" "$commits" "$revlist_ms" "$revlist_graph_ms" "$log_ms" "$blame_ms" "$(cut -d' ' -f1-3 /proc/loadavg)"
  [ "$mode" = scalar ] && scalar unregister "$dir" > /dev/null 2>&1 || true
  rm -rf "$scratch/$mode"
done
