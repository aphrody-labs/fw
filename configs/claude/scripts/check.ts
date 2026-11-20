import {
  existsSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, relative, resolve } from "node:path";
import { FORBIDDEN, type Settings, undeclaredPlugins } from "./hygiene.ts";
import { discoverWorkspaceRoot, REPO_ROOT, runCmd } from "./lib.ts";

let failed = 0;

function checkStep(label: string, cmd: string[]): void {
  const res = runCmd(cmd);
  if (!res.ok) {
    failed++;
    console.error(`FAIL [${label}]:\n${res.out}\n${res.err}`);
  } else {
    console.log(`PASS [${label}]`);
  }
}

// 1. Validate marketplace & both plugins across Claude, AGY (--strict), and Gemini CLI.
if (Bun.which("claude")) {
  checkStep("claude validate marketplace", [
    "claude",
    "plugin",
    "validate",
    REPO_ROOT,
  ]);
  checkStep("claude validate aphrody", [
    "claude",
    "plugin",
    "validate",
    resolve(REPO_ROOT, "packages", "aphrody"),
  ]);
  checkStep("claude validate bun", [
    "claude",
    "plugin",
    "validate",
    resolve(REPO_ROOT, "packages", "bun"),
  ]);
}
if (Bun.which("agy")) {
  for (const name of ["aphrody", "bun"]) {
    const res = runCmd([
      "agy",
      "plugin",
      "validate",
      resolve(REPO_ROOT, "packages", name),
      "--strict",
      "--json",
    ]);
    if (!res.ok) {
      failed++;
      console.error(`FAIL [agy validate ${name}]:\n${res.out}\n${res.err}`);
    } else {
      console.log(`PASS [agy validate ${name} --strict]`);
    }
  }
}
if (Bun.which("gemini")) {
  checkStep("gemini validate aphrody", [
    "gemini",
    "extensions",
    "validate",
    resolve(REPO_ROOT, "packages", "aphrody"),
  ]);
  checkStep("gemini validate bun", [
    "gemini",
    "extensions",
    "validate",
    resolve(REPO_ROOT, "packages", "bun"),
  ]);
}

// 2. Native TypeScript, lint, and formatting checks.
checkStep("bun check", ["bun", "check", "--no-script"]);
checkStep("agy skill alignment (scripts/align-agy.ts --check)", [
  "bun",
  "scripts/align-agy.ts",
  "--check",
]);
// Directory map: one tool for every repository, owned by the organization repository (aphrody-labs/.github tools/map.ts).
const mapTool = join(discoverWorkspaceRoot(), "aphrody-labs-github", "tools", "map.ts");
if (existsSync(mapTool))
  checkStep("directory map (aphrody-labs/.github tools/map.ts --check)", ["bun", mapTool, "--root", REPO_ROOT, "--check"]);
checkStep("Claude profile (scripts/profile.ts --check)", [
  "bun",
  "scripts/profile.ts",
  "--check",
  REPO_ROOT,
]);
checkStep("bun lint", ["bun", "lint"]);
checkStep("bun fmt --check", [
  "bun",
  "fmt",
  "--check",
  "scripts",
  "packages/aphrody/hooks-handlers",
  "packages/bun/hooks",
  "CLAUDE.md",
  "README.md",
  "settings.json",
  "package.json",
  "tsconfig.json",
  ".claude-plugin/marketplace.json",
]);

// 3. Coherence & hygiene audit on tracked config files.
const settingsPath = join(REPO_ROOT, "settings.json");
const settings = JSON.parse(readFileSync(settingsPath, "utf8")) as Settings;
if (settings.permissions && "deny" in settings.permissions) {
  failed++;
  console.error(
    "FAIL [hygiene]: settings.json must not declare permissions.deny",
  );
}
const unexpectedPlugins = undeclaredPlugins(settings);
if (unexpectedPlugins.length > 0) {
  failed++;
  console.error(
    `FAIL [hygiene]: enabledPlugins from an undeclared marketplace: ${unexpectedPlugins.join(", ")}`,
  );
}

