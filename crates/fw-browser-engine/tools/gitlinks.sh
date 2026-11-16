#!/usr/bin/env bash
# Submodules of the imported upstreams arrive as bare gitlinks. harfbuzz is a build dependency of harfbuzz-sys:
# vendor its tree at the pinned commit (snapshot, no history). The html5lib test corpora are test data only:
# the gitlinks are removed and the pins stay here for `git clone` on demand.
set -euo pipefail
cd "$(dirname "$0")/.."
git rm -q --cached --ignore-unmatch src/lol_html/tests/data/html5lib-tests src/markup5ever_rcdom/html5lib-tests src/markup5ever_rcdom/xml5lib-tests >/dev/null
# on demand: html5lib-tests 85c968ac / f994590f (lol_html, rcdom), xml5lib-tests 9a7ca895 (rcdom)
if git ls-files -s src/harfbuzz-sys/harfbuzz | grep -q '^160000'; then
  sha=$(git ls-files -s src/harfbuzz-sys/harfbuzz | cut -d' ' -f2)
  git rm -q --cached src/harfbuzz-sys/harfbuzz
  tmp=$(mktemp -d)
  git clone -q --filter=blob:none https://github.com/harfbuzz/harfbuzz "$tmp/hb"
  git -C "$tmp/hb" checkout -q "$sha"
  rm -rf "$tmp/hb/.git" src/harfbuzz-sys/harfbuzz
  mv "$tmp/hb" src/harfbuzz-sys/harfbuzz
  rm -rf "$tmp"
fi
# keep only what harfbuzz-sys compiles
for d in src/harfbuzz-sys/harfbuzz/*; do case "$d" in */src|*/COPYING) ;; *) rm -rf "$d";; esac; done
