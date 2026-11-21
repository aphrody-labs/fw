#!/usr/bin/env bun
/**
 * Reproducible direct Rust competitor benchmark for `fw` (zero external deps).
 * Fetches live GitHub metadata for direct Rust peers, reads local `Cargo.toml`
 * metrics from this workspace, and emits:
 *   - docs/research/rust-competitors-matrix.tsv
 *   - docs/research/rust-competitors-matrix-2026-11-19.md
 */

import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const ROOT = resolve(import.meta.dir, "..");
const OUT_DIR = join(ROOT, "docs", "research");
const TSV_PATH = join(OUT_DIR, "rust-competitors-matrix.tsv");
const MD_PATH = join(OUT_DIR, "rust-competitors-matrix-2026-11-19.md");

interface CompetitorSpec {
  slug: string;
  label: string;
  role: string;
  licenseOverride?: string;
  forbidUnsafeNote: string;
}

interface RepoMetrics {
  slug: string;
  label: string;
  role: string;
  stars: number | null;
  pushedAt: string;
  license: string;
  topDirs: string[];
  cratesCount: number;
  workspaceMembers: number;
  rootCargoDeps: number;
  rootTotalDeps: number;
  forbidUnsafe: string;
}

const COMPETITORS: CompetitorSpec[] = [
  {
    slug: "zeroclaw-labs/zeroclaw",
    label: "ZeroClaw",
    role: "Autonomous Rust agent runtime & multi-channel daemon",
    forbidUnsafeNote: "partial (FFI/sys crates)",
  },
  {
    slug: "RightNow-AI/openfang",
    label: "OpenFang",
    role: "Rust agent OS & modular skill/tool runtime",
    forbidUnsafeNote: "partial (runtime crates)",
  },
  {
    slug: "nearai/ironclaw",
    label: "IronClaw",
    role: "Rust TEE/Wasm-sandboxed agent & multi-protocol hub",
    forbidUnsafeNote: "partial (Wasmtime/crypto)",
  },
  {
    slug: "zed-industries/zed",
    label: "Zed",
    role: "GPU-accelerated Rust editor (GPUI) & ACP agent host",
    licenseOverride: "GPL-3.0 / Apache-2.0",
    forbidUnsafeNote: "unsafe in GPU/platform FFI",
  },
  {
    slug: "asterinas/asterinas",
    label: "Asterinas",
    role: "Rust Framekernel OS (OSTD + OSDK Linux ABI)",
    forbidUnsafeNote: "OSTD isolates unsafe; services safe",
  },
];

function getGitHubHeaders(): Record<string, string> {
  let token = process.env.GITHUB_TOKEN ?? process.env.GH_TOKEN ?? "";
  if (!token) {
    const res = Bun.spawnSync(["gh", "auth", "token"], { stdout: "pipe", stderr: "pipe" });
    if (res.exitCode === 0) token = res.stdout.toString().trim();
  }
  const headers: Record<string, string> = {
    "User-Agent": "fw-competitor-audit/4.2",
    Accept: "application/vnd.github+json",
  };
  if (token) headers.Authorization = `Bearer ${token}`;
  return headers;
}

async function ghJson<T>(path: string, headers: Record<string, string>): Promise<T> {
  const res = await fetch(`https://api.github.com${path}`, { headers });
  if (!res.ok) {
    throw new Error(`GitHub API ${path} failed (${res.status} ${res.statusText})`);
  }
  return (await res.json()) as T;
}

interface ParsedCargo {
  workspaceMembers: number;
  rootExternalDeps: number;
  rootTotalDeps: number;
  license: string | null;
}

