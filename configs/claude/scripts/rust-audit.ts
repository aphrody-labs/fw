#!/usr/bin/env bun
// Measurement suite for the fw Rust workspace, run on a build host over ssh (never on the PC).
//   bun scripts/rust-audit.ts [--host fr1|local] [--dir ~/ci/fw-opt] [--steps build,rebuild,...] [--label before] [--out file.json]
//   bun scripts/rust-audit.ts --compare before.json after.json
// Put --dir on a roomy volume (fr1: /srv/xbuild/<task>/fw): target dirs and TMPDIR of the experiments live next to it.
// The checkout in --dir is measured as it stands (no reset), so the same command gives the before and after numbers.
// Wall time on a shared host is noisy: every timed step also reports user+sys CPU seconds, which the load barely moves.
import { readFileSync, writeFileSync } from "node:fs";

export type Metrics = Record<string, number | string>;

export interface Step {
  name: string;
  about: string;
  /** Experiments answer one decision (linker, cache, profile) and run only when named in --steps. */
  experiment?: boolean;
  script: string;
}

/** Prelude shared by every step: `tm <key> <cmd...>` times a command and emits `<key>_wall/_cpu/_rss_mb/_rc`. */
const PRELUDE = String.raw`
set -u
cd "$FW_DIR"
FW_BASE=$(dirname "$(pwd)")
mkdir -p "$FW_BASE/tmp"
export CARGO_TARGET_DIR="$FW_BASE/target"
unset RUSTC_WRAPPER
export TMPDIR="$FW_BASE/tmp"
LOG=$(mktemp -d)
tm() {
  local key=$1; shift
  /usr/bin/time -o "$LOG/$key.t" -f "%e %U %S %M" "$@" >"$LOG/$key.log" 2>&1
  local rc=$?
  read -r wall user sys rss < <(tail -1 "$LOG/$key.t")
  printf "%s_rc=%s\n" "$key" "$rc"
  printf "%s_wall_s=%s\n" "$key" "$wall"
  printf "%s_cpu_s=%s\n" "$key" "$(awk -v u="$user" -v s="$sys" 'BEGIN{printf "%.1f", u+s}')"
  printf "%s_rss_mb=%s\n" "$key" "$((rss/1024))"
}

# bench_fw <key> <binary>: median wall ms of startup, a directory walk and a JSON query on a cargo metadata dump.
bench_fw() {
  local key=$1 bin=$2 spec name args
  [ -s "$LOG/meta.json" ] || cargo metadata --format-version 1 >"$LOG/meta.json" 2>/dev/null
  for spec in "version|--version" "tokei|tokei crates" "jq|jq .packages|length $LOG/meta.json"; do
    name=$(echo "$spec" | cut -d'|' -f1)
    args=$(echo "$spec" | cut -d'|' -f2-)
    hyperfine -N -w 5 -r 60 --export-json "$LOG/hf.json" "$bin $args" >/dev/null 2>&1
    printf "%s_%s_ms=%s\n" "$key" "$name" "$(bun -e 'const m=JSON.parse(require("fs").readFileSync(process.argv[1],"utf8")).results[0].measurements.map((x)=>x.time_wall_clock.value*1000).sort((p,q)=>p-q);console.log(m[m.length>>1].toFixed(2))' "$LOG/hf.json")"
  done
}
`;

const RELEASE = "cargo build --release -p fw-bin --bin fw";

