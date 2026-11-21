#!/usr/bin/env bun
// Linker choice by measurement: builds fw-bin release once per node with a linker wrapper that records the real link
// command and its object files, then replays that exact link with each linker, three runs each, wall time and peak RSS.
//   bun linker-bench.ts [--package fw-bin] [--bin fw] [--profile release] [--runs 3]
// Nodes, work dirs and the linker tool dir come from cluster/build.toml; the bench clone lives under <work>/bench and is
// removed at the end. Replays write the same output path as the build, inside that throwaway clone.
import { join, posix, resolve } from "node:path";
import { loadConfig, type ClusterConfig } from "./build-cluster.ts";

const root = resolve(import.meta.dir, "..", "..", "..");

export const CAPTURE_WRAPPER = String.raw`#!/bin/bash
# Records the link of the final binary (output build/<package>/<hash>/out/<bin>, cargo's split build-dir layout), then runs the real linker.
cap="$LINK_CAPTURE"
joined="$*"
for a in "$@"; do case "$a" in @*) joined="$joined $(cat "\${a#@}")";; esac; done
if [[ "$joined" =~ -o[[:space:]]+\"?[^[:space:]\"]*/build/$LINK_PKG/[0-9a-f]+/out/$LINK_PACKAGE\"?([[:space:]]|$) ]] && [ ! -e "$cap/argv.bin" ]; then
  mkdir -p "$cap/files"
  printf '%s\0' "$@" > "$cap/argv.bin"
  for token in $joined; do
    token="\${token//\"/}"
    token="\${token#@}"
    case "$token" in
      /*) if [ -f "$token" ] && { [[ "$token" == *.o ]] || [[ "$token" == */linker-arguments ]] || [[ "$token" == @* ]]; }; then mkdir -p "$cap/files$(dirname "$token")"; cp -a "$token" "$cap/files$token"; fi;;
    esac
  done
  for a in "$@"; do case "$a" in @*) f="\${a#@}"; mkdir -p "$cap/files$(dirname "$f")"; cp -a "$f" "$cap/files$f";; esac; done
  for token in $joined; do
    token="\${token//\"/}"
    if [[ "$token" =~ ^(/.*/rustc[A-Za-z0-9]+)/ ]]; then d="\${BASH_REMATCH[1]}"; [ -d "$cap/files$d" ] || { mkdir -p "$cap/files$(dirname "$d")"; cp -a "$d" "$cap/files$d"; }; fi
  done
fi
exec cc "$@"
`;

// Runs the recorded cc command with extra flags; prints "<exit> <wall seconds> <peak RSS KiB>" (peak RSS of the whole child tree).
export const REPLAY_PY = `import resource, subprocess, sys, time
capture, name, tools = sys.argv[1], sys.argv[2], sys.argv[3]
argv = [a.decode() for a in open(capture, "rb").read().split(b"\\0")[:-1]]
gcc_ld = next((a[2:] for a in argv if a.startswith("-B") and a.endswith("/gcc-ld")), "")
kept = [a for a in argv if not a.startswith("-fuse-ld=") and not (a.startswith("-B") and a.endswith("/gcc-ld"))]
flags = {"lld": ["-B" + gcc_ld, "-fuse-ld=lld"], "bfd": ["-fuse-ld=bfd"], "mold": ["-B" + tools + "/mold-ld"], "wild": ["-B" + tools + "/wild-ld"]}[name]
start = time.monotonic()
done = subprocess.run(["cc", *flags, *kept], stdout=subprocess.DEVNULL, stderr=open(capture + ".err", "wb"))
print(done.returncode, round(time.monotonic() - start, 3), resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss)
`;

