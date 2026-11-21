import { afterAll, beforeAll, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { codeMask, commandWords, deriveRules, fixSource, loadRegistry, loadRules, rulesDocument, Scanner, scanRepo, selectRepos, summarize, tokensOf } from "./bun-native.ts";

const tmp = mkdtempSync(join(tmpdir(), "ay-bun-native-test-"));
afterAll(() => rmSync(tmp, { recursive: true, force: true }));

const registryFiles = {
  apis: `
[[apis]]
id = "api/fs-readFileSync"
pattern = '''\\bfs\\.readFileSync\\s*\\(\\s*([^,)]+?)\\s*,\\s*['"]utf-?8['"]\\s*\\)'''
message = "m"
severity = "warn"
replace = "template"
replacement = "await Bun.file($1).text()"
aggressive = true

[[apis]]
id = "api/exec"
pattern = '\\b(?:child_process\\.)?exec\\s*\\('
message = "use Bun.$ for shell"
severity = "warn"
replace = "none"
import_from = "child_process"

[[apis]]
id = "api/buffer-alloc"
pattern = '\\bBuffer\\.alloc\\s*\\(\\s*([^)]+?)\\s*\\)'
message = "m"
severity = "warn"
replace = "template"
replacement = "new Uint8Array($1)"
aggressive = true

[[apis]]
id = "api/process-env"
pattern = '\\bprocess\\.env\\.X\\b'
message = "style"
severity = "info"
replace = "none"

[[apis]]
id = "api/child-process-spawn"
pattern = '\\b(?:[A-Za-z_$][\\w$]*\\s*\\.\\s*)?[A-Za-z_$][\\w$]*\\s*\\('
message = "Bun.spawn is nicer"
severity = "warn"
replace = "none"
import_from = "node:child_process"
`,
  cli: `
[[cli]]
id = "cli/npm-ci"
pattern = '\\bnpm\\s+ci\\b'
replace = "bun install --frozen-lockfile"
message = "m"

[[cli]]
id = "cli/npx"
pattern = '\\bnpx\\s+'
replace = "bunx "
message = "m"
`,
  globals: `
[[globals]]
id = "globals/__dirname"
symbol = "__dirname"
bun = "import.meta.dir"
context = "esm"
rewrite = "template"
severity = "warn"
`,
  modules: `
[[modules]]
id = "imports/node-fs"
module = "fs"
compat = "full"

[[modules]]
id = "imports/node-child_process"
module = "child_process"
compat = "full"

[[modules]]
id = "imports/node-test"
module = "test"
compat = "full"
equivalent = "bun:test"
`,
  packages: `
[[packages]]
id = "imports/bun-native"
package = "express"
replacement = "Bun.serve"
aggressive = true

[[packages]]
id = "imports/bun-native"
package = "rimraf"
replacement = "fs.rm"

[[packages]]
id = "imports/bun-native"
package = "which"
replacement = "Bun.which"
aggressive = true

[[packages]]
id = "imports/rust-sdk-alt"
package = "express"
replacement = "axum"
`,
};

let reg: Awaited<ReturnType<typeof loadRegistry>>;
beforeAll(async () => {
  const dir = join(tmp, "registry");
  mkdirSync(dir);
  for (const [n, t] of Object.entries(registryFiles)) writeFileSync(join(dir, `${n}.toml`), t);
  reg = await loadRegistry("unused", dir);
});

const builtins = () => new Set((reg.files.modules as { module: string }[]).map((m) => m.module));

test("codeMask marks code, not strings, comments, template text or regex literals", () => {
  const src = "a(); // fs.x()\nb('fs.y()'); c(`fs.z() ${d()}`); /* fs.w() */ const r = /fs\\.v()/; e()";
  const m = codeMask(src);
  const code = (needle: string) => m[src.indexOf(needle)] === 1;
  expect(code("a()")).toBe(true);
  expect(code("fs.x")).toBe(false);
  expect(code("fs.y")).toBe(false);
  expect(code("fs.z")).toBe(false);
  expect(code("d()")).toBe(true);
  expect(code("fs.w")).toBe(false);
  expect(code("fs\\.v")).toBe(false);
  expect(code("e()")).toBe(true);
});

test("commandWords reads command positions only", () => {
  expect(commandWords("FOO=1 node a.js && echo node | npx tsc; sudo rimraf dist")).toEqual(["node", "echo", "npx", "rimraf"]);
});

test("derived rules: info rules and generic patterns are dropped, severity follows the registry, Rust alternatives are skipped", () => {
  const { rules, dropped } = deriveRules(reg, new Map([["left-pad", "String.prototype.padStart"]]));
  const ids = rules.map((r) => r.id);
  expect(ids).not.toContain("api/process-env");
  expect(dropped.info).toBe(1);
  expect(dropped.rustAlt).toBe(1);
  expect(rules.filter((r) => r.id === "package/express")).toHaveLength(1);
  expect(rules.find((r) => r.id === "api/fs-readFileSync")?.severity).toBe("behaviour-change"); // aggressive
  expect(rules.find((r) => r.id === "package/rimraf")?.severity).toBe("safe-rewrite");
  expect(rules.find((r) => r.id === "cli/npm-ci")?.severity).toBe("safe-rewrite");
  expect(rules.find((r) => r.id === "api/exec")?.replacement).toBe("Bun.$");
  expect(rules.find((r) => r.id === "api/child-process-spawn")?.match).toContain("spawn");
  expect(rules.find((r) => r.id === "package/left-pad")?.source).toBe("skill:bun-native");
  expect(rules.find((r) => r.id === "api/fs-readFileSync")?.gate).toEqual(["fs", "fs/promises"]);
});

test("Web-standard substitutes are the style tier; shell words match only command-line packages", () => {
  const rules = deriveRules(reg, new Map()).rules;
  expect(rules.find((r) => r.id === "api/buffer-alloc")?.tier).toBe("style");
  expect(rules.find((r) => r.id === "api/fs-readFileSync")?.tier).toBeUndefined();
  const sc = new Scanner(rules, builtins(), false);
  const ids: string[] = [];
  sc.shellLine("which git && rimraf dist", (id) => ids.push(id));
  expect(ids).toEqual(["package/rimraf"]);
});

test("tokensOf names the first Bun owner of each alternative", () => {
  expect(tokensOf("await Bun.file($1).text()")).toEqual(["Bun.file"]);
  expect(tokensOf("Bun.$ / Bun.spawn")).toEqual(["Bun.$", "Bun.spawn"]);
  expect(tokensOf("bun:test")).toEqual(["bun:test"]);
  expect(tokensOf("<global fetch>")).toEqual(["fetch"]);
  expect(tokensOf("bun (Glob)")).toEqual(["Bun.Glob"]);
  expect(tokensOf("bun start")).toEqual(["bun run"]);
  expect(tokensOf("")).toEqual([]);
});

const scanner = () => new Scanner(deriveRules(reg, new Map()).rules, builtins(), false);

test("a JS file: imports by the parser, calls only in code and gated by imports", () => {
  const hits: { rule: string; line: number }[] = [];
  const code = [
    '#!/usr/bin/env node',
    'import fs from "fs";',
    'import express from "express";',
    'import { exec } from "node:child_process";',
    'const a = fs.readFileSync("x", "utf8");',
    '// fs.readFileSync("y", "utf8")',
    'const s = "fs.readFileSync(z, \'utf8\')";',
    'exec("ls"); /re/.exec("x"); Bun.TOML.parse("");',
    'const d = __dirname;',
  ].join("\n");
  scanner().jsFile("a.mjs", code, new Set(), hits as never, () => {});
  const ids = hits.map((h) => h.rule);
  expect(ids).toContain("shebang/node");
  expect(ids).toContain("module/node-prefix"); // bare `fs`
  expect(ids).toContain("package/express");
  expect(ids.filter((i) => i === "api/fs-readFileSync")).toHaveLength(1);
  expect(hits.find((h) => h.rule === "api/fs-readFileSync")?.line).toBe(5);
  expect(ids.filter((i) => i === "api/exec")).toHaveLength(1); // `/re/.exec(` is not child_process
  expect(ids).toContain("globals/__dirname");
});

test("a builtin name declared in package.json is an npm package, not a missing node: prefix", () => {
  const hits: { rule: string }[] = [];
  scanner().jsFile("b.ts", 'import fs from "fs";', new Set(["fs"]), hits as never, () => {});
  expect(hits.map((h) => h.rule)).not.toContain("module/node-prefix");
});

test("scanRepo reads package.json, scripts, tsconfig, workflows, Dockerfiles and lockfiles", async () => {
  const repo = join(tmp, "repo");
  const put = (p: string, c: string) => {
    mkdirSync(dirname(join(repo, p)), { recursive: true });
    writeFileSync(join(repo, p), c);
  };
  put("package.json", JSON.stringify({ dependencies: { express: "^4" }, scripts: { ci: "npm ci && node build.js", dev: "rimraf dist" }, packageManager: "pnpm@9" }));
  put("package-lock.json", "{}");
  put("tsconfig.json", '{ // jsonc\n "compilerOptions": { "types": ["node"], "moduleResolution": "node" } }');
  put(".github/workflows/ci.yml", "jobs:\n  t:\n    steps:\n      - uses: actions/setup-node@v4\n      - run: |\n          npx tsc\n");
  put("Dockerfile", 'FROM node:22\nRUN echo a \\\n  && npm ci\nCMD ["node", "x.js"]\n');
  put("node_modules/x/index.js", 'import e from "express";');
  put("src/index.ts", 'import express from "express";\nexport const x = 1;');
  const sc = new Scanner(
    [
      ...deriveRules(reg, new Map()).rules,
    ],
    builtins(),
    false,
  );
  const res = await scanRepo(repo, sc, new Set(["node_modules"]));
  const ids = new Set(res.hits.map((h) => h.rule));
  for (const want of ["package/express", "cli/npm-ci", "cli/npx", "command/node", "package/rimraf", "config/package-manager", "lockfile/foreign", "lockfile/missing", "tsconfig/types-node", "tsconfig/module-resolution", "tsconfig/no-bun-types", "ci/setup-node", "docker/from-node"])
    expect(ids).toContain(want);
  expect(res.hits.some((h) => h.file.startsWith("node_modules"))).toBe(false);
  // a continued RUN is reported where it starts; the CMD after it keeps its own line
  expect(res.hits.find((h) => h.file === "Dockerfile" && h.rule === "cli/npm-ci")?.line).toBe(2);
  expect(res.hits.find((h) => h.file === "Dockerfile" && h.rule === "command/node")?.line).toBe(4);
  const rules = new Map(deriveRules(reg, new Map()).rules.map((r) => [r.id, r]));
  const sum = summarize(res, rules);
  expect(sum.total).toBe(res.hits.length);
  expect(sum.severity["safe-rewrite"] + sum.severity["behaviour-change"] + sum.severity["no-equivalent"]).toBe(sum.total);
});

test("fixSource adds node: prefixes and replaces __dirname only in code of ES modules that do not declare it", () => {
  const all = new Set(["module/node-prefix", "globals/__dirname", "globals/__filename"]);
  const src = [
    'import fs from "fs";',
    'import events from "events";',
    'const p = require("path");',
    "const s = \"from 'fs' __dirname\";",
    "// __dirname",
    "const a = join(__dirname, 'x');",
    "const b = __filename; const c = obj.__dirname;",
  ].join("\n");
  const out = fixSource(src, all, new Set(["events"]), new Set(["fs", "events", "path"]));
  expect(out).toContain('import fs from "node:fs";');
  expect(out).toContain('import events from "events";'); // declared in package.json: an npm package
  expect(out).toContain('require("node:path")');
  expect(out).toContain("\"from 'fs' __dirname\""); // string untouched
  expect(out).toContain("// __dirname");
  expect(out).toContain("join(import.meta.dir, 'x')");
  expect(out).toContain("const b = import.meta.path; const c = obj.__dirname;");
  const own = 'import x from "x";\nconst __dirname = "mine";\nuse(__dirname);';
  expect(fixSource(own, all, new Set(), new Set())).toBe(own);
  const cjs = "const d = __dirname;";
  expect(fixSource(cjs, all, new Set(), new Set())).toBe(cjs);
});

test("unproven rules never match", () => {
  const rules = deriveRules(reg, new Map()).rules; // none proven yet
  const sc = new Scanner(rules, builtins());
  expect(sc.has("package/express")).toBe(false);
});

test("repository selection: ay.toml repos minus runtime forks, plus non-archived originals, minus legacy and mirrors", () => {
  const cfg = { repos: { a: { slug: "o/a", kind: "sdk", checkout: "../a" }, b: { slug: "o/b", kind: "runtime" } } };
  const inv = { github: { o: { originals: [{ name: "c", archived: false }, { name: "d", archived: true }, { name: "legacy", archived: false }, { name: "m", archived: false }, { name: "a", archived: false }] } } };
  const out = selectRepos(cfg, { registry: "", crates: "", exclude: [], mirrors: ["m"], legacy: "legacy" }, inv);
  expect(out.map((r) => r.slug)).toEqual(["o/a", "o/c"]);
});

test("the committed rules file parses, records the Bun version, carries no repository data and every proven rule has its proof", async () => {
  const file = Bun.file(new URL("../docs/research/bun-native-rules.toml", import.meta.url));
  if (!(await file.exists())) return;
  const t = Bun.TOML.parse(await file.text()) as any;
  expect(t.meta.bun_version).toMatch(/^\d+\.\d+\.\d+/);
  expect(t.meta.proven + t.meta.unproven).toBe(t.rule.length);
  expect(t.repo).toBeUndefined();
  for (const r of t.rule) if (r.status === "proven") expect(r.check_ok && (r.doc || r.types)).toBeTruthy();
  expect(JSON.stringify(t)).not.toMatch(/"(?:\/(?:home|srv|root|Users)\/|[A-Za-z]:\\\\)/);
  const { rules, builtins } = loadRules(new URL("../docs/research/bun-native-rules.toml", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1"));
  expect(rules.length).toBe(t.rule.length);
  expect(builtins.has("fs")).toBe(true);
});

test("the rules document round-trips through loadRules and keeps proofs, gates and tiers", async () => {
  const { rules } = deriveRules(reg, new Map());
  const dir = join(tmp, "doc");
  mkdirSync(dir);
  const path = join(dir, "rules.toml");
  const audit = { registry: "n2b-registry", crates: "x-", exclude: [], mirrors: [], legacy: "" };
  writeFileSync(path, rulesDocument({ rules, reg, dropped: {}, builtins: builtins(), bunVersion: "1.0.0", revision: "abc", audit, skillRules: 0 }));
  const back = loadRules(path);
  expect(back.rules.map((r) => r.id)).toEqual(rules.map((r) => r.id));
  expect(back.rules.find((r) => r.id === "api/fs-readFileSync")?.gate).toEqual(["fs", "fs/promises"]);
  expect(back.rules.find((r) => r.id === "api/buffer-alloc")?.tier).toBe("style");
  expect(back.builtins.has("fs")).toBe(true);
  expect(back.meta.registry_crate).toBe("x-n2b-registry");
});
