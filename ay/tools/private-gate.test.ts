import { expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { ipv4Pattern, patterns, scan } from "./private-gate.ts";

const files = (o: Record<string, string>) => new Map(Object.entries(o));

test("generic leaks are found: profile and home paths, e-mail, public addresses; loopback and documentation addresses are fine", () => {
  const root = mkdtempSync(join(tmpdir(), "gate-"));
  try {
    const drive = ["C", ":", "\\", "Users", "\\", "someone"].join("");
    const home = ["", "home", "dev", "x"].join("/");
    const text = [`see ${drive} and ${home}`, "mail me@example.com", "host 51.77.1.2", "local 127.0.0.1 and 203.0.113.9", "git@github.com:o/r.git Chrome/131.0.0.0 6.6.36.6-microsoft-WSL2"].join("\n");
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

test("the Cloudflare manifest declares what is public: owned role mailboxes and the private mesh; its zone ids stay private", () => {
  const root = mkdtempSync(join(tmpdir(), "gate-"));
  try {
    mkdirSync(join(root, "packages/cf-control-plane"), { recursive: true });
    writeFileSync(join(root, "ay.toml"), "");
    writeFileSync(join(root, "packages/cf-control-plane/cf.toml"), '[domains]\nprimary = ["owned.dev"]\n[cluster]\nmesh_cidr = "10.77.5.0/24"\n');
    writeFileSync(join(root, "cf.local.toml"), '[cloudflare]\naccount_id = "0123456789abcdef0123456789abcdef"\n');
    const text = ["contact@owned.dev", "ceo@owned.dev", "mesh 10.77.5.9", "public 51.77.1.2", "id 0123456789abcdef0123456789abcdef"].join("\n");
    const leaks = scan(files({ "a.md": text }), patterns(root), ipv4Pattern(root));
    expect(leaks.map((l) => `${l.line}:${l.why}`).sort()).toEqual(["2:an e-mail address", "4:an IPv4 address", "5:a value of cf.local.toml (a Cloudflare id or a host address)"].sort());
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
