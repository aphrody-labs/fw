import { expect, test } from "bun:test";
import { resolve_, toToml, type Entry } from "./rust-map.ts";

const entries: Entry[] = [
  { component: "sudo", replaces: "sudo", rust: "sudo-rs", upstream: "o/sudo-rs", arch_pkg: "sudo-rs" },
  { component: "x", replaces: "y", rust: "x-rs", upstream: "o/x", arch_pkg: "x-rs" },
  { component: "libc", replaces: "glibc", rust: "relibc", upstream: "o/relibc", status: "experimental" },
  { component: "init", replaces: "systemd", rust: "", upstream: "", status: "none" },
];

test("status comes from the live package state unless the entry fixes it", async () => {
  const rows = await resolve_(entries, async (p) => (p === "sudo-rs" ? { repo: "extra", version: "0.2.15" } : undefined));
  expect(rows.map((r) => `${r.component}:${r.status}:${r.version}`)).toEqual(["sudo:ships:0.2.15", "x:aur:", "libc:experimental:", "init:none:"]);
});

test("the generated TOML parses back with the counts", async () => {
  const rows = await resolve_(entries, async () => ({ repo: "extra", version: "1" }));
  const t = Bun.TOML.parse(toToml(rows, "2026-01-01")) as { rows: number; shipping: number; row: { component: string }[] };
  expect(t.rows).toBe(4);
  expect(t.shipping).toBe(2);
  expect(t.row[0]!.component).toBe("sudo");
});