function parseCargoToml(raw: string, cratesDirCount: number): ParsedCargo {
  const parsed = Bun.TOML.parse(raw) as Record<string, unknown>;
  const ws = (parsed.workspace ?? {}) as Record<string, unknown>;
  const rawMembers = Array.isArray(ws.members) ? (ws.members as string[]) : [];
  const rawExclude = Array.isArray(ws.exclude) ? (ws.exclude as string[]) : [];

  let workspaceMembers = 0;
  if (rawMembers.length === 0) {
    workspaceMembers = 1;
  } else {
    for (const m of rawMembers) {
      if (m === "crates/*" && cratesDirCount > 0) {
        workspaceMembers += cratesDirCount;
      } else {
        workspaceMembers += 1;
      }
    }
    if (rawMembers.includes("crates/*")) {
      const excludedInCrates = rawExclude.filter((e) => e.startsWith("crates/")).length;
      workspaceMembers = Math.max(1, workspaceMembers - excludedInCrates);
    }
  }

  const targetObj = (parsed.target ?? {}) as Record<string, Record<string, unknown>>;
  const depSections: unknown[] = [
    ws.dependencies,
    parsed.dependencies,
    parsed["dev-dependencies"],
    parsed["build-dependencies"],
    ...Object.values(targetObj).flatMap((t) => [
      t?.dependencies,
      t?.["dev-dependencies"],
      t?.["build-dependencies"],
    ]),
  ];

  const externalNames = new Set<string>();
  const totalNames = new Set<string>();
  for (const sec of depSections) {
    if (!sec || typeof sec !== "object") continue;
    for (const [name, spec] of Object.entries(sec as Record<string, unknown>)) {
      totalNames.add(name);
      const isLocalPath = typeof spec === "object" && spec !== null && "path" in spec;
      if (!isLocalPath) externalNames.add(name);
    }
  }

  const wsPkg = (ws.package ?? {}) as Record<string, unknown>;
  const pkg = (parsed.package ?? {}) as Record<string, unknown>;
  const license =
    typeof wsPkg.license === "string"
      ? wsPkg.license
      : typeof pkg.license === "string"
        ? pkg.license
        : null;

  return {
    workspaceMembers,
    rootExternalDeps: externalNames.size,
    rootTotalDeps: totalNames.size,
    license,
  };
}

async function auditCompetitor(
  spec: CompetitorSpec,
  headers: Record<string, string>,
): Promise<RepoMetrics> {
  const [repo, contents, cargoRes] = await Promise.all([
    ghJson<{
      stargazers_count: number;
      pushed_at: string;
      license: { spdx_id?: string } | null;
    }>(`/repos/${spec.slug}`, headers),
    ghJson<Array<{ name: string; type: string }>>(`/repos/${spec.slug}/contents`, headers),
    ghJson<{ content?: string }>(`/repos/${spec.slug}/contents/Cargo.toml`, headers),
  ]);

  const topDirs = contents
    .filter((entry) => entry.type === "dir" && !entry.name.startsWith("."))
    .map((entry) => entry.name)
    .sort();

  let cratesCount = 0;
  if (topDirs.includes("crates")) {
    const cratesEntries = await ghJson<Array<{ name: string; type: string }>>(
      `/repos/${spec.slug}/contents/crates`,
      headers,
    );
    cratesCount = cratesEntries.filter((e) => e.type === "dir").length;
  }

  const cargoRaw = cargoRes.content ? Buffer.from(cargoRes.content, "base64").toString("utf8") : "";
  const parsed = parseCargoToml(cargoRaw, cratesCount);
  if (cratesCount === 0) cratesCount = parsed.workspaceMembers;

  const spdx = repo.license?.spdx_id;
  const license =
    spec.licenseOverride ??
    (spdx && spdx !== "NOASSERTION" ? spdx : parsed.license ?? "Unknown");

  return {
    slug: spec.slug,
    label: spec.label,
    role: spec.role,
    stars: repo.stargazers_count,
    pushedAt: repo.pushed_at,
    license,
    topDirs,
    cratesCount,
    workspaceMembers: parsed.workspaceMembers,
    rootCargoDeps: parsed.rootExternalDeps,
    rootTotalDeps: parsed.rootTotalDeps,
    forbidUnsafe: spec.forbidUnsafeNote,
  };
}