export const STEPS: Step[] = [
  {
    name: "env",
    about: "host, load, toolchain, commit",
    script: String.raw`
echo "host=$(hostname)"
echo "load1=$(cut -d' ' -f1 /proc/loadavg)"
echo "cpus=$(nproc)"
echo "rustc=$(rustc --version | cut -d' ' -f2)"
echo "commit=$(git rev-parse --short HEAD)$(git diff --quiet HEAD -- . || echo '+dirty')"
`,
  },
  {
    name: "build",
    about: "clean release build of the fw binary, binary size",
    script: String.raw`
cargo clean -q 2>/dev/null
tm build ${RELEASE}
echo "binary_bytes=$(stat -c %s target/release/fw)"
`,
  },
  {
    name: "rebuild",
    about: "release rebuild after touching fw-bin",
    script: String.raw`
touch crates/fw-bin/src/main.rs
tm rebuild ${RELEASE}
`,
  },
  {
    name: "dev",
    about: "clean dev build, then dev rebuild after touching fw-bin",
    script: String.raw`
rm -rf target/debug
tm dev_build cargo build -p fw-bin --bin fw
touch crates/fw-bin/src/main.rs
tm dev_rebuild cargo build -p fw-bin --bin fw
`,
  },
  {
    name: "deps",
    about: "unique crate versions in the fw-bin and workspace graphs, duplicated crate names",
    script: String.raw`
echo "deps_fw_bin=$(cargo tree -p fw-bin -e normal,build --prefix none 2>/dev/null | grep -E '^[a-z0-9_-]+ v' | sed 's/ (.*//' | sort -u | wc -l)"
echo "deps_workspace=$(cargo tree --workspace -e normal,build,dev --prefix none 2>/dev/null | grep -E '^[a-z0-9_-]+ v' | sed 's/ (.*//' | sort -u | wc -l)"
echo "dup_names_workspace=$(cargo tree --workspace -e normal,build,dev -d --prefix depth 2>/dev/null | grep -E '^0[a-z0-9_-]+ v' | sed 's/^0//' | awk '{print $1}' | sort -u | wc -l)"
echo "dup_lanes_workspace=$(cargo tree --workspace -e normal,build,dev -d --prefix depth 2>/dev/null | grep -cE '^0[a-z0-9_-]+ v')"
echo "dup_names_fw_bin=$(cargo tree -p fw-bin -e normal,build -d --prefix depth 2>/dev/null | grep -E '^0[a-z0-9_-]+ v' | sed 's/^0//' | awk '{print $1}' | sort -u | wc -l)"
`,
  },
  {
    name: "test",
    about: "nextest: test build time, test run wall time, counts",
    script: String.raw`
tm test_build cargo nextest run --workspace --no-run
tm test_run cargo nextest run --workspace --no-fail-fast
summary=$(grep -E "^\s*Summary" "$LOG/test_run.log" | tail -1)
echo "test_summary=$(echo "$summary" | sed 's/\x1b\[[0-9;]*m//g' | tr -s ' ')"
`,
  },
  {
    name: "deny",
    about: "cargo deny findings per check",
    script: String.raw`
for c in advisories bans licenses sources; do
  cargo deny --workspace --format json check "$c" >"$LOG/deny_$c.json" 2>"$LOG/deny_$c.err"
  printf "deny_%s_errors=%s\n" "$c" "$(grep -c '"severity":"error"' "$LOG/deny_$c.err")"
  printf "deny_%s_warnings=%s\n" "$c" "$(grep -c '"severity":"warning"' "$LOG/deny_$c.err")"
done
`,
  },
  {
    name: "audit",
    about: "cargo audit: vulnerabilities and warnings",
    script: String.raw`
cargo audit --json >"$LOG/audit.json" 2>/dev/null
echo "audit_vulns=$(bun -e 'const j=JSON.parse(require("fs").readFileSync(process.argv[1],"utf8"));console.log(j.vulnerabilities.count)' "$LOG/audit.json" 2>/dev/null)"
echo "audit_warnings=$(bun -e 'const j=JSON.parse(require("fs").readFileSync(process.argv[1],"utf8"));console.log(Object.values(j.warnings).flat().length)' "$LOG/audit.json" 2>/dev/null)"
`,
  },
  {
    name: "unused",
    about: "machete, shear, udeps: unused dependencies",
    script: String.raw`
cargo machete >"$LOG/machete.out" 2>&1
echo "machete_unused=$(grep -cE '^\s+[a-zA-Z0-9_-]+$' "$LOG/machete.out")"
cargo shear >"$LOG/shear.out" 2>&1
echo "shear_unused=$(grep -cE '^\s+[a-zA-Z0-9_-]+( |$)' "$LOG/shear.out")"
tm udeps cargo +nightly udeps --workspace
echo "udeps_unused=$(grep -cE '^\s+"' "$LOG/udeps.log")"
cp "$LOG/machete.out" /tmp/fw-audit-machete.out; cp "$LOG/shear.out" /tmp/fw-audit-shear.out; cp "$LOG/udeps.log" /tmp/fw-audit-udeps.out
`,
  },
  {
    name: "bloat",
    about: "top crates by .text size and top llvm-lines (written to /tmp/fw-audit-*.txt on the host)",
    script: String.raw`
cargo bloat --release -p fw-bin --bin fw --crates -n 25 >/tmp/fw-audit-bloat.txt 2>&1
cargo llvm-lines --release -p fw-bin --bin fw 2>/dev/null | head -40 >/tmp/fw-audit-llvm-lines.txt
echo "bloat_top=$(sed -n '/File/,$p' /tmp/fw-audit-bloat.txt | sed -n '2,6p' | awk '{print $NF":"$(NF-2)}' | tr '\n' ' ')"
echo "llvm_lines_total=$(sed -n '3p' /tmp/fw-audit-llvm-lines.txt | awk '{print $1}')"
`,
  },
  {
    name: "startup",
    about: "hyperfine of the release fw binary: startup, a directory walk and a JSON query (median wall ms)",
    experiment: true,
    script: String.raw`
cargo metadata --format-version 1 >"$LOG/meta.json" 2>/dev/null
bench_fw startup target/release/fw
`,
  },
  {
    name: "linker",
    about: "dev build and fw-bin relink per linker (rust-lld default, mold, wild), 5 relinks each",
    experiment: true,
    script: String.raw`
TRIPLE=$(rustc -vV | sed -n 's/^host: //p')
printf '' >"$LOG/lld.toml"
printf '[target.%s]\nlinker = "clang"\nrustflags = ["-C", "link-arg=-fuse-ld=mold"]\n' "$TRIPLE" >"$LOG/mold.toml"
printf '[target.%s]\nlinker = "clang"\nrustflags = ["-C", "link-arg=--ld-path=wild"]\n' "$TRIPLE" >"$LOG/wild.toml"
for L in lld mold wild; do
  export CARGO_TARGET_DIR="$FW_BASE/tgt-link-$L"
  rm -rf "$CARGO_TARGET_DIR"
  tm "link_$L"_clean cargo --config "$LOG/$L.toml" build -p fw-bin --bin fw
  for i in 1 2 3 4 5; do
    touch crates/fw-bin/src/main.rs
    tm "link_$L"_r$i cargo --config "$LOG/$L.toml" build -p fw-bin --bin fw
  done
  printf "link_%s_relink_wall_mean_s=%s\n" "$L" "$(for i in 1 2 3 4 5; do tail -1 "$LOG/link_$L"_r$i.t | cut -d' ' -f1; done | awk '{s+=$1} END {printf "%.2f", s/NR}')"
  printf "link_%s_relink_cpu_mean_s=%s\n" "$L" "$(for i in 1 2 3 4 5; do tail -1 "$LOG/link_$L"_r$i.t | awk '{print $2+$3}'; done | awk '{s+=$1} END {printf "%.2f", s/NR}')"
  printf "link_%s_binary_bytes=%s\n" "$L" "$(stat -c %s "$CARGO_TARGET_DIR/debug/fw")"
  rm -rf "$CARGO_TARGET_DIR"
done
`,
  },
  {
    name: "sccache",
    about: "clean release build with a cold then a warm sccache (separate target dirs)",
    experiment: true,
    script: String.raw`
export SCCACHE_DIR="$LOG/sccache" RUSTC_WRAPPER=sccache
sccache --stop-server >/dev/null 2>&1; sccache --start-server >/dev/null 2>&1
for pass in cold warm; do
  export CARGO_TARGET_DIR="$FW_BASE/tgt-sccache-$pass"
  rm -rf "$CARGO_TARGET_DIR"
  tm "sccache_$pass" ${RELEASE}
  printf "sccache_%s_hits=%s\n" "$pass" "$(sccache --show-stats --stats-format json | bun -e 'const j=JSON.parse(await Bun.stdin.text());console.log(Object.values(j.stats.cache_hits.counts).reduce((a,b)=>a+b,0))')"
  rm -rf "$CARGO_TARGET_DIR"
done
sccache --stop-server >/dev/null 2>&1
`,
  },
  {
    name: "profile",
    about: "release size and build time per profile variant (opt-level s and z, thin LTO), clean target each",
    experiment: true,
    script: String.raw`
variant() {
  local key=$1; shift
  export CARGO_TARGET_DIR="$FW_BASE/tgt-prof-$key"
  rm -rf "$CARGO_TARGET_DIR"
  env "$@" /usr/bin/time -o "$LOG/$key.t" -f "%e %U %S %M" cargo build --release -p fw-bin --bin fw >"$LOG/$key.log" 2>&1
  read -r wall user sys rss < <(tail -1 "$LOG/$key.t")
  printf "prof_%s_wall_s=%s\nprof_%s_bytes=%s\n" "$key" "$wall" "$key" "$(stat -c %s "$CARGO_TARGET_DIR/release/fw")"
  bench_fw "prof_$key" "$CARGO_TARGET_DIR/release/fw"
  rm -rf "$CARGO_TARGET_DIR"
}
variant opt3 CARGO_PROFILE_RELEASE_OPT_LEVEL=3
variant opts CARGO_PROFILE_RELEASE_OPT_LEVEL=s
variant optz CARGO_PROFILE_RELEASE_OPT_LEVEL=z
variant thin CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
`,
  },
];

