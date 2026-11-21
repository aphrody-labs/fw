import { expect, test } from "bun:test";
import { assemble, classify, collectCrates, collectNpm, collectRepos, collectTrees, dirIndex, hostScript, counts, LiveIndex, manifestName, norm, npmRepository, parseTrees, ruleFor, scanCheckout, scanRevision, toMarkdown, toToml, type Context, type Manifest, type Rule } from "./recover.ts";

const m = (repo: string, ecosystem: "cargo" | "npm", name: string, path: string): Manifest => ({ repo, ecosystem, name, path });
const ctx = (over: Partial<Context> = {}): Context => ({
  live: new LiveIndex([m("browser", "cargo", "obscura-dom", "crates/obscura-dom/Cargo.toml"), m("ai-os", "npm", "@aphrody/a2a", "packages/a2a/package.json"), m("nie", "cargo", "nie-core", "crates/core/Cargo.toml")]),
  archives: new LiveIndex([m("bun-os", "cargo", "bun-re", "crates/bun-re/Cargo.toml")]),
  forks: new Set(["effectcraft", "zig"]),
  rules: [],
  ...over,
});

test("norm strips scope and aphrody, ay and bunos prefixes and separators", () => {
  expect(norm("@aphrody/m3-tokens")).toBe("m3tokens");
  expect(norm("aphrody_nie_core")).toBe("niecore");
  expect(norm("bunos-wsl-core")).toBe("wslcore");
});

test("manifestName reads Cargo.toml and package.json", () => {
  expect(manifestName("a/Cargo.toml", '[package]\nname = "x"\n')).toBe("x");
  expect(manifestName("a/package.json", '{"name":"@s/y"}')).toBe("@s/y");
  expect(manifestName("a/Cargo.toml", "[workspace]\nmembers = []")).toBeUndefined();
  expect(manifestName("a/package.json", "{broken")).toBeUndefined();
});

test("live index matches exact names first, then identities, never very short ones", () => {
  const i = ctx().live;
  expect(i.find("cargo", "obscura-dom")?.exact).toBe(true);
  expect(i.find("cargo", "aphrody-nie-core")?.exact).toBe(false);
  expect(i.find("cargo", "aphrody-nie-core")?.m.repo).toBe("nie");
  expect(i.find("npm", "@aphrody/a2a")?.m.path).toBe("packages/a2a/package.json");
  expect(i.find("cargo", "aphrody-ab")).toBeUndefined();
});

test("classify: live in a repo, live as a fork, lost with a rule home, dead by rule, lost without home", () => {
  const rules: Rule[] = [
    { origin: "crate", name: "^bun-re$", home: "bun", note: "reverse engineering core" },
    { origin: "crate", name: "^aphrody-tauri", state: "dead", note: "renamed upstream" },
  ];
  const c = ctx({ rules });
  const crate = (name: string) => ({ origin: "crate" as const, name, state: "lost" as const });
  expect(classify(crate("obscura-dom"), c)).toMatchObject({ state: "live", where: "browser:crates/obscura-dom" });
  expect(classify(crate("effectcraft-gpu"), c)).toMatchObject({ state: "live", where: "fork:effectcraft" });
  expect(classify(crate("zigish"), c).state).toBe("lost");
  expect(classify(crate("bun-re"), c)).toMatchObject({ state: "lost", home: "bun", archive: "bun-os:crates/bun-re" });
  expect(classify(crate("aphrody-tauri-utils"), c)).toMatchObject({ state: "dead" });
  expect(classify(crate("mystery"), c)).toMatchObject({ state: "lost", home: "review" });
});

test("ruleFor matches origin, kind and regular expressions case-insensitively", () => {
  const r: Rule[] = [{ origin: "vps", name: "/opt/ANDROID", state: "dead" }];
  expect(ruleFor({ origin: "vps", name: "fr1:/opt/android-sdk", state: "lost" }, r)?.state).toBe("dead");
  expect(ruleFor({ origin: "crate", name: "fr1:/opt/android-sdk", state: "lost" }, r)).toBeUndefined();
});

test("parseTrees reads the host script output and symlinks", () => {
  const tsv = "/srv/a\t12\t\tgit@x:y\tabc1234\t3\t2\t2026-11-16\n/srv/l\t0\t/srv/xbuild/l\t\t\t0\t0\t\nbroken\n";
  const t = parseTrees("fr1", tsv);
  expect(t).toHaveLength(2);
  expect(t[0]).toMatchObject({ host: "fr1", path: "/srv/a", mb: 12, remote: "git@x:y", head: "abc1234", dirty: 3, ahead: 2 });
  expect(t[1]).toMatchObject({ link: "/srv/xbuild/l", mb: 0 });
});

