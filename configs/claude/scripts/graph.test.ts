import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  buildAny,
  buildGraph,
  importsOf,
  resolveImport,
  scriptsInCommand,
  toMermaid,
} from "./graph.ts";

test("importsOf finds static, re-export and dynamic relative imports only", () => {
  const src = `import a from "./a.ts";\nexport * from "../b.ts";\nconst c = await import("./c.ts");\nimport x from "node:fs";\nimport y from "pkg";`;
  expect(importsOf(src).sort()).toEqual(["../b.ts", "./a.ts", "./c.ts"]);
});

test("resolveImport and scriptsInCommand", () => {
  expect(resolveImport("scripts/a.ts", "../packages/x/y.ts")).toBe(
    "packages/x/y.ts",
  );
  expect(
    scriptsInCommand(
      'bun --no-install "${CLAUDE_PLUGIN_ROOT}/hooks-handlers/learn.ts" capture',
    ),
  ).toEqual(["${CLAUDE_PLUGIN_ROOT}/hooks-handlers/learn.ts"]);
});

function fixture(): string {
  const root = mkdtempSync(join(tmpdir(), "graph-"));
  const p = join(root, "packages", "demo");
  mkdirSync(join(p, ".claude-plugin"), { recursive: true });
  mkdirSync(join(p, "hooks-handlers"));
  mkdirSync(join(p, "skills", "one"), { recursive: true });
  mkdirSync(join(root, "scripts"));
  writeFileSync(
    join(p, ".claude-plugin", "plugin.json"),
    '{"name":"demo","version":"1.2.3"}',
  );
  writeFileSync(
    join(p, ".claude-plugin", "hooks.json"),
    JSON.stringify({
      hooks: {
        PostToolUse: [
          {
            matcher: "Bash",
            hooks: [
              {
                type: "command",
                command: 'bun "${CLAUDE_PLUGIN_ROOT}/hooks-handlers/h.ts"',
              },
            ],
          },
        ],
      },
    }),
  );
  writeFileSync(
    join(p, ".claude-plugin", "mcp.json"),
    JSON.stringify({
      mcpServers: {
        srv: {
          command: "bun",
          args: ["${CLAUDE_PLUGIN_ROOT}/hooks-handlers/m.ts"],
        },
        remote: { url: "https://x" },
      },
    }),
  );
  writeFileSync(
    join(p, "hooks-handlers", "h.ts"),
    'import { x } from "./m.ts";',
  );
  writeFileSync(join(p, "hooks-handlers", "m.ts"), "export const x = 1;");
  writeFileSync(
    join(p, "skills", "one", "SKILL.md"),
    "---\nname: one\ndescription: d\n---\n",
  );
  writeFileSync(
    join(root, "agent.toml"),
    '[[cron]]\nid = "j"\nschedule = "* * * * *"\nkind = "cmd"\nargv = ["bun", "scripts/s.ts"]\n',
  );
  writeFileSync(join(root, "scripts", "s.ts"), "export {};");
  writeFileSync(
    join(root, "settings.json"),
    JSON.stringify({
      enabledPlugins: {
        "demo@aphrody": true,
        "ext@market": true,
        "off@market": false,
      },
    }),
  );
  return root;
}

test("buildGraph links packages, hooks, MCP servers, scripts, cron jobs and settings", () => {
  const root = fixture();
  try {
    const g = buildGraph(root, "demo");
    const has = (from: string, to: string, kind: string) =>
      g.edges.some((e) => e.from === from && e.to === to && e.kind === kind);
    expect(g.nodes.find((n) => n.id === "package:demo")!.label).toBe(
      "demo 1.2.3",
    );
    expect(has("package:demo", "skill:demo/one", "contains")).toBe(true);
    expect(
      has(
        "script:packages/demo/hooks-handlers/h.ts",
        "script:packages/demo/hooks-handlers/m.ts",
        "imports",
      ),
    ).toBe(true);
    expect(
      has(
        "hook:demo/PostToolUse[Bash]/h.ts",
        "script:packages/demo/hooks-handlers/h.ts",
        "runs",
      ),
    ).toBe(true);
    expect(
      has("mcp:demo/srv", "script:packages/demo/hooks-handlers/m.ts", "runs"),
    ).toBe(true);
    expect(
      g.nodes.some(
        (n) => n.id === "mcp:demo/remote" && n.label.includes("remote"),
      ),
    ).toBe(true);
    expect(has("cron:j", "script:scripts/s.ts", "runs")).toBe(true);
    expect(has("settings", "package:demo", "enables")).toBe(true);
    expect(g.nodes.some((n) => n.id === "plugin:ext@market")).toBe(true);
    expect(g.nodes.some((n) => n.id === "plugin:off@market")).toBe(false);
    const mm = toMermaid(g);
    expect(mm.startsWith("flowchart LR")).toBe(true);
    expect(mm).toContain("(1 skills)");
    expect(mm).not.toContain("skill_demo_one");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the committed claude graph matches the repo shape", () => {
  const g = buildGraph(join(import.meta.dir, ".."), "claude");
  for (const id of [
    "package:aphrody",
    "package:bun",
    "mcp:aphrody/memory",
    "cron:graph",
  ])
    expect(g.nodes.some((n) => n.id === id)).toBe(true);
});

test("buildAny picks the JS workspace builder and links workspace dependencies", () => {
  const root = mkdtempSync(join(tmpdir(), "graph-js-"));
  try {
    writeFileSync(
      join(root, "pnpm-workspace.yaml"),
      "packages:\n  - packages/*\n",
    );
    for (const [dir, deps] of [
      ["core", {}],
      ["app", { "@x/core": "1" }],
    ] as const) {
      mkdirSync(join(root, "packages", dir), { recursive: true });
      writeFileSync(
        join(root, "packages", dir, "package.json"),
        JSON.stringify({ name: `@x/${dir}`, dependencies: deps }),
      );
    }
    const g = buildAny(root, "js");
    expect(g.edges).toEqual([
      {
        from: "package:packages/app",
        to: "package:packages/core",
        kind: "depends",
      },
    ]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("buildAny picks the Python builder first when pyproject.toml exists and counts top-level imports", () => {
  const root = mkdtempSync(join(tmpdir(), "graph-py-"));
  try {
    writeFileSync(join(root, "pyproject.toml"), "[project]\nname='x'\n");
    writeFileSync(join(root, "pnpm-workspace.yaml"), "packages: []\n");
    mkdirSync(join(root, "gateway"));
    mkdirSync(join(root, "agent"));
    writeFileSync(
      join(root, "gateway", "a.py"),
      "from agent import run\nimport os\n",
    );
    writeFileSync(join(root, "agent", "run.py"), "x = 1\n");
    const g = buildAny(root, "py");
    expect(g.nodes.map((n) => n.id)).toEqual([
      "module:agent",
      "module:gateway",
    ]);
    expect(g.edges).toEqual([
      { from: "module:gateway", to: "module:agent", kind: "imports" },
    ]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