export function parseKv(out: string): Metrics {
  const m: Metrics = {};
  for (const line of out.split("\n")) {
    const i = line.indexOf("=");
    if (i < 1) continue;
    const v = line.slice(i + 1).trim();
    m[line.slice(0, i)] = v !== "" && !Number.isNaN(Number(v)) ? Number(v) : v;
  }
  return m;
}

async function runRemote(host: string, dir: string, script: string): Promise<string> {
  const body = `export FW_DIR=${dir}\n${PRELUDE}\n${script}`;
  const proc = Bun.spawn(host === "local" ? ["bash", "-s"] : ["ssh", host, "bash", "-s"], {
    stdin: new Blob([body]),
    stdout: "pipe",
    stderr: "inherit",
  });
  const out = await new Response(proc.stdout).text();
  await proc.exited;
  return out;
}

export function table(rows: [string, string, string?][]): string {
  const w = rows.map((r) => r.map((c) => (c ?? "").length));
  const cols = Math.max(...rows.map((r) => r.length));
  const width = Array.from({ length: cols }, (_, c) => Math.max(...w.map((r) => r[c] ?? 0)));
  return rows
    .map((r) => "| " + r.map((c, i) => (c ?? "").padEnd(width[i]!)).join(" | ") + " |")
    .join("\n");
}

