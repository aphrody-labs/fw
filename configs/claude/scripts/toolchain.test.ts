import { expect, test } from "bun:test";
import {
  bin,
  load,
  platformKey,
  problems,
  sourceFor,
  type Manifest,
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
