#!/usr/bin/env bun
// Runs builds and tests on the build host against its single checkout, with git as the only transport.
//
//   bun tools/fr1.ts gate -p fw-drive -p fw-cluster [--features fw-cluster/kube]   clippy -D warnings + tests
//   bun tools/fr1.ts run -- cargo build --release -p fw-bin                         any command in the checkout
//
// Steps: push this checkout (tools/sync-tree.ts merges upstream first, without stash or reset), then on the
// host bring its checkout of the same repository up to date the same way, then run. No tar copies, no scratch
// clones: one checkout per repository per machine, one shared target directory, sccache when the host has it.
// The host comes from $FW_BUILD_HOST (default fr1, an alias in ~/.ssh/config); the checkout is ~/src/<repo>.

import { $ } from "bun";
import { basename, resolve } from "node:path";

const host = process.env.FW_BUILD_HOST ?? "fr1";
const repoDir = resolve(import.meta.dir, "..");
const repo = basename(repoDir);
const [cmd, ...rest] = process.argv.slice(2);

function usage(): never {
  console.error("usage: bun tools/fr1.ts gate -p <crate> [-p <crate>…] [--features <f>] | run -- <command…>");
  process.exit(2);
}

const sync = await $`bun ${resolve(import.meta.dir, "sync-tree.ts")} ${repoDir}`.quiet().nothrow();
const local = JSON.parse(sync.stdout.toString().trim().split("\n").pop() ?? "{}");
if (!local.ok) {
  console.error(`local sync failed: ${local.note ?? sync.stderr.toString()}`);
  process.exit(1);
}
const dirty = (await $`git -C ${repoDir} status --porcelain --untracked-files=no`.quiet().text()).trim();
if (dirty) console.error(`note: uncommitted local changes are not on ${host}:\n${dirty.split("\n").slice(0, 10).join("\n")}`);

let remote: string;
if (cmd === "gate") {
  const crates: string[] = [];
  let features = "";
  for (let i = 0; i < rest.length; i++) {
    if (rest[i] === "-p") crates.push(rest[++i]);
    else if (rest[i] === "--features") features = rest[++i];
  }
  if (!crates.length) usage();
  const pkgs = crates.map((c) => `-p ${c}`).join(" ");
  const feat = features ? `--features ${features}` : "";
  remote = `cargo clippy --locked ${pkgs} ${feat} --all-targets -- -D warnings && cargo test --locked ${pkgs} ${feat}`;
} else if (cmd === "run") {
  const i = rest.indexOf("--");
  const argv = i >= 0 ? rest.slice(i + 1) : rest;
  if (!argv.length) usage();
  remote = argv.map((a) => (/^[\w./:=@%+,-]+$/.test(a) ? a : `'${a.replace(/'/g, "'\\''")}'`)).join(" ");
} else usage();

const script = [
  "set -e",
  "source ~/.cargo/env 2>/dev/null || true",
  `cd ~/src/${repo}`,
  // The sync script itself comes from upstream, so a checkout that is far behind still syncs.
  `git fetch -q && git show @{u}:tools/sync-tree.ts > /tmp/sync-tree-$$.ts`,
  `res=$(bun /tmp/sync-tree-$$.ts . | tail -1); rm -f /tmp/sync-tree-$$.ts; echo "$res"`,
  `case "$res" in *'"ok":true'*) ;; *) echo "remote sync failed" >&2; exit 1;; esac`,
  `git log --oneline -1`,
  "command -v sccache >/dev/null && export RUSTC_WRAPPER=${RUSTC_WRAPPER:-$(command -v sccache)}",
  `nice -n 5 ${remote}`,
].join("\n");

const proc = Bun.spawn(["ssh", "-o", "BatchMode=yes", host, "bash", "-s"], { stdin: new TextEncoder().encode(script), stdout: "inherit", stderr: "inherit" });
process.exit(await proc.exited);