const pct = (a: number, b: number) =>
  a === 0 ? "" : `${(((b - a) / a) * 100).toFixed(1)}%`;

export function compare(before: Metrics, after: Metrics): string {
  const keys = [...new Set([...Object.keys(before), ...Object.keys(after)])];
  const rows: [string, string, string?][] = [["metric", "before", "after", "delta"] as never];
  for (const k of keys) {
    const a = before[k];
    const b = after[k];
    if (typeof a === "number" && typeof b === "number")
      rows.push([k, String(a), String(b), `${b - a >= 0 ? "+" : ""}${+(b - a).toFixed(1)} (${pct(a, b)})`] as never);
    else rows.push([k, String(a ?? ""), String(b ?? ""), ""] as never);
  }
  return table(rows);
}

if (import.meta.main) {
  const args = Bun.argv.slice(2);
  const opt = (name: string, fallback: string) => {
    const i = args.indexOf(`--${name}`);
    return i >= 0 ? (args[i + 1] ?? fallback) : fallback;
  };
  if (args[0] === "--compare") {
    const [a, b] = [args[1]!, args[2]!].map((f) => JSON.parse(readFileSync(f, "utf8")) as Metrics);
    console.log(compare(a!, b!));
    process.exit(0);
  }
  const host = opt("host", "fr1");
  const dir = opt("dir", "/srv/xbuild/fw-opt/fw");
  const wanted = opt("steps", STEPS.filter((s) => !s.experiment).map((s) => s.name).join(",")).split(",");
  const metrics: Metrics = { label: opt("label", "run") };
  const outFile = opt("out", "");
  for (const step of STEPS.filter((s) => wanted.includes(s.name))) {
    const started = Date.now();
    console.error(`== ${step.name}: ${step.about}`);
    Object.assign(metrics, parseKv(await runRemote(host, dir, step.script)));
    if (outFile) writeFileSync(outFile, JSON.stringify(metrics, null, 2) + "\n");
    console.error(`   done in ${((Date.now() - started) / 1000).toFixed(0)}s`);
  }
  console.log(table([["metric", "value"], ...Object.entries(metrics).map(([k, v]) => [k, String(v)] as [string, string])]));
}