// Scan portable files (excluding historical project memory descriptions) for stale literals.
const SCAN_ROOTS = [
  "CLAUDE.md",
  "settings.json",
  "package.json",
  "tsconfig.json",
  "README.md",
  ".claude",
  ".claude-plugin",
  "gemini",
  "scripts",
  "packages",
];
function walk(p: string, out: string[]): void {
  if (!existsSync(p)) return;
  const st = statSync(p);
  if (st.isFile()) {
    if (/\.(ts|json|md|txt)$/i.test(p)) out.push(p);
    return;
  }
  for (const name of readdirSync(p)) {
    if (name === "node_modules" || name === ".git") continue;
    walk(join(p, name), out);
  }
}

const files: string[] = [];
for (const entry of SCAN_ROOTS) walk(join(REPO_ROOT, entry), files);

let lintHits = 0;
for (const f of files) {
  const rel = relative(REPO_ROOT, f).replaceAll("\\", "/");
  // Exclude self-referential checks in check.ts/merge-brain.ts and third-party Windows path pattern examples if any.
  if (
    rel === "scripts/check.ts" ||
    rel === "scripts/hygiene.ts" ||
    rel === "scripts/hygiene.test.ts" ||
    rel === "scripts/merge-brain.ts"
  ) {
    continue;
  }
  const text = readFileSync(f, "utf8");
  for (const rule of FORBIDDEN) {
    if (rule.re.test(text)) {
      lintHits++;
      failed++;
      console.error(`FAIL [hygiene ${rule.label}] in ${rel}`);
    }
  }
}
if (lintHits === 0) console.log("PASS [hygiene scan]");

// Hardcoded absolute paths (scanner: ay/tools/paths.ts under WORKSPACE_ROOT, allowlist: paths.allow).
const pathsScanner = join(discoverWorkspaceRoot(), "ay", "tools", "paths.ts");
if (existsSync(pathsScanner)) {
  checkStep("absolute paths", ["bun", pathsScanner, "--root", REPO_ROOT]);
} else {
  console.log(
    "SKIP [absolute paths]: ay/tools/paths.ts not found next to the checkouts",
  );
}

// 4. Smoke test hooks with non-blocking NHITL stdin payloads.
async function smokeHook(
  name: string,
  cmd: string[],
  payload: unknown,
  assertOut: (raw: string) => boolean,
): Promise<void> {
  const p = Bun.spawn(cmd, {
    cwd: REPO_ROOT,
    stdin: new Blob([`${JSON.stringify(payload)}\n`]),
    stdout: "pipe",
    stderr: "pipe",
  });
  const [out, err, code] = await Promise.all([
    new Response(p.stdout).text(),
    new Response(p.stderr).text(),
    p.exited,
  ]);
  if (code !== 0 || !assertOut(out)) {
    failed++;
    console.error(`FAIL [hook ${name}] code=${code} out=${out} err=${err}`);
  } else {
    console.log(`PASS [hook ${name}]`);
  }
}

