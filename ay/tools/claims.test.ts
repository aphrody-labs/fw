import { afterAll, expect, test } from "bun:test";
import { judge, loadClaims, pick, run, table, type Claim } from "./claims.ts";

const server = Bun.serve({
  port: 0,
  fetch(req) {
    const { pathname } = new URL(req.url);
    if (pathname === "/releases.json") return Response.json({ releases: [{ version: "7.4-rc3" }, { version: "7.3.4" }], latest: { version: "7.3.4" }, count: 3 });
    if (pathname === "/pkg") return new Response("pkgbase = x\n\tpkgver = 7.4.rc3\n\tpkgrel = 1\n");
    return new Response("no", { status: 404 });
  },
});
afterAll(() => server.stop(true));
const base = `http://localhost:${server.port}`;
const claim = (extra: Partial<Claim>): Claim => ({ id: "t", text: "t", probe: "json", url: `${base}/releases.json`, op: "eq", ...extra });

test("paths, mapping over arrays and every operator", () => {
  const data = { a: { b: [{ n: "x" }, { n: "y" }] }, c: 5 };
  expect(pick(data, "a.b.0.n")).toBe("x");
  expect(pick(data, "a.b.*.n")).toEqual(["x", "y"]);
  expect(pick(data, "a.missing.deep")).toBeUndefined();
  expect(judge("eq", 5, "5")).toBe(true);
  expect(judge("matches", "7.4-rc3", "^7\\.4-rc")).toBe(true);
  expect(judge("any-matches", ["a", "b"], "^b$")).toBe(true);
  expect(judge("none-matches", ["a", "b"], "^c$")).toBe(true);
  expect(judge("gte", 5, 5)).toBe(true);
  expect(judge("lte", 6, 5)).toBe(false);
  expect(judge("exists", [], undefined)).toBe(false);
});

test("a probe is proved, failed (the claim is false) or error (the source is unreadable)", async () => {
  expect((await run(claim({ path: "releases.0.version", op: "matches", value: "^7\\.4-rc" }), "2026-01-01")).status).toBe("proved");
  expect((await run(claim({ path: "releases.*.version", op: "any-matches", value: "^7\\.5" }), "2026-01-01")).status).toBe("failed");
  expect((await run(claim({ probe: "regex", url: `${base}/pkg`, pattern: "pkgver = (\\S+)", op: "matches", value: "^7\\.\\d+\\.rc\\d+$" }))).status).toBe("proved");
  expect((await run(claim({ probe: "status", url: `${base}/nothing`, op: "eq", value: 200 }))).status).toBe("failed");
  const dead = await run(claim({ url: `${base}/gone`, path: "x" }));
  expect(dead.status).toBe("error");
  expect(dead.observed).toContain("404");
});

test("the table reports counts and escapes pipes", () => {
  const t = table([{ id: "a", text: "A", status: "proved", observed: "x|y", checked: "2026-01-01" }, { id: "b", text: "B", status: "failed", observed: "z", checked: "2026-01-01" }]);
  expect(t).toContain("1 proved, 1 failed, 0 unreadable.");
  expect(t).toContain("x/y");
});

test("the repository's claims are well formed, unique and use known probes", async () => {
  const claims = await loadClaims();
  expect(new Set(claims.map((c) => c.id)).size).toBe(claims.length);
  for (const c of claims) {
    expect(["json", "gh", "regex", "status", "git-refs"], c.id).toContain(c.probe);
    expect(c.url, c.id).toBeTruthy();
    if (c.op !== "exists") expect(c.value, c.id).not.toBeUndefined();
    if (c.probe === "regex") expect(c.pattern, c.id).toBeTruthy();
  }
});