export function remoteScript(cfg: ClusterConfig, node: string, options: { pkg: string; bin: string; profile: string; runs: number }): string {
  const h = cfg.hosts[node]!;
  const linkers = posix.join(h.work, cfg.linkers.tools_dir);
  const bench = posix.join(h.work, "bench", "linker-fw");
  const wrapperB64 = Buffer.from(CAPTURE_WRAPPER.replaceAll("\\${", "${")).toString("base64");
  return [
    "set -e",
    "unset RUSTC_WRAPPER",
    `L='${linkers}'; B='${bench}'; CAP="$B/.capture"; src=$(ls -d '${h.work}/src/fw' "$HOME/src/fw" 2>/dev/null | head -1)`,
    // The node's own checkout can lag or carry other agents' edits: the bench builds the tip of the upstream main, cloned shallow from the checkout's remote.
    `mkdir -p '${h.work}/bench' "$CAP" "$L/wild-ld" "$L/mold-ld"`,
    `echo '${wrapperB64}' | base64 -d > "$CAP/cc-capture"; chmod +x "$CAP/cc-capture"`,
    `ln -sf "$L/bin/wild" "$L/wild-ld/ld"; ln -sf "$L/bin/mold" "$L/mold-ld/ld"`,
    // A capture left by an earlier run is replayed as is, so a failed replay does not cost another build.
    `if [ ! -e "$CAP/argv.bin" ]; then`,
    `  rm -rf "$B"; git clone -q --depth 1 "$(git -C "$src" -c safe.directory='*' remote get-url origin)" "$B"; mkdir -p "$CAP"; echo '${wrapperB64}' | base64 -d > "$CAP/cc-capture"; chmod +x "$CAP/cc-capture"`,
    `  cd "$B"; echo "commit=$(git rev-parse --short HEAD) cores=$(nproc)"`,
    `  export CARGO_TARGET_DIR="$B/target" LINK_CAPTURE="$CAP" LINK_PACKAGE='${options.bin}' LINK_PKG='${options.pkg}'`,
    `  build_start=$(date +%s)`,
    `  CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="$CAP/cc-capture" cargo build --profile ${options.profile} -p ${options.pkg} -q 2>&1 | tail -15; echo "cargo_exit=\${PIPESTATUS[0]}"`,
    `  echo "build_s=$(( $(date +%s) - build_start ))"`,
    `fi`,
    `[ -e "$CAP/argv.bin" ] || { ls "$B/target/${options.profile}/${options.bin}"; echo "no link captured (package ${options.pkg}, binary ${options.bin})"; exit 1; }`,
    // Restore the object files the linker deleted, then replay the recorded command once per linker and run.
    `(cd "$CAP/files" && find . -type f | while read -r f; do mkdir -p "$(dirname "/$f")"; cp -a "$f" "/$f"; done)`,
    `echo '${Buffer.from(REPLAY_PY).toString("base64")}' | base64 -d > "$CAP/replay.py"`,
    `replay() { python3 "$CAP/replay.py" "$CAP/argv.bin" "$1" "$L"; }`,
    `failed=; for name in lld bfd mold wild; do`,
    `  for run in $(seq 1 ${options.runs}); do line="link $name $run $(replay "$name")"; echo "$line"; case "$line" in "link $name $run 0 "*) ;; *) failed=1;; esac; done`,
    "done",
    `tail -3 "$CAP/argv.bin.err"`,
    `[ -z "$failed" ] && { cd /; rm -rf "$B"; } || true`,
  ].join("\n");
}

async function ssh(host: string, script: string): Promise<string> {
  const proc = Bun.spawn(["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=15", host, "bash -s"], { stdin: new Response(script + "\n"), stdout: "pipe", stderr: "pipe" });
  const [out, err, code] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited]);
  if (code !== 0) throw new Error(`ssh ${host} exit ${code}: ${(err || out).trim().slice(-600)}`);
  return out.trim();
}

export type LinkRun = { linker: string; run: number; seconds: number; peakRssMiB: number };

export function parseRuns(output: string): LinkRun[] {
  return output
    .split("\n")
    .map((line) => /^link (\w+) (\d+) 0 ([\d.]+) (\d+)$/.exec(line))
    .filter((m): m is RegExpExecArray => m !== null)
    .map((m) => ({ linker: m[1]!, run: Number(m[2]), seconds: Number(m[3]), peakRssMiB: Math.round(Number(m[4]) / 1024) }));
}

export function summarize(runs: LinkRun[]) {
  const names = [...new Set(runs.map((r) => r.linker))];
  return names.map((linker) => {
    const own = runs.filter((r) => r.linker === linker);
    const median = (values: number[]) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)]!;
    return { linker, runs: own.length, medianSeconds: median(own.map((r) => r.seconds)), medianPeakRssMiB: median(own.map((r) => r.peakRssMiB)) };
  });
}

if (import.meta.main) {
  const args = Bun.argv.slice(2);
  const option = (name: string, fallback: string) => (args.includes(name) ? args[args.indexOf(name) + 1]! : fallback);
  const cfg = await loadConfig(join(root, "cluster", "build.toml"));
  const pkg = option("--package", cfg.bench.package);
  const manifest = Bun.TOML.parse(await Bun.file(join(root, "crates", pkg, "Cargo.toml")).text()) as { bin?: { name: string }[]; package: { name: string } };
  const options = { pkg, bin: option("--bin", manifest.bin?.[0]?.name ?? pkg), profile: option("--profile", "release"), runs: Number(option("--runs", "3")) };
  const results = await Promise.all(
    Object.keys(cfg.hosts).map(async (node) => {
      const out = await ssh(cfg.hosts[node]!.ssh, remoteScript(cfg, node, options));
      return { node, target_cpu: cfg.hosts[node]!.target_cpu, header: out.split("\n").filter((l) => /^(commit|build_s)=/.test(l.trim()) || l.startsWith("commit=") || l.startsWith("build_s=")), summary: summarize(parseRuns(out)), runs: parseRuns(out) };
    }),
  );
  console.log(JSON.stringify(results, null, 2));
}
