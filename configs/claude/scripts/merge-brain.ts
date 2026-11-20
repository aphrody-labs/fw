import { Database } from "bun:sqlite";
import {
  existsSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, join } from "node:path";
import { discoverSrcRoot, HOME, REPO_ROOT } from "./lib.ts";

const MERGED_LEGACY_FILES = new Set([
  "agent-report-vs-request.md",
  "agents-block-causes.md",
  "ai-os-google-self-hosted.md",
  "aphrody-checkout-relocated.md",
  "aphrody-ui.md",
  "artcraft-storytold-absorption.md",
  "assemble-not-reinvent.md",
  "auto-saas-of-open.md",
  "blanket-consent.md",
  "budget-sheet.md",
  "code-before-docs.md",
  "definition-of-done.md",
  "english-source-i18n.md",
  "first-party-audit-approved.md",
  "fix-at-source-continuous-refactor.md",
  "fix-it-yourself.md",
  "fleet-roles.md",
  "fleet-ssh-traps.md",
  "gemini-review-file-upload.md",
  "generalist-no-fixed-picks.md",
  "google-quality-criteria.md",
  "language-by-domain.md",
  "licenses-go-to-human.md",
  "logo-like-bun.md",
  "material-ui-stack.md",
  "minimal-input-goal.md",
  "naming-ay-cli.md",
  "never-stash-shared-tree.md",
  "no-blocking-gates.md",
  "no-legal-calls.md",
  "no-machine-excuses.md",
  "no-pauses.md",
  "no-private-data-in-source.md",
  "no-tool-dogma.md",
  "open-repo-ownership.md",
  "open-session-lessons.md",
  "orgs-labs-vs-dev.md",
  "os-family-ay-aynt-aydroid.md",
  "owner-direction-2026-11-15.md",
  "owner-low-vision.md",
  "owner-standing-directives-2026-11-13.md",
  "plan-design-before-build.md",
  "product-split-vitrine-launcher.md",
  "product-split.md",
  "proof-not-http-200.md",
  "relay-down-fallbacks.md",
  "requirements-ledger.md",
  "reuse-one-year-of-work.md",
  "reuse-research-purge-perf.md",
  "rust-crate-docs-tools.md",
  "sandbox-doc-examples-outside-projects.md",
  "ship-complete-latest-roadmap.md",
  "subagent-models-cheap.md",
  "submodule-sibling-checkouts.md",
  "vercel-blocked.md",
  "windows-paths-in-regex-tools.md",
]);

interface ParsedMemory {
  project: string;
  fileName: string;
  name: string;
  description: string;
  type: string;
  body: string;
}