test("collectCrates pages through the owner's crates", async () => {
  const page = (n: number, count: number) => ({ crates: Array.from({ length: count }, (_, i) => ({ name: `c${n}-${i}`, max_version: "1.0.0", repository: null, description: "d", yanked: false })) });
  const calls: string[] = [];
  const get = async (url: string) => {
    calls.push(url);
    if (url.includes("/users/")) return { user: { id: 7 } };
    return url.includes("page=1&") ? page(1, 100) : page(2, 3);
  };
  const out = await collectCrates("me", get, 0);
  expect(out).toHaveLength(103);
  expect(calls.some(u => u.includes("user_id=7"))).toBe(true);
  expect(out[0]).toMatchObject({ name: "c1-0", version: "1.0.0", repository: undefined });
});

test("collectNpm unions the scope list and the maintainer search, then reads the latest manifest", async () => {
  const get = async (url: string) => {
    if (url.includes("search")) return { objects: [{ package: { name: "plain" } }, { package: { name: "@s/a" } }] };
    const name = decodeURIComponent(url.split("registry.npmjs.org/")[1]!);
    if (name === "plain") throw new Error("404");
    return { "dist-tags": { latest: "2.0.0" }, versions: { "2.0.0": { description: "A", repository: { url: "git+https://github.com/o/r.git" }, deprecated: "old" } } };
  };
  const sh = async (cmd: string[]) => (cmd[1] === "whoami" ? "me\n" : JSON.stringify({ "@s/a": "read-write", "@s/b": "read-write" }));
  const out = await collectNpm("@s", get, sh as any);
  expect(out.map(p => p.name)).toEqual(["@s/a", "@s/b", "plain"]);
  expect(out[0]).toMatchObject({ version: "2.0.0", repository: "https://github.com/o/r", description: "A", deprecated: true });
  expect(out[2]).toEqual({ name: "plain" });
  expect(npmRepository({ repository: "git+https://x/y.git" })).toBe("https://x/y");
});

test("collectRepos lists every org", async () => {
  const gh = async (org: string) => JSON.stringify([{ name: `${org}-r`, isArchived: true, isFork: false, description: "", pushedAt: "2026-01-01", diskUsage: 5 }]);
  const out = await collectRepos(["a", "b"], gh);
  expect(out.map(r => r.name)).toEqual(["a-r", "b-r"]);
  expect(out[0]).toMatchObject({ org: "a", archived: true, description: undefined });
});

test("assemble covers every origin and the legacy states", () => {
  const rules: Rule[] = [{ origin: "vps", name: "/opt/dotnet$", state: "dead" }, { origin: "repo", name: "/gaming$", home: "nie" }];
  const units = assemble(
    {
      crates: [{ name: "obscura-dom", version: "1.0.0", yanked: false }, { name: "lost-one", version: "0.1.0", yanked: true }],
      npm: [{ name: "@aphrody/a2a", version: "1.0.0" }],
      repos: [{ name: "gaming", org: "o", archived: true, fork: false }, { name: "live", org: "o", archived: false, fork: false }],
      legacy: [
        { kind: "crate", name: "nie-core", path: "x", product: "craft", state: "tracked" },
        { kind: "third_party", name: "zig", path: "third_party/zig", product: "platform", state: "review", fork: "aphrody-labs/zig" },
        { kind: "app", name: "ghost", path: "apps/ghost", product: "platform", state: "review" },
      ],
      trees: [{ host: "fr1", path: "/opt/dotnet", mb: 900, dirty: 0, ahead: 0, mtime: "2026-10-30" }, { host: "fr1", path: "/srv/work", mb: 5, remote: "r", dirty: 2, ahead: 1, mtime: "2026-11-16" }],
    },
    ctx({ rules }),
  );
  const by = (o: string, n: string) => units.find(u => u.origin === o && u.name.endsWith(n))!;
  expect(by("crate", "obscura-dom").state).toBe("live");
  expect(by("crate", "lost-one")).toMatchObject({ state: "lost", note: "yanked" });
  expect(by("npm", "a2a").state).toBe("live");
  expect(by("repo", "gaming")).toMatchObject({ state: "lost", home: "nie" });
  expect(units.some(u => u.name === "o/live")).toBe(false);
  expect(by("legacy", "nie-core").state).toBe("live");
  expect(by("legacy", "zig")).toMatchObject({ state: "live", where: "fork:aphrody-labs/zig" });
  expect(by("legacy", "ghost")).toMatchObject({ state: "lost", home: "review" });
  expect(by("vps", "/opt/dotnet").state).toBe("dead");
  expect(by("vps", "/srv/work")).toMatchObject({ state: "lost", description: "5 MB, modified 2026-11-16, 2 uncommitted files, 1 unpushed commits" });
  expect(counts(units).get("crate")).toEqual({ live: 1, lost: 1, dead: 0 });
});

