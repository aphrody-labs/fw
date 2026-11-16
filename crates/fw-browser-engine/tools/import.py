#!/usr/bin/env python3
"""Import (or re-sync) one upstream into the monorepo with full history.

usage: import.py <monorepo> <workdir> <name>...        (names from tools/upstreams.toml)

Per upstream: shallow-free single-branch clone, git-filter-repo rewrite so that every crate lives in
src/<package> (flat, Bun layout) and every other file in upstream/<name>/, then
`git merge --allow-unrelated-histories` into the monorepo. The rewrite is deterministic: a re-run on a
newer upstream head produces the same SHAs for the old commits, so a re-sync merges only the new ones.
"""
import json, subprocess, sys, os, tomllib, shutil

RESERVED = {".github", ".cargo", ".taskcluster.yml", ".circleci", ".gitlab", "Cargo.lock",
            "rust-toolchain.toml", "rust-toolchain", ".gitmodules", ".husky", ".claude"}


def run(*a, cwd=None, env=None, out=False):
    e = dict(os.environ, RUSTUP_TOOLCHAIN="nightly", **(env or {}))
    r = subprocess.run(a, cwd=cwd, env=e, check=True, text=True, capture_output=out)
    return r.stdout.strip() if out else None


def taken_dirs(mono):
    p = os.path.join(mono, "src")
    return set(os.listdir(p)) if os.path.isdir(p) else set()


def crate_map(work, name, taken, manifest="Cargo.toml"):
    meta = json.loads(run("cargo", "metadata", "--no-deps", "--format-version", "1",
                          "--manifest-path", manifest, cwd=work, out=True))
    root = os.path.realpath(work)
    pairs, used = [], set(taken)
    for p in sorted(meta["packages"], key=lambda p: p["name"]):
        d = os.path.relpath(os.path.dirname(p["manifest_path"]), root).replace("\\", "/")
        if d.startswith(".."):
            continue
        t = p["name"]
        if t in used:
            t = f"{name}-{t}"
        used.add(t)
        pairs.append((d, "src/" + t))
    pairs.sort(key=lambda x: -len(x[0]) if x[0] != "." else 1)
    return pairs


CB = r'''
import json
M = %(m)s
NAME = %(n)r
DROP = %(drop)s
RES = %(res)s
s = filename.decode("utf-8", "surrogateescape")
for d in DROP:
    if s == d or s.startswith(d + "/"):
        return None
root = None
for d, t in M:
    if d == ".":
        root = t
        continue
    if s == d or s.startswith(d + "/"):
        return (t + s[len(d):]).encode("utf-8", "surrogateescape")
top = s.split("/")[0]
if root is not None and top not in RES:
    return (root + "/" + s).encode("utf-8", "surrogateescape")
return ("upstream/%%s/%%s" %% (NAME, s)).encode("utf-8", "surrogateescape")
'''

CB_BUN = r'''
s = filename.decode("utf-8", "surrogateescape")
if s.startswith("src/"):
    return filename
if s in KEEP:
    return ("upstream/bun/" + s).encode()
return None
'''


def main():
    mono, wd, names = sys.argv[1], sys.argv[2], sys.argv[3:]
    with open(os.path.join(mono, "tools", "upstreams.toml"), "rb") as f:
        spec = {u["name"]: u for u in tomllib.load(f)["upstream"]}
    lock_path = os.path.join(mono, "tools", "upstreams.lock.json")
    lock = json.load(open(lock_path)) if os.path.exists(lock_path) else {}
    for n in names:
        u = spec[n]
        w = os.path.join(wd, n)
        if os.path.exists(w):
            shutil.rmtree(w)
        run("git", "clone", "-q", "--no-tags", "--single-branch", "--branch", u["branch"], u["url"], w)
        if u.get("rev"):
            run("git", "fetch", "-q", "--no-tags", "origin", u["rev"], cwd=w)
            run("git", "reset", "-q", "--hard", u["rev"], cwd=w)
        sha = run("git", "rev-parse", "HEAD", cwd=w, out=True)
        if u.get("mode") == "bun":
            cb = "KEEP = %r\n" % (u.get("keep", [])) + CB_BUN
            mp = []
        else:
            own = {t.split("/", 1)[1] for t in lock.get(n, {}).get("crates", {}).values()}
            mp = crate_map(w, n, taken_dirs(mono) - own, u.get("manifest", "Cargo.toml"))
            cb = CB % {"m": json.dumps(mp), "n": n, "drop": json.dumps(u.get("drop", [])),
                       "res": json.dumps(sorted(RESERVED))}
        run("git", "filter-repo", "--force", "--filename-callback", cb, cwd=w)
        ref = f"refs/imports/{n}"
        run("git", "fetch", "-q", "--no-tags", w, f"+{u['branch']}:{ref}", cwd=mono)
        run("git", "merge", "-q", "--allow-unrelated-histories", "--no-edit", "-m",
            f"import {n} {u['url']} {u['branch']} @ {sha[:12]} (history preserved, crates flattened into src/)",
            ref, cwd=mono)
        lock[n] = {"url": u["url"], "branch": u["branch"], "sha": sha, "crates": dict(mp)}
        json.dump(lock, open(lock_path, "w"), indent=1, sort_keys=True)
        shutil.rmtree(w)
        print("imported", n, sha[:12], len(mp), "crates", flush=True)


main()