export function parseFrontmatter(
  raw: string,
  fallbackName: string,
): Omit<ParsedMemory, "project" | "fileName"> {
  const m = raw.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/);
  if (!m)
    return {
      name: fallbackName,
      description: "",
      type: "project",
      body: raw.trim(),
    };
  const fm = m[1] ?? "";
  const body = (m[2] ?? "").trim();
  const field = (k: string) => {
    const r = fm.match(new RegExp(`^${k}:\\s*(.+)$`, "m"));
    return r?.[1]?.trim().replace(/^["']|["']$/g, "") ?? "";
  };
  return {
    name: field("name") || fallbackName,
    description: field("description"),
    type: field("type") || "project",
    body,
  };
}

function collectMemories(): ParsedMemory[] {
  const projRoot = join(REPO_ROOT, "projects");
  if (!existsSync(projRoot)) return [];
  const items: ParsedMemory[] = [];
  for (const proj of readdirSync(projRoot)) {
    const memDir = join(projRoot, proj, "memory");
    if (!existsSync(memDir) || !statSync(memDir).isDirectory()) continue;
    for (const f of readdirSync(memDir)) {
      if (!f.endsWith(".md") || f === "MEMORY.md") continue;
      const raw = readFileSync(join(memDir, f), "utf8");
      const parsed = parseFrontmatter(raw, f.replace(/\.md$/, ""));
      items.push({ project: proj, fileName: f, ...parsed });
    }
  }
  return items;
}

function syncSqliteStores(memories: ParsedMemory[]): void {
  const nowMs = Date.now();
  const nowSec = Math.floor(nowMs / 1000);

  const agentDbPath = join(HOME, ".agent", "memory.sqlite");
  if (existsSync(agentDbPath)) {
    const db = new Database(agentDbPath);
    db.exec("BEGIN");
    try {
      db.query(
        "DELETE FROM entries WHERE source = 'memory' AND id LIKE 'md:%'",
      ).run();
      const ins = db.query(
        "INSERT INTO entries(id, source, scope, title, text, symbols, at) VALUES (?, 'memory', 'user', ?, ?, '', ?)",
      );
      for (const m of memories) {
        const text = m.description ? `${m.description}\n\n${m.body}` : m.body;
        ins.run(`md:${m.project}:${m.fileName}`, m.name, text, nowMs);
      }
      db.exec("COMMIT");
    } catch (e) {
      db.exec("ROLLBACK");
      throw e;
    } finally {
      db.close();
    }
  }

  const bunDbPath = join(HOME, ".bun", "agent", "memory.db");
  if (existsSync(bunDbPath)) {
    const db = new Database(bunDbPath);
    db.exec("BEGIN");
    try {
      db.query("DELETE FROM memories WHERE source = 'claude-memory'").run();
      const ins = db.query(
        "INSERT OR REPLACE INTO memories(project, name, description, type, body, source, updated) VALUES (?, ?, ?, ?, ?, 'claude-memory', ?)",
      );
      for (const m of memories) {
        ins.run(m.project, m.name, m.description, m.type, m.body, nowSec);
      }
      db.exec("COMMIT");
    } catch (e) {
      db.exec("ROLLBACK");
      throw e;
    } finally {
      db.close();
    }
  }
}

function cleanupLegacyAndClutter(): void {
  // 1. Prune consolidated legacy memory files from C--src
  const memDir = join(REPO_ROOT, "projects", "C--src", "memory");
  if (existsSync(memDir)) {
    for (const f of readdirSync(memDir)) {
      if (MERGED_LEGACY_FILES.has(f)) rmSync(join(memDir, f), { force: true });
    }
  }

  // 2. Remove Temp test Claude project
  const tempProj = join(
    REPO_ROOT,
    "projects",
    "C--Users-aphro-AppData-Local-Temp",
  );
  if (existsSync(tempProj)) rmSync(tempProj, { recursive: true, force: true });

  // 3. Clean stale backups, plans, and old test orch runs
  for (const f of readdirSync(HOME)) {
    if (/^\.(claude\.json|bashrc|zshrc).*\.(bak|tmp)/.test(f)) {
      rmSync(join(HOME, f), { force: true });
    }
  }
  for (const dir of [
    join(REPO_ROOT, "backups"),
    join(REPO_ROOT, "plans"),
    join(HOME, ".agent", "orch", "runs"),
  ]) {
    if (!existsSync(dir)) continue;
    for (const entry of readdirSync(dir)) {
      rmSync(join(dir, entry), { recursive: true, force: true });
    }
  }

  // 4. Clean dead project entries from ~/.claude.json
  const claudeJsonPath = join(HOME, ".claude.json");
  if (existsSync(claudeJsonPath)) {
    const data = JSON.parse(readFileSync(claudeJsonPath, "utf8")) as {
      projects?: Record<string, unknown>;
    };
    if (data.projects) {
      for (const key of Object.keys(data.projects)) {
        if (
          !existsSync(key) ||
          /scratch-workspaces|gpui-os/i.test(key) ||
          /^[a-z]:[\\/]aphrody(?:[\\/]|$)/i.test(key)
        ) {
          delete data.projects[key];
        }
      }
      writeFileSync(
        claudeJsonPath,
        `${JSON.stringify(data, null, 2)}\n`,
        "utf8",
      );
    }
  }

  // 5. Clean stale empty or expired (>14d) brain dirs without touching active sessions
  const brainDir = join(HOME, ".gemini", "antigravity-cli", "brain");
  const cutoffMs = Date.now() - 14 * 24 * 60 * 60 * 1000;
  if (existsSync(brainDir)) {
    for (const entry of readdirSync(brainDir)) {
      if (entry === process.env.ANTIGRAVITY_CONVERSATION_ID) continue;
      const full = join(brainDir, entry);
      const st = statSync(full);
      if (!st.isDirectory()) continue;
      const items = readdirSync(full);
      if (items.length === 0 || st.mtimeMs < cutoffMs) {
        rmSync(full, { recursive: true, force: true });
      }
    }
  }

  // 6. Clean broken ~/.hermes/skills symlinks, legacy ~/.agents/SOUL.md, pre-tool-guard.ts, old aphrody-skills copies
  const hermesSkills = join(HOME, ".hermes", "skills");
  if (existsSync(hermesSkills)) {
    for (const entry of readdirSync(hermesSkills)) {
      const full = join(hermesSkills, entry);
      if (!existsSync(full)) rmSync(full, { recursive: true, force: true });
    }
  }
  for (const deadPath of [
    join(HOME, ".agents", "SOUL.md"),
    join(HOME, ".agents", "scripts", "pre-tool-guard.ts"),
    join(HOME, ".gemini", "config", "plugins", "aphrody-skills"),
    join(HOME, ".gemini", "antigravity-cli", "plugins", "aphrody-skills"),
    join(HOME, ".gemini", "extensions", "aphrody-skills"),
  ]) {
    if (existsSync(deadPath) || basename(deadPath) === "aphrody-skills") {
      try {
        rmSync(deadPath, { recursive: true, force: true });
      } catch {
        // Ignore if already removed.
      }
    }
  }

  // Clean legacy Codex TOML sections and AGY CLI legacy workspace entries.
  const codexToml = join(HOME, ".codex", "config.toml");
  if (existsSync(codexToml)) {
    const raw = readFileSync(codexToml, "utf8");
    const next =
      raw
        .replace(/\r?\n\[projects\.'c:\\aphrody'\][^\[]*/i, "\n") // paths:ok: removes the stale legacy entry
        .replace(/\r?\n\[marketplaces\.aphrody\][^\[]*/i, "\n")
        .replace(/\r?\n\[plugins\."aphrody-skills@aphrody"\][^\[]*/i, "\n")
        .replace(/\n{3,}/g, "\n\n")
        .trimEnd() + "\n";
    if (next !== raw) writeFileSync(codexToml, next, "utf8");
  }

  const agyConfigPath = join(HOME, ".gemini", "config", "config.json");
  if (existsSync(agyConfigPath)) {
    const srcRoot = discoverSrcRoot();
    const srcWin = srcRoot.replaceAll("/", "\\\\");
    const raw = readFileSync(agyConfigPath, "utf8");
    const cleaned = raw
      .replaceAll("C:\\\\aphrody", srcWin)
      .replaceAll("C:/aphrody", srcRoot); // paths:ok: stale legacy root
    if (cleaned !== raw) writeFileSync(agyConfigPath, cleaned, "utf8");
  }

  const agyCliSettings = join(
    HOME,
    ".gemini",
    "antigravity-cli",
    "settings.json",
  );
  if (existsSync(agyCliSettings)) {
    try {
      const data = JSON.parse(readFileSync(agyCliSettings, "utf8")) as {
        trustedWorkspaces?: string[];
      };
      if (Array.isArray(data.trustedWorkspaces)) {
        const filtered = data.trustedWorkspaces.filter(
          (w) =>
            existsSync(w) &&
            !/[\\/](gpui-os|labs)$/i.test(w) &&
            !/^[a-z]:[\\/]aphrody(?:[\\/]|$)/i.test(w),
        );
        if (filtered.length !== data.trustedWorkspaces.length) {
          data.trustedWorkspaces = filtered;
          writeFileSync(
            agyCliSettings,
            `${JSON.stringify(data, null, 2)}\n`,
            "utf8",
          );
        }
      }
    } catch {
      // Ignore parse error.
    }
  }
}

if (import.meta.main) {
  const doCleanup = Bun.argv.includes("--cleanup");
  if (doCleanup) cleanupLegacyAndClutter();
  const memories = collectMemories();
  syncSqliteStores(memories);
  console.log(
    `Synced ${memories.length} memories to SQLite${doCleanup ? " + cleaned legacy clutter" : ""}.`,
  );
}
