import { expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  bin,
  expand,
  load,
  lockedVersion,
  platformKey,
  problems,
  sourceFor,
  type Manifest,
  type Tool,
} from "./toolchain.ts";

test("the versioned manifest is valid", async () => {
  const manifest = await load();
  expect(problems(manifest)).toEqual([]);
  expect(Object.keys(manifest).length).toBeGreaterThan(0);
});

test("platform keys follow the manifest's naming", () => {
  expect(platformKey("win32", "x64")).toBe("windows-x86_64");
  expect(platformKey("linux", "x64")).toBe("linux-x86_64");
  expect(platformKey("darwin", "arm64")).toBe("macos-aarch64");
});

test("a source resolves its version in the url and the archive member", () => {
  const manifest: Manifest = {
    gh: {
      version: "2.1.0",
      check: ["gh", "--version"],
      role: "x",
      "linux-x86_64": {
        url: "https://e.org/v{version}/gh_{version}.tar.gz",
        sha256: "a".repeat(64),
        archive: "tar.gz",
        member: "gh_{version}/bin/gh",
      },
    },
  };
  expect(sourceFor(manifest.gh!, "linux-x86_64")).toEqual({
    url: "https://e.org/v2.1.0/gh_2.1.0.tar.gz",
    sha256: "a".repeat(64),
    archive: "tar.gz",
    member: "gh_2.1.0/bin/gh",
  });
  expect(sourceFor(manifest.gh!, "windows-x86_64")).toBeUndefined();
});

test("problems rejects loose versions, unpinned or insecure downloads and archives without a member", () => {
  const manifest: Manifest = {
    a: {
      version: "latest",
      check: [],
      role: "",
      "linux-x86_64": {
        url: "http://e.org/a",
        sha256: "abc",
        archive: "tar.gz",
      },
    },
  };
  expect(problems(manifest)).toEqual([
    'a: version "latest" is not an exact version',
    "a: no check command",
    "a: no role",
    "a.linux-x86_64: url is not https",
    "a.linux-x86_64: sha256 is not 64 hex digits",
    "a.linux-x86_64: an archive names its member",
  ]);
});

test("binaries carry .exe on Windows only", () => {
  expect(bin("/b", "jq", "windows-x86_64")).toMatch(/jq\.exe$/);
  expect(bin("/b", "jq", "linux-x86_64")).toMatch(/jq$/);
});

test("the wasm family pins the wasm-bindgen CLI to Cargo.lock and one Binaryen everywhere", async () => {
  const manifest = await load();
  const wasm = Object.entries(manifest).filter(([, t]) => t.group === "wasm");
  expect(wasm.map(([name]) => name)).toEqual(
    expect.arrayContaining(["wasm-bindgen", "wasm-opt", "wasm-tools", "wasmtime", "wasmer", "twiggy", "wasi-sdk"]),
  );
  const binaryen = ["wasm-opt", "wasm-merge", "wasm-metadce", "wasm-split"].map((n) => manifest[n]!.version);
  expect(new Set(binaryen).size).toBe(1);
  expect(manifest["wasm-bindgen"]!.lock).toBeDefined();
});

test("a lock reads the pinned value from a sibling file and ignores a missing one", () => {
  const base = mkdtempSync(join(tmpdir(), "toolchain-lock-"));
  writeFileSync(join(base, "Cargo.lock"), 'name = "wasm-bindgen"\r\nversion = "0.2.130"\r\n');
  const tool: Tool = {
    version: "0.2.130",
    check: ["x"],
    role: "x",
    lock: { file: "Cargo.lock", pattern: 'name = "wasm-bindgen"\\r?\\nversion = "([^"]+)"' },
  };
  expect(lockedVersion(tool, base)).toBe("0.2.130");
  expect(lockedVersion({ ...tool, lock: { file: "absent.lock", pattern: "(x)" } }, base)).toBeUndefined();
  expect(lockedVersion({ ...tool, lock: undefined }, base)).toBeUndefined();
});

test("SDK trees expand {opt} and {version}, and dir or cargo sources need an archive member", () => {
  expect(expand("{opt}/sdk-{version}/bin/clang", "34.0")).toMatch(/\.local[\\/]opt[\\/]sdk-34\.0[\\/]bin[\\/]clang$/);
  const manifest: Manifest = {
    sdk: {
      version: "1.0",
      check: ["x"],
      role: "x",
      "linux-x86_64": { url: "https://e.org/a", sha256: "a".repeat(64), dir: "sdk", cargo: true, post: [["x"]] },
    },
  };
  expect(problems(manifest)).toEqual([
    "sdk.linux-x86_64: a dir or cargo source names its archive and member",
    "sdk.linux-x86_64: dir and cargo are exclusive",
  ]);
});
