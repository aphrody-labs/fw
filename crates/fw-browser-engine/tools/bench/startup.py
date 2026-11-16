#!/usr/bin/env python3
"""Cold start + load + peak process-tree RSS of headless engines on one URL.

usage: startup.py <url> <runs> name=cmd...   ({url} and {out} are substituted)
Peak RSS = max over 5 ms samples of the summed VmRSS of the process tree (children included).
"""
import os, subprocess, sys, time, statistics, shlex, json


def tree_rss(pid):
    seen, stack, total = set(), [pid], 0
    while stack:
        p = stack.pop()
        if p in seen:
            continue
        seen.add(p)
        try:
            with open(f"/proc/{p}/status") as f:
                for line in f:
                    if line.startswith("VmRSS:"):
                        total += int(line.split()[1])
            with open(f"/proc/{p}/task/{p}/children") as f:
                stack += [int(x) for x in f.read().split()]
        except OSError:
            pass
    return total


def once(cmd):
    t0 = time.perf_counter()
    p = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    peak = 0
    while p.poll() is None:
        peak = max(peak, tree_rss(p.pid))
        time.sleep(0.005)
    return time.perf_counter() - t0, peak / 1024, p.returncode


def main():
    url, runs = sys.argv[1], int(sys.argv[2])
    res = {}
    for spec in sys.argv[3:]:
        name, cmd = spec.split("=", 1)
        out = f"/tmp/bench-{name}.png"
        argv = shlex.split(cmd.replace("{url}", url).replace("{out}", out))
        rows = [once(argv) for _ in range(runs)]
        ok = [r for r in rows if r[2] == 0]
        res[name] = {
            "ok": f"{len(ok)}/{runs}",
            "wall_s_median": round(statistics.median(r[0] for r in ok), 3) if ok else None,
            "peak_rss_mib_median": round(statistics.median(r[1] for r in ok), 1) if ok else None,
            "out_bytes": os.path.getsize(out) if os.path.exists(out) else None,
        }
    print(json.dumps(res, indent=1))


main()