await smokeHook(
  "bun pre-tool-use rewrite (claude)",
  [
    "bun",
    "--no-install",
    "--no-env-file",
    join(REPO_ROOT, "packages", "bun", "hooks", "pre-tool-use.ts"),
    "claude",
  ],
  {
    hook_event_name: "PreToolUse",
    tool_name: "Bash",
    tool_input: { command: "npm test" },
    cwd: REPO_ROOT,
  },
  (out) =>
    out.includes("bun run test") &&
    out.includes('"allow"') &&
    !out.includes('"deny"') &&
    !out.includes('"ask"'),
);
await smokeHook(
  "aphrody auto-background (claude)",
  [
    "bun",
    "--no-install",
    "--no-env-file",
    join(
      REPO_ROOT,
      "packages",
      "aphrody",
      "hooks-handlers",
      "auto-background.ts",
    ),
    "claude",
  ],
  {
    hook_event_name: "PreToolUse",
    tool_name: "Bash",
    tool_input: { command: "cargo test --workspace" },
  },
  (out) =>
    out.includes('"run_in_background":true') &&
    out.includes('"allow"') &&
    !out.includes('"deny"'),
);
await smokeHook(
  "bun pre-tool-use rewrite (agy toolCall)",
  [
    "bun",
    "--no-install",
    "--no-env-file",
    join(REPO_ROOT, "packages", "bun", "hooks", "pre-tool-use.ts"),
    "agy",
  ],
  {
    conversationId: "00000000-0000-0000-0000-000000000001",
    workspacePaths: [REPO_ROOT],
    hookEventName: "PreToolUse",
    toolCall: { name: "run_command", args: { CommandLine: "npm test" } },
  },
  (out) =>
    out.includes("bun run test") &&
    out.includes('"decision":"allow"') &&
    out.includes('"overwrite"') &&
    !out.includes('"systemMessage"') &&
    !out.includes('"deny"'),
);
await smokeHook(
  "aphrody auto-background (agy toolCall)",
  [
    "bun",
    "--no-install",
    "--no-env-file",
    join(
      REPO_ROOT,
      "packages",
      "aphrody",
      "hooks-handlers",
      "auto-background.ts",
    ),
    "agy",
  ],
  {
    conversationId: "00000000-0000-0000-0000-000000000001",
    hookEventName: "PreToolUse",
    toolCall: {
      name: "run_command",
      args: { CommandLine: "cargo build", WaitMsBeforeAsync: 5000 },
    },
  },
  (out) =>
    out.includes('"decision":"allow"') &&
    out.includes('"WaitMsBeforeAsync":1500') &&
    !out.includes('"systemMessage"') &&
    !out.includes('"deny"'),
);
await smokeHook(
  "aphrody typecheck PostToolUse returns {} (agy toolCall)",
  [
    "bun",
    "--no-install",
    "--no-env-file",
    join(REPO_ROOT, "packages", "aphrody", "hooks-handlers", "typecheck.ts"),
    "agy",
  ],
  {
    conversationId: "00000000-0000-0000-0000-000000000001",
    hookEventName: "PostToolUse",
    toolCall: {
      name: "write_file",
      args: { Path: join(REPO_ROOT, "README.md") },
    },
  },
  (out) => out.trim() === "{}",
);
await smokeHook(
  "bun session-start (agy)",
  [
    "bun",
    "--no-install",
    "--no-env-file",
    join(REPO_ROOT, "packages", "bun", "hooks", "session-start.ts"),
    "agy",
  ],
  { hook_event_name: "SessionStart" },
  (out) => out.includes("injectSteps") && !out.includes("WARNING:"),
);
await smokeHook(
  "aphrody bridge-mcp initialize+tools",
  [
    "bun",
    "--no-install",
    "--no-env-file",
    join(REPO_ROOT, "packages", "aphrody", "hooks-handlers", "bridge-mcp.ts"),
  ],
  { jsonrpc: "2.0", id: 1, method: "tools/list" },
  (out) => out.includes("claude_inject") && out.includes("claude_sessions"),
);

// Test real-time session-bridge hook mailbox queue + delivery cleanly in temp dir.
{
  const tmpRelay = mkdtempSync(join(tmpdir(), "relay-bridge-test-"));
  try {
    process.env.RELAY_HOME = tmpRelay;
    const { queueHookMail } = await import(
      join(REPO_ROOT, "packages", "aphrody", "hooks-handlers", "claude-peer.ts")
    );
    queueHookMail("test-sess-1234", "agy-test", "REALTIME_BRIDGE_OK");
    const p = Bun.spawn(
      [
        "bun",
        "--no-install",
        "--no-env-file",
        join(
          REPO_ROOT,
          "packages",
          "aphrody",
          "hooks-handlers",
          "session-bridge.ts",
        ),
        "deliver",
      ],
      {
        cwd: REPO_ROOT,
        env: { ...process.env, RELAY_HOME: tmpRelay },
        stdin: new Blob([
          JSON.stringify({
            session_id: "test-sess-1234",
            hook_event_name: "PostToolUse",
          }),
        ]),
        stdout: "pipe",
        stderr: "pipe",
      },
    );
    const out = await new Response(p.stdout).text();
    const code = await p.exited;
    if (
      code !== 0 ||
      !out.includes("REALTIME_BRIDGE_OK") ||
      !out.includes("PostToolUse")
    ) {
      failed++;
      console.error(
        `FAIL [hook session-bridge deliver] code=${code} out=${out}`,
      );
    } else {
      console.log("PASS [hook session-bridge deliver]");
    }
  } finally {
    delete process.env.RELAY_HOME;
    rmSync(tmpRelay, { recursive: true, force: true });
  }
}

if (failed > 0) {
  console.error(`\nTotal failures: ${failed}`);
  process.exit(1);
}
console.log("\nAll checks passed.");