function auditLocalFw(): RepoMetrics {
  const cargoRaw = readFileSync(join(ROOT, "Cargo.toml"), "utf8");
  const cratesDir = join(ROOT, "crates");
  const crateNames = readdirSync(cratesDir)
    .filter((name) => statSync(join(cratesDir, name)).isDirectory())
    .sort();

  let forbidCount = 0;
  for (const crateName of crateNames) {
    const libPath = join(cratesDir, crateName, "src", "lib.rs");
    const mainPath = join(cratesDir, crateName, "src", "main.rs");
    const src = existsSync(libPath)
      ? readFileSync(libPath, "utf8")
      : existsSync(mainPath)
        ? readFileSync(mainPath, "utf8")
        : "";
    if (src.includes("#![forbid(unsafe_code)]")) forbidCount++;
  }

  const ignoredTop = new Set(["target", "dist", "node_modules", "packages"]);
  const topDirs = readdirSync(ROOT)
    .filter((name) => !name.startsWith(".") && !ignoredTop.has(name))
    .filter((name) => statSync(join(ROOT, name)).isDirectory())
    .sort();

  const parsed = parseCargoToml(cargoRaw, crateNames.length);
  const gitDate = Bun.spawnSync(["git", "-C", ROOT, "log", "-1", "--format=%cI"], {
    stdout: "pipe",
    stderr: "pipe",
  })
    .stdout.toString()
    .trim();

  return {
    slug: "aphrody-labs/fw",
    label: "fw",
    role: "Zero-bloat Rust OS, WebGPU/Wasm, Cluster & Agent runtime",
    stars: null,
    pushedAt: gitDate || new Date().toISOString(),
    license: parsed.license ?? "MIT",
    topDirs,
    cratesCount: crateNames.length,
    workspaceMembers: parsed.workspaceMembers,
    rootCargoDeps: parsed.rootExternalDeps,
    rootTotalDeps: parsed.rootTotalDeps,
    forbidUnsafe: `${forbidCount}/${crateNames.length} #![forbid(unsafe_code)]`,
  };
}

function fmtNum(n: number | null): string {
  return n === null ? "private" : n.toLocaleString("en-US");
}

function renderTsv(rows: RepoMetrics[]): string {
  const header = [
    "repo",
    "label",
    "stars",
    "pushed_at",
    "license",
    "workspace_members",
    "crates_count",
    "root_external_deps",
    "root_total_deps",
    "forbid_unsafe",
    "top_dirs",
  ].join("\t");

  const body = rows.map((r) =>
    [
      r.slug,
      r.label,
      r.stars === null ? "private" : String(r.stars),
      r.pushedAt,
      r.license,
      String(r.workspaceMembers),
      String(r.cratesCount),
      String(r.rootCargoDeps),
      String(r.rootTotalDeps),
      r.forbidUnsafe,
      r.topDirs.join(","),
    ].join("\t"),
  );

  return `${[header, ...body].join("\n")}\n`;
}

