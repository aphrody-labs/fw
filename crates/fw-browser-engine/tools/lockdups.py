#!/usr/bin/env python3
"""Duplicate-version report from Cargo.lock files: lockdups.py <Cargo.lock>... (name -> versions)."""
import sys, tomllib, json, collections
tot = collections.defaultdict(set)
for f in sys.argv[1:]:
    with open(f, "rb") as fh:
        d = tomllib.load(fh)
    v = collections.defaultdict(set)
    for p in d.get("package", []):
        if "source" in p:               # registry/git packages only, workspace members excluded
            v[p["name"]].add(p["version"]); tot[p["name"]].add(p["version"])
    dups = {n: sorted(x) for n, x in v.items() if len(x) > 1}
    print(f"{f}: {sum(len(x) for x in v.values())} external packages, {len(v)} names, {len(dups)} names with >1 version")
dups = {n: sorted(x) for n, x in tot.items() if len(x) > 1}
print(f"UNION of all locks: {sum(len(x) for x in tot.values())} package versions, {len(tot)} names, {len(dups)} names with >1 version")
if "--json" in sys.argv[0:0]:
    print(json.dumps(dups))