test("toToml and toMarkdown are parseable and grouped by home", () => {
  const units = [
    { origin: "crate" as const, name: 'a"b', state: "lost" as const, home: "bun", version: "1.0.0" },
    { origin: "crate" as const, name: "c", state: "live" as const, where: "ay:crates/c" },
  ];
  const parsed = Bun.TOML.parse(toToml(units, { scanned: "2026-11-17" })) as any;
  expect(parsed.unit).toHaveLength(2);
  expect(parsed.unit[0].name).toBe('a"b');
  const md = toMarkdown(units, { scanned: "2026-11-17" });
  expect(md).toContain("| crate | 1 | 1 | 0 | 2 |");
  expect(md).toContain("### bun");
});

test("scanRevision and scanCheckout read manifests of a git revision and of a checkout", async () => {
  const { mkdtempSync, mkdirSync, writeFileSync } = await import("node:fs");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");
  const dir = mkdtempSync(join(tmpdir(), "recover-git-"));
  mkdirSync(join(dir, "a/b"), { recursive: true });
  writeFileSync(join(dir, "a/b/Cargo.toml"), '[package]\nname = "deep"\nversion = "0.1.0"\n');
  writeFileSync(join(dir, "package.json"), '{"name":"@s/top"}');
  writeFileSync(join(dir, "notes.txt"), "x");
  const git = (...a: string[]) => Bun.spawnSync(["git", "-C", dir, "-c", "user.name=t", "-c", "user.email=t@t", ...a]);
  git("init", "-q");
  git("add", ".");
  git("commit", "-q", "-m", "init");
  const rev = await scanRevision("r", dir, "HEAD");
  expect(rev.map(x => `${x.ecosystem}:${x.name}:${x.path}`).sort()).toEqual(["cargo:deep:a/b/Cargo.toml", "npm:@s/top:package.json"]);
  const co = await scanCheckout("r", dir);
  expect(co.map(x => x.name).sort()).toEqual(["@s/top", "deep"]);
});

test("collectTrees asks each distinct host once with its own work volume and parses the answer", async () => {
  const asked: string[] = [];
  const ssh = async (host: string, script: string, work: string) => {
    asked.push(`${host} ${work} ${script.includes("du -sxm")}`);
    return `/x/${host}\t3\t\t\t\t0\t0\t2026-11-17\n`;
  };
  const out = await collectTrees({ build: "h1", test: "h2", prod: "h1" }, { build: "/w1", test: "/w2" }, ["/a"], ssh);
  expect(asked).toEqual(["h1 /w1 true", "h2 /w2 true"]);
  expect(out.map(t => t.path)).toEqual(["/x/h1", "/x/h2"]);
  expect(hostScript()).toContain("git -C");
});

test("dirIndex keys directories by identity and skips upstream copies; legacy dir units match them", () => {
  const dirs = dirIndex("bun", ["src/codegen/a.ts", "vendor/zlib/x.c", "src/node-fallbacks/z.js", "README.md"]);
  expect(dirs.get("codegen")).toBe("bun:src/codegen");
  expect(dirs.get("nodefallbacks")).toBe("bun:src/node-fallbacks");
  expect(dirs.has("zlib")).toBe(false);
  const units = assemble(
    { crates: [], npm: [], repos: [], trees: [], legacy: [{ kind: "dir", name: "codegen", path: "system/codegen", product: "platform", state: "review" }, { kind: "dir", name: "gone", path: "system/gone", product: "platform", state: "review" }] },
    ctx({ dirs }),
  );
  expect(units[0]).toMatchObject({ state: "live", where: "bun:src/codegen" });
  expect(units[1].state).toBe("lost");
});
