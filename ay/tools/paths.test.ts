import { describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { globToRegExp, parseAllow, scanRepo, scanText } from "./paths.ts";

const kinds = (s: string) => scanText("f", s).map(f => `${f.kind}:${f.match}`);

describe("absolute-path literals", () => {
  test("Windows drives in every spelling", () => {
    expect(kinds(String.raw`run C:\Users\x and C:/src/y and "C:\\Program Files" or %s`)).toEqual([
      String.raw`drive:C:\U`,
      "drive:C:/s",
      String.raw`drive:C:\\P`,
    ]);
  });
  test("MSYS drives, home directories and host layouts", () => {
    expect(kinds("cd /c/src/ay; ls /home/ubuntu/x /Users/bob/y /root/.cache /srv/xbuild")).toEqual([
      "msys:/c/s",
      "home:/home/ubuntu",
      "users:/Users/bob",
      "root:/root/",
      "srv:/srv/",
    ]);
  });
  test("derived paths, URLs and standard locations are clean", () => {
    const clean = [
      "https://example.com/c/src and file:///etc/os-release",
      "join(os.homedir(), '.local', 'bin') and $HOME/src and %USERPROFILE%\\bin",
      "/etc/os-release /proc/cpuinfo /usr/bin/env /tmp/x ./srv/x ../home/y a/home/b",
      "git@github.com:org/repo.git and ssh host:/path and x:y",
    ];
    for (const c of clean) expect(kinds(c)).toEqual([]);
    expect(kinds(String.raw`const asm = "f:\n\tmov"`)).toEqual([]);
  });
  test("an inline paths:ok marker exempts the line", () => {
    expect(kinds("C:\\x // paths:ok")).toEqual([]);
  });
  test("reports file, line and trimmed text", () => {
    const [f] = scanText("a/b.ts", "ok\n  const p = '/home/me/x';");
    expect(f).toMatchObject({ file: "a/b.ts", line: 2, kind: "home", text: "const p = '/home/me/x';" });
  });
});

describe("allowlist", () => {
  const ok = (glob: string, path: string) => globToRegExp(glob).test(path);
  test("globs", () => {
    expect(ok("docs/**", "docs/a/b.md")).toBe(true);
    expect(ok("docs/*.md", "docs/a/b.md")).toBe(false);
    expect(ok("*.test.ts", "tools/x.test.ts")).toBe(true);
    expect(ok("tools/paths.test.ts", "tools/paths.test.ts")).toBe(true);
    expect(ok("data/", "data/x/y.json")).toBe(true);
    expect(ok("a?c", "abc")).toBe(true);
  });
  test("comments and blanks are ignored", () => {
    const r = parseAllow("# header\n\ndocs/** # archived dump\nfoo.md\n");
    expect(r).toHaveLength(2);
    expect(r[0]!.test("docs/x/y")).toBe(true);
  });
});

describe("repository scan", () => {
  test("scans tracked files, honors the allowlist and --include", () => {
    const dir = mkdtempSync(join(tmpdir(), "paths-"));
    try {
      const git = (...a: string[]) => Bun.spawnSync(["git", "-C", dir, ...a]);
      git("init", "-q");
      mkdirSync(join(dir, "a"));
      mkdirSync(join(dir, "b"));
      writeFileSync(join(dir, "a", "x.ts"), "const p = '/srv/data';\n");
      writeFileSync(join(dir, "b", "y.ts"), "const p = '/home/me';\n");
      writeFileSync(join(dir, "untracked.ts"), "const p = '/root/x';\n");
      git("add", "a", "b");
      expect(scanRepo(dir).map(f => f.file).sort()).toEqual(["a/x.ts", "b/y.ts"]);
      expect(scanRepo(dir, parseAllow("a/**")).map(f => f.file)).toEqual(["b/y.ts"]);
      expect(scanRepo(dir, [], ["a"]).map(f => f.file)).toEqual(["a/x.ts"]);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
