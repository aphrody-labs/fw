import { expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { patterns, scan } from "./private-gate.ts";

const files = (o: Record<string, string>) => new Map(Object.entries(o));

test("generic leaks are found: profile and home paths, e-mail, public addresses; loopback and documentation addresses are fine", () => {
  const root = mkdtempSync(join(tmpdir(), "gate-"));
  try {
    const drive = ["C", ":", "\\", "Users", "\\", "someone"].join("");
    const home = ["", "home", "dev", "x"].join("/");
    const text = [`see ${drive} and ${home}`, "mail me@example.com", "host 51.77.1.2", "local 127.0.0.1 and 203.0.113.9"].join("\n");
    const leaks = scan(files({ "a.md": text }), patterns(root));
    expect(leaks.map((l) => `${l.line}:${l.why}`).sort()).toEqual(["1:a Windows user profile path", "1:a home directory path", "2:an e-mail address", "3:an IPv4 address"].sort());
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("values of ay.local.toml can never reappear in a tracked file", () => {
  const root = mkdtempSync(join(tmpdir(), "gate-"));
  try {
    writeFileSync(join(root, "ay.toml"), "");
    writeFileSync(join(root, "ay.local.toml"), '[hosts]\nbuild = "alpha7"\n[paths.build]\nwork = "/vol/work7"\n');
    const leaks = scan(files({ "d.md": "run on alpha7 in /vol/work7/dir\nalpha77 and xalpha7 are other words", "t.test.ts": "alpha7" }), patterns(root));
    expect(leaks.map((l) => l.line)).toEqual([1, 1]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
