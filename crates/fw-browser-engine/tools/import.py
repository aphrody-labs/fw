#!/usr/bin/env python3
"""Import (or re-sync) one upstream into the monorepo with full history.

usage: import.py <monorepo> <workdir> <name>...        (names from tools/upstreams.toml)

`mode = "pin"` upstreams (large non-crate trees, e.g. JetStream) are never imported: the branch head is resolved live
with `git ls-remote` and recorded in tools/upstreams.lock.json, nothing else.

Per upstream: shallow-free single-branch clone, git-filter-repo rewrite so that every crate lives in
third_party/<name>/<package> and every other file in third_party/<name>/ (the final locations, tools/layout.toml: the rewrite
stays deterministic, a re-run on a newer upstream head produces the same SHAs for the old commits), then the commits that
are new since the last import (lock field `imported`) are replayed onto main one by one: main requires a linear history,
so nothing is merged (conflicts of a commit
picked onto a different base resolve to the commit's side; a first import is then forced to the exact upstream tree). The history is rewritten straight into the final locations because a replay across a directory move
cannot be done by git (a commit that renames a file inside a moved directory is a rename/rename conflict, e.g. hyperfine
80e436030), whereas identical paths on both sides pick cleanly. `mode = "bun"` still goes through the legacy locations
that `tools/layout.ts` places afterwards.
"""
import json, subprocess, sys, os, tomllib, shutil

RESERVED = {".github", ".cargo", ".taskcluster.yml", ".circleci", ".gitlab", "Cargo.lock",
            "rust-toolchain.toml", "rust-toolchain", ".gitmodules", ".husky", ".claude"}


def run(*a, cwd=None, env=None, out=False):
    e = dict(os.environ, RUSTUP_TOOLCHAIN="nightly", **(env or {}))
    r = subprocess.run(a, cwd=cwd, env=e, check=True, text=True, capture_output=out)
    return r.stdout.strip() if out else None


def layout(mono):
    with open(os.path.join(mono, "tools", "layout.toml"), "rb") as f:
        return tomllib.load(f)


def taken_dirs(mono):
    """Directory names in use anywhere in the layout: a new crate must not take one."""
    lay = layout(mono)
    names = set()
    for d in (os.path.join(mono, "src"), os.path.join(mono, lay["first_party"]["dir"]),
              os.path.join(mono, lay["bun"]["dir"], "src")):
        if os.path.isdir(d):
            names |= set(os.listdir(d))
    tp = os.path.join(mono, lay["third_party"]["dir"])
    for u in os.listdir(tp) if os.path.isdir(tp) else []:
        if os.path.isdir(os.path.join(tp, u)):
            names |= set(os.listdir(os.path.join(tp, u)))
    return names


def crate_map(work, name, taken, dest, manifest="Cargo.toml"):
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
        pairs.append((d, f"{dest}/{t}"))
    pairs.sort(key=lambda x: -len(x[0]) if x[0] != "." else 1)
    return pairs


CB = r'''
import json
M = %(m)s
NAME = %(n)r
DROP = %(drop)s
RES = %(res)s
DEST = %(dest)r
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
return ("%%s/%%s" %% (DEST, s)).encode("utf-8", "surrogateescape")
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
        if u.get("mode") == "pin":
            sha = run("git", "ls-remote", u["url"], f"refs/heads/{u['branch']}", out=True).split()[0]
            lock[n] = {"url": u["url"], "branch": u["branch"], "sha": sha, "crates": {}}
            json.dump(lock, open(lock_path, "w"), indent=1, sort_keys=True)
            print("pinned", n, sha[:12], flush=True)
            continue
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
            own = {t.rsplit("/", 1)[1] for t in lock.get(n, {}).get("crates", {}).values()}
            dest = f"{layout(mono)['third_party']['dir']}/{n}"
            mp = crate_map(w, n, taken_dirs(mono) - own, dest, u.get("manifest", "Cargo.toml"))
            cb = CB % {"m": json.dumps(mp), "n": n, "drop": json.dumps(u.get("drop", [])),
                       "res": json.dumps(sorted(RESERVED)), "dest": dest}
        run("git", "filter-repo", "--force", "--filename-callback", cb, cwd=w)
        ref = f"refs/imports/{n}"
        run("git", "fetch", "-q", "--no-tags", w, f"+{u['branch']}:{ref}", cwd=mono)
        head = run("git", "rev-parse", ref, cwd=mono, out=True)
        last = lock.get(n, {}).get("imported")
        span = f"{last}..{ref}" if last else ref
        new = run("git", "rev-list", "--reverse", "--topo-order", "--no-merges", span, cwd=mono, out=True).split()
        pick = ["git", "cherry-pick", "-X", "theirs", "--allow-empty", "--keep-redundant-commits"]
        for i in range(0, len(new), 200):
            run(*pick, *new[i:i + 200], cwd=mono)
        sub = f"{layout(mono)['third_party']['dir']}/{n}"
        if not last and run("git", "rev-parse", f"HEAD:{sub}", cwd=mono, out=True) != \
                run("git", "rev-parse", f"{ref}:{sub}", cwd=mono, out=True):
            # dropping merge commits loses their conflict resolutions: a first import ends on exactly the upstream tree
            run("git", "rm", "-rqf", "--cached", "--", sub, cwd=mono)
            run("git", "read-tree", f"--prefix={sub}/", ref + ":" + sub, cwd=mono)
            run("git", "checkout-index", "-fa", cwd=mono)
            run("git", "clean", "-fdq", "--", sub, cwd=mono)
            run("git", "commit", "-q", "-m", f"import {n}: resolutions of the merge commits not replayed", cwd=mono)
        run("git", "commit", "-q", "--allow-empty", "-m",
            f"import {n} {u['url']} {u['branch']} @ {sha[:12]} (history preserved: {len(new)} commits replayed linearly, crates under third_party/{n})",
            cwd=mono)
        old = lock.get(n, {}).get("crates", {})
        lock[n] = {"url": u["url"], "branch": u["branch"], "sha": sha, "imported": head, "crates": {**old, **dict(mp)}}
        json.dump(lock, open(lock_path, "w"), indent=1, sort_keys=True)
        run("bun", "tools/layout.ts", "apply", ".", cwd=mono)       # place the legacy locations; also rewrites the lock paths
        run("git", "add", "-A", "tools/upstreams.lock.json", cwd=mono)
        if subprocess.run(["git", "diff", "--cached", "--quiet"], cwd=mono).returncode != 0:
            run("git", "commit", "-q", "-m", f"layout: place {n} (tools/layout.toml)" if u.get("mode") == "bun"
                else f"lock: {n} @ {sha[:12]} (tools/upstreams.lock.json)", cwd=mono)
        shutil.rmtree(w)
        print("imported", n, sha[:12], len(mp), "crates", flush=True)


main()