function renderMarkdown(rows: RepoMetrics[], generatedAt: string): string {
  const fw = rows[0]!;
  const bySlug = new Map(rows.map((r) => [r.slug, r]));
  const zc = bySlug.get("zeroclaw-labs/zeroclaw")!;
  const of = bySlug.get("RightNow-AI/openfang")!;
  const ic = bySlug.get("nearai/ironclaw")!;
  const zed = bySlug.get("zed-industries/zed")!;
  const ast = bySlug.get("asterinas/asterinas")!;

  const summaryRows = rows
    .map(
      (r) =>
        `| **${r.label}** (\`${r.slug}\`) | ${fmtNum(r.stars)} | \`${r.pushedAt.slice(0, 10)}\` | ${r.license} | ${r.workspaceMembers} | ${r.cratesCount} | **${r.rootCargoDeps}** (${r.rootTotalDeps} total) | ${r.forbidUnsafe} |`,
    )
    .join("\n");

  const treeRows = rows
    .map(
      (r) =>
        `| **${r.label}** | ${r.topDirs.length} | \`${r.topDirs.slice(0, 10).join("`, `")}${r.topDirs.length > 10 ? "`, ..." : "`"} |`,
    )
    .join("\n");

  const peerExtMin = Math.min(
    zc.rootCargoDeps,
    of.rootCargoDeps,
    ic.rootCargoDeps,
    zed.rootCargoDeps,
    ast.rootCargoDeps,
  );
  const peerExtMax = Math.max(
    zc.rootCargoDeps,
    of.rootCargoDeps,
    ic.rootCargoDeps,
    zed.rootCargoDeps,
    ast.rootCargoDeps,
  );

  return `# Direct Rust Competitors Benchmark: \`fw\` vs Peers (2026-11-19)

Generated reproducibly by \`bun tools/competitor_audit.ts\` (live GitHub API + local \`Cargo.toml\` parsed via \`Bun.TOML.parse\`, timestamp \`${generatedAt}\`). Machine-readable matrix: [\`rust-competitors-matrix.tsv\`](./rust-competitors-matrix.tsv).

## 1. Synthèse quantitative

| Projet | Stars | Dernier push | Licence | Workspace Members | Crates | Root Deps (externes) | Sécurité mémoire |
|---|---:|---|---|---:|---:|---:|---|
${summaryRows}

**Lecture directe :** \`fw\` couvre OS, Wasm, WebGPU, cluster et agent MCP/Claw dans **${fw.workspaceMembers} crates** avec **${fw.rootCargoDeps} dépendances externes** (\`serde\`, \`serde_json\`, \`sha2\`, \`tokio\`, \`flate2\`) et **${fw.forbidUnsafe}**. Les cinq pairs analysés déclarent entre **${peerExtMin}** et **${peerExtMax}** dépendances externes directes dans leur \`Cargo.toml\` racine.

## 2. Arborescence

| Projet | Dossiers racine | Organisation top-level |
|---|---:|---|
${treeRows}

- **\`fw\`** : monorepo plat à couches séparées : \`crates/fw-*\` (${fw.cratesCount} crates Rust purs), \`os/\` (Asterinas OSDK + WSL), \`nix/\` (fleet NixOS + \`flake.nix\`), \`cloudflare/\` + \`vercel/\` (edge), \`web/\` (control-plane embarqué), \`plugin/\` (10 skills \`agentskills.io\`), \`tools/\` (audit concurrentiel) et \`xtask/\` (CI parallèle Rust pure). Un index \`MAP.tsv\` (< 600 tokens) évite aux agents de scanner l'arbre.
- **ZeroClaw** (${zc.workspaceMembers} members) : découpage dense par domaine agent (canaux, mémoire, providers, outils, sécurité, TUI/web) avec \`crates/\` et dossiers auxiliaires (\`docs\`, \`scripts\`, \`web\`, \`deploy\`).
- **OpenFang** (${of.workspaceMembers} members) : structure compacte centrée sur \`crates/\` (runtime agent, mémoire, skills, CLI, gateway).
- **IronClaw** (${ic.workspaceMembers} members) : workspace étendu autour de \`crates/\`, \`channels-src/\` et \`tools-src/\` pour isoler les connecteurs Wasm et les politiques TEE.
- **Zed** (${zed.workspaceMembers} members) : monorepo centré sur \`crates/\` (GPUI, langage/LSP, collaboration, ACP/agent, extensions WIT) et \`tooling/\`.
- **Asterinas** (${ast.workspaceMembers} members) : séparation stricte entre \`ostd/\` (couche TCB minimale), \`kernel/\` (services Linux-compatibles en Rust sûr) et \`osdk/\` (outillage de build/test QEMU).

## 3. Architecture

| Projet | Modèle d'exécution | Isolation & sûreté | Cibles compilées |
|---|---|---|---|
| **\`fw\`** | Binaire Fat-LTO \`fw-bin\` + contrôle-plan HTTP/WS/MCP/GraphQL + module \`fw-wasm\` | ${fw.forbidUnsafe} (seul \`fw-wasm\` expose l'ABI C/Wasm) | Windows MSVC, Linux \`x86_64-unknown-linux-musl\`, \`wasm32-unknown-unknown\`, Asterinas OSDK |
| **ZeroClaw** | Daemon Tokio multi-canaux (Telegram/Discord/Slack/HTTP/CLI) | Sandboxing OS/politiques outil + crates système | Linux, macOS, Windows |
| **OpenFang** | Runtime agent Rust + boucle autonome & registre de skills | Permissions par outil et sandboxing de commandes | Linux, macOS, Windows |
| **IronClaw** | Hôte Rust + plugins Wasm (Wasmtime) + attestation TEE (NEAR AI) | Isolation mémoire par bac à sable Wasm + enclaves | Linux (TEE/cloud), macOS, Wasm32 |
| **Zed** | Application desktop GPU native (GPUI) + serveur collab + hôte ACP | \`unsafe\` confiné aux bindings Metal/Vulkan/DirectX/Win32/Cocoa | macOS, Linux, Windows |
| **Asterinas** | Noyau Framekernel (OSTD = petite TCB, services noyau en Rust sûr) | Séparation formelle TCB (\`ostd\`) / reste du noyau (\`#![deny(unsafe_code)]\`) | Bare-metal / QEMU \`x86_64\`, \`riscv64\` |

## 4. Dépendances

- **\`fw\`** : **${fw.rootCargoDeps} dépendances externes** (\`serde\`, \`serde_json\`, \`sha2\`, \`tokio\`, \`flate2\`), zéro dépendance C/OpenSSL/libgit2 dans le workspace principal. Compilation incrémentale courte, binaire statique musl Alpine sans glibc, surface d'audit supply-chain réduite.
- **OpenFang** : ${of.rootCargoDeps} dépendances externes racine (${of.rootTotalDeps} au total). Runtime agent généraliste compact sur stack Tokio/Reqwest/SQLite.
- **Asterinas** : ${ast.rootCargoDeps} dépendances externes racine (${ast.rootTotalDeps} au total). Crates \`no_std\` noyau, allocateurs, virtio, ACPI et pilotes bas niveau.
- **IronClaw** : ${ic.rootCargoDeps} dépendances externes racine (${ic.rootTotalDeps} au total). Volume porté par Wasmtime, crypto TEE, bases de données et SDK providers.
- **ZeroClaw** : ${zc.rootCargoDeps} dépendances externes racine (${zc.rootTotalDeps} au total). Volume porté par les connecteurs messagerie, clients LLM, stockage vectoriel et TUI.
- **Zed** : ${zed.rootCargoDeps} dépendances externes racine (${zed.rootTotalDeps} au total). Volume lié à un IDE complet (tree-sitter, wgpu/blade, font-kit, webrtc, lsp-types, git2/gix, protobuf).

## 5. Feature Matrix

| Capacité | \`fw\` | ZeroClaw | OpenFang | IronClaw | Zed | Asterinas |
|---|:---:|:---:|:---:|:---:|:---:|:---:|
| **OS / Userland** | Oui (\`fw-uutils\`, Alpine musl, NixOS) | Non | Non | Non | Non | Oui (Linux ABI) |
| **Kernel** | Oui (\`os/fw-os\` Asterinas OSDK + \`fw-ostd-wasm\`) | Non | Non | Non | Non | Oui (Framekernel OSTD) |
| **Wasm** | Oui (\`fw-wasm\` \`wasm32-unknown-unknown\`) | Partiel | Partiel | Oui (Wasmtime plugins) | Oui (extensions WIT) | Non |
| **WebGPU** | Oui (\`fw-wayland-wgpu\`, WGSL, probe web) | Non | Non | Non | Oui (GPUI Metal/Vulkan/DX12) | Non |
| **Cluster / Routing** | Oui (\`fw-cluster\` Pingora/Dynamo/vLLM-Router) | Non | Non | Partiel (TEE nodes) | Partiel (collab server) | Non |
| **MCP (JSON-RPC 2.0)** | Oui (\`fw-hermes-claw\` + \`POST /mcp\`) | Oui | Oui | Oui | Oui (client MCP) | Non |
| **ACP (Agent Client Protocol)** | Partiel (planifié) | Partiel | Non | Partiel | Oui (natif) | Non |
| **A2A (Agent-to-Agent)** | Partiel (spec \`fw-cluster\`) | Partiel | Oui | Oui | Non | Non |
| **Cloudflare Edge** | Oui (\`cloudflare/\` Workers + SQLite DO) | Non | Non | Non | Non | Non |
| **Nix** | Oui (\`flake.nix\` flake-parts + crane + NixOS) | Partiel | Non | Non | Oui (\`nix/\` flake-parts + crane) | Partiel (dev env) |
| **WSL** | Oui (\`os/fw-wsl\` tuning WSL + GPU) | Non | Non | Non | Oui (WSL remote) | Non |
| **CLI-to-Skill** | Oui (\`fw-hermes-claw\` \`agentskills.io\` gen) | Partiel (skills MD) | Oui (skill registry) | Partiel (Wasm tools) | Oui (skills / rules) | Non |

## 6. Comparaison UI/UX (Web / GPUI / Blitz / TUI)

| Surface UI | \`fw\` | ZeroClaw / OpenFang / IronClaw | Zed | Asterinas |
|---|---|---|---|---|
| **Web** | Dashboard embarqué zéro-build (\`web/index.html\` servi par \`fw-proto\` sur \`/\`, câblé sur \`/health\`, \`/cluster\`, \`/claw/*\`, \`/mcp\`, WebGPU probe) | Web UI optionnelle (SPA JS/TS séparée ou gateway HTTP) | Non (desktop natif) | Non |
| **GPUI** | Modèle scène/couches \`fw-ostd-wasm\` + \`fw-wayland-wgpu\` | Non | Référence GPU desktop (60-120 FPS, rendu natif, Taffy layout) | Non |
| **Blitz** | Moteur déclaratif HTML/CSS-vers-GPU (\`fw-ui-blitz\`) sans dépendance Chromium | Non | Non | Non |
| **TUI** | \`tui-test\` + \`wta\` (contrôle PTY, snapshots, locators) | TUI Ratatui / CLI interactive centrée chat & logs | Terminal intégré GPUI | Console série / QEMU |

**Constat UI/UX :** \`fw\` combine un panneau Web embarqué instantané (zéro bundler au runtime) avec une base graphique Rust (\`fw-ui-blitz\` + \`fw-wayland-wgpu\`) et une suite de test terminal (\`tui-test\`). Zed domine le rendu GPU desktop (GPUI), tandis que ZeroClaw, OpenFang et IronClaw concentrent leurs efforts sur la CLI/TUI et les canaux messagerie.

## 7. Pistes d'action concrètes pour \`fw\`

1. **Verrouiller le budget dépendances (<= 10 crates externes racine)** : l'écart de ${fw.rootCargoDeps} vs ${peerExtMin}-${peerExtMax} dépendances externes est le principal avantage structurel de \`fw\` (temps de build, binaires statiques musl, audit sécurité). Toute nouvelle fonctionnalité doit rester en Rust pur ou isolée hors chemin critique.
2. **Compléter la parité filaire ACP + A2A dans \`fw-proto\` et \`fw-hermes-claw\`** : Zed impose ACP côté IDE et IronClaw/OpenFang poussent A2A côté agents distribués. Exposer des handlers JSON-RPC ACP et A2A dans \`fw-proto\` (à côté de \`POST /mcp\`) aligne \`fw\` sur les trois standards sans dépendance externe supplémentaire.
3. **Renforcer le bac à sable Wasm de \`fw-wasm\` pour les skills tiers** : reprendre le modèle d'isolation par capacités d'IronClaw et des extensions WIT de Zed pour exécuter les skills générés par \`CLI-to-Skill\` avec permissions explicites (FS, réseau, env).
4. **Ajouter une vérification visuelle headless sur \`fw-ui-blitz\` et \`fw-wayland-wgpu\`** : produire des captures framebuffer/PNG déterministes en test pour rapprocher la rigueur visuelle de GPUI sans embarquer Chromium.
5. **Maintenir la veille reproductible via \`just competitors\`** : exécuter \`bun tools/competitor_audit.ts\` périodiquement pour suivre l'évolution des étoiles, licences, tailles de workspace et dérives de dépendances des cinq pairs dans \`docs/research/rust-competitors-matrix.tsv\`.
`;
}

async function main(): Promise<void> {
  const headers = getGitHubHeaders();
  const local = auditLocalFw();
  const peers = await Promise.all(COMPETITORS.map((spec) => auditCompetitor(spec, headers)));
  const rows: RepoMetrics[] = [local, ...peers];
  const generatedAt = new Date().toISOString();

  mkdirSync(OUT_DIR, { recursive: true });
  writeFileSync(TSV_PATH, renderTsv(rows), "utf8");
  writeFileSync(MD_PATH, renderMarkdown(rows, generatedAt), "utf8");

  console.log(
    JSON.stringify(
      {
        ok: true,
        generatedAt,
        tsvPath: TSV_PATH,
        mdPath: MD_PATH,
        repos: rows.map((r) => ({
          repo: r.slug,
          stars: r.stars,
          pushedAt: r.pushedAt,
          license: r.license,
          workspaceMembers: r.workspaceMembers,
          cratesCount: r.cratesCount,
          rootExternalDeps: r.rootCargoDeps,
          rootTotalDeps: r.rootTotalDeps,
        })),
      },
      null,
      2,
    ),
  );
}

await main();
