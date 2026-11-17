import { describe, expect, setDefaultTimeout, test } from "bun:test";
import { join } from "node:path";
import { openDb } from "../src/db.ts";
import { ensureMirror } from "../src/git.ts";
import { deriveTags, manifestPaths, parseCargoToml, parseGoMod, parsePackageJson, parsePyproject, parseZon, syncManifests } from "../src/manifests.ts";
import { fixture, makeRepo, tmp } from "./helpers.ts";

setDefaultTimeout(60_000);

describe("manifests from real files", () => {
  test("package.json: name, version, and Bun evidence", () => {
    const m = parsePackageJson(fixture("package.json"))!;
    expect(m).toMatchObject({ ecosystem: "npm", name: "@opentunnel/client", private: false });
    expect(m.version).toMatch(/^\d+\.\d+\.\d+$/);
    expect(parsePackageJson('{"name":"x","private":true,"scripts":{"dev":"bun run src/a.ts"}}')).toMatchObject({ private: true, bun: true });
    expect(parsePackageJson('{"name":"y","packageManager":"bun@1.4.5"}')!.bun).toBe(true);
    expect(parsePackageJson('{"version":"1.0.0"}')!.private).toBe(true);
    expect(parsePackageJson("{ not json")).toBeUndefined();
  });

  test("Cargo.toml: a crate inheriting its version from the workspace, publish = false, a workspace root without a package", () => {
    expect(parseCargoToml(fixture("Cargo.toml"))).toMatchObject({ ecosystem: "crates", name: "opentunnel-cli", version: null, private: false });
    expect(parseCargoToml('[package]\nname = "p"\nversion = "1.0.0"\npublish = false\n')).toMatchObject({ version: "1.0.0", private: true });
    expect(parseCargoToml('[workspace]\nmembers = ["a"]\n')).toMatchObject({ name: null, private: true });
  });

  test("pyproject.toml: PEP 621 and Poetry names", () => {
    expect(parsePyproject(fixture("pyproject.toml"))).toMatchObject({ ecosystem: "pypi", name: "click", private: false });
    expect(parsePyproject('[tool.poetry]\nname = "po"\nversion = "0.1.0"\n')).toMatchObject({ name: "po", version: "0.1.0" });
    expect(parsePyproject('[project]\nname = "n"\nclassifiers = ["Private :: Do Not Upload"]\n')!.private).toBe(true);
  });

  test("build.zig.zon: enum literal names, string names, dependencies are not mistaken for the package", () => {
    expect(parseZon(fixture("build.zig.zon"))).toMatchObject({ ecosystem: "zig", name: "opentui" });
    expect(parseZon(fixture("build.zig.zon"))!.version).toMatch(/^\d+\.\d+\.\d+/);
    expect(parseZon('.{ .name = "quoted", .version = "1.2.3", .dependencies = .{ .name = .nope } }')).toMatchObject({ name: "quoted", version: "1.2.3" });
    expect(parseZon(".{ .paths = .{} }")).toBeUndefined();
  });

  test("go.mod module path", () => {
    expect(parseGoMod("module example.test/org/torpedo\n\ngo 1.22\n")).toMatchObject({ ecosystem: "go", name: "example.test/org/torpedo" });
  });

  test("manifestPaths skips vendored and fixture copies", () => {
    expect(manifestPaths(["package.json", "a/Cargo.toml", "node_modules/x/package.json", "third_party/y/go.mod", "test/fixtures/package.json", "README.md"])).toEqual(["package.json", "a/Cargo.toml"]);
  });
});

describe("ecosystem tags", () => {
  const m = (ecosystem: any, path: string, bun = false) => ({ path, ecosystem, name: "n", version: null, private: false, bun });
  test("language share, manifests and Bun files each tag a repo with their evidence", () => {
    const tags = deriveTags({
      languages: [{ language: "Rust", share: 0.5 }, { language: "TypeScript", share: 0.4 }, { language: "Zig", share: 0.02 }, { language: "Shell", share: 0.08 }],
      manifests: [m("crates", "Cargo.toml"), m("zig", "native/build.zig.zon"), m("pypi", "py/pyproject.toml"), m("npm", "package.json")],
      paths: ["bun.lock", "package.json", "Cargo.toml"],
    });
    expect(tags).toEqual([
      { tag: "bun", via: "bun.lock", share: null },
      { tag: "python", via: "py/pyproject.toml", share: null },
      { tag: "rust", via: "language", share: 0.5 },
      { tag: "typescript", via: "language", share: 0.4 },
      { tag: "zig", via: "native/build.zig.zon", share: null },
    ]);
  });

  test("a package.json that runs bun tags the repo even without a lockfile; a plain Node repo is not Bun", () => {
    expect(deriveTags({ languages: [], manifests: [m("npm", "package.json", true)], paths: [] }).map(t => t.tag)).toEqual(["bun"]);
    expect(deriveTags({ languages: [{ language: "JavaScript", share: 1 }], manifests: [m("npm", "package.json")], paths: [] }).map(t => t.tag)).toEqual(["javascript"]);
  });
});

test("syncManifests reads the default branch of a real repository and stores manifests and tags", async () => {
  const origin = await makeRepo([{ message: "m", files: { "package.json": fixture("package.json"), "crates/a/Cargo.toml": fixture("Cargo.toml"), "build.zig": "const std = @import(\"std\");\n", "bun.lock": "{}", "node_modules/x/package.json": '{"name":"vendored"}' } }]);
  const dir = join(tmp(), "m.git");
  await ensureMirror(origin, dir);
  const db = openDb(":memory:");
  db.query("INSERT INTO repo_languages (repo, language, bytes, share) VALUES ('o/r', 'TypeScript', 10, 0.9)").run();
  const out = await syncManifests(db, "o/r", dir, "main");
  expect(out.tags).toEqual(["bun", "rust", "typescript", "zig"]);
  expect(db.query("SELECT path, ecosystem, name FROM manifests ORDER BY path").all()).toEqual([
    { path: "crates/a/Cargo.toml", ecosystem: "crates", name: "opentunnel-cli" },
    { path: "package.json", ecosystem: "npm", name: "@opentunnel/client" },
  ]);
  expect(db.query("SELECT tag, via FROM repo_tags ORDER BY tag").all()).toEqual([
    { tag: "bun", via: "bun.lock" }, { tag: "rust", via: "crates/a/Cargo.toml" }, { tag: "typescript", via: "language" }, { tag: "zig", via: "build.zig" },
  ]);
});
