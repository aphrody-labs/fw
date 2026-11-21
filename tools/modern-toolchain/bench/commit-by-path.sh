#!/usr/bin/env bash
# The agents' commit-by-path flow without the push: temporary index, read-tree, update-index --cacheinfo, write-tree, commit-tree.
#   bash commit-by-path.sh <repo> <path> [runs]
# Prints the median wall time in ms of the whole flow and of each git call. Nothing is written to refs.
set -euo pipefail
repo=${1:?repo}
path=${2:?path}
runs=${3:-7}
cd "$repo"
now() { date +%s%N; }
samples=()
for _ in $(seq "$runs"); do
  index=$(mktemp -u)
  export GIT_INDEX_FILE=$index
  t0=$(now)
  git read-tree HEAD
  t1=$(now)
  blob=$(git hash-object -w -- "$path")
  git update-index --add --cacheinfo "100644,$blob,$path"
  t2=$(now)
  tree=$(git write-tree)
  t3=$(now)
  git commit-tree "$tree" -p HEAD -m "bench" > /dev/null
  t4=$(now)
  unset GIT_INDEX_FILE
  rm -f "$index"
  samples+=("$(( (t1 - t0) / 1000000 )) $(( (t2 - t1) / 1000000 )) $(( (t3 - t2) / 1000000 )) $(( (t4 - t3) / 1000000 )) $(( (t4 - t0) / 1000000 ))")
done
printf '%s\n' "${samples[@]}" | sort -k5 -n | sed -n "$(( (runs + 1) / 2 ))p" | awk '{printf "{\"read_tree_ms\":%s,\"hash_and_update_index_ms\":%s,\"write_tree_ms\":%s,\"commit_tree_ms\":%s,\"total_ms\":%s}\n",$1,$2,$3,$4,$5}'
