#!/usr/bin/env bun
// Machine-checkable claims: each statement the estate relies on (a version, a branch, a count, a CI state, a documented
// capability) is a row of .private/claims.toml with a probe that reads the live source and an expectation. The run
// writes .private/claims.md and claims-status.json; a claim is `proved` (probe ran, expectation holds), `failed`
// (probe ran, expectation does not hold: the claim is false now) or `error` (the source could not be read).
//   bun tools/claims.ts           run every probe, write the two outputs, exit 1 when a claim failed
//   bun tools/claims.ts --id=X    run one claim and print it
// Probes: json (URL + path), gh (GitHub API path, token from GITHUB_TOKEN/GH_TOKEN or `gh`), regex (URL + first capture
// group), status (HTTP status), git-refs (`git ls-remote --refs` names). Path: dot segments, `*` maps an array.
import { privatePath } from "./local.ts";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dir, "..");

export type Claim = {
  id: string;
  text: string;
  probe: "json" | "gh" | "regex" | "status" | "git-refs";
  url?: string;
  path?: string;
  pattern?: string;
  headers?: Record<string, string>;
  op: "eq" | "matches" | "any-matches" | "none-matches" | "gte" | "lte" | "exists" | "absent";
  value?: string | number | boolean;
};
export type Result = { id: string; text: string; status: "proved" | "failed" | "error"; observed: string; checked: string };

export const pick = (data: unknown, path: string): unknown => {
  let cur: unknown[] = [data];
  let mapped = false;
  for (const seg of path.split(".").filter(Boolean)) {
    if (seg === "*") {
      cur = cur.flatMap((c) => (Array.isArray(c) ? c : []));
      mapped = true;
    } else cur = cur.map((c) => (c && typeof c === "object" ? (c as Record<string, unknown>)[seg] : undefined));
  }
  return mapped ? cur : cur[0];
};

export function judge(op: Claim["op"], observed: unknown, value: Claim["value"]): boolean {
  const list = Array.isArray(observed) ? observed : [observed];
  switch (op) {
    case "eq": return observed === value || String(observed) === String(value);
    case "matches": return typeof observed === "string" && new RegExp(String(value)).test(observed);
    case "any-matches": return list.some((x) => new RegExp(String(value)).test(String(x)));
    case "none-matches": return !list.some((x) => new RegExp(String(value)).test(String(x)));
    case "gte": return Number(observed) >= Number(value);
    case "lte": return Number(observed) <= Number(value);
    case "exists": return observed !== undefined && observed !== null && !(Array.isArray(observed) && observed.length === 0);
    case "absent": return observed === undefined || observed === null || (Array.isArray(observed) && observed.length === 0);
  }
}

const timeout = () => AbortSignal.timeout(25_000);

/** fetch that waits and retries on 429 and 503 (raw.githubusercontent.com and registries rate-limit bursts). */
async function get(url: string, init: RequestInit): Promise<Response> {
  for (let attempt = 0; ; attempt++) {
    const res = await fetch(url, { ...init, signal: timeout() });
    if ((res.status !== 429 && res.status !== 503) || attempt >= 3) return res;
    await Bun.sleep(1500 * (attempt + 1));
  }
}

async function ghApi(path: string): Promise<unknown> {
  const token = Bun.env.GITHUB_TOKEN ?? Bun.env.GH_TOKEN;
  if (!token) {
    const proc = Bun.spawn(["gh", "api", path], { stdout: "pipe", stderr: "ignore" });
    const [out, code] = await Promise.all([new Response(proc.stdout).text(), proc.exited]);
    if (code === 0) return JSON.parse(out);
  }
  const res = await fetch(`https://api.github.com/${path.replace(/^\//, "")}`, { headers: { accept: "application/vnd.github+json", "user-agent": "ay-claims", ...(token ? { authorization: `Bearer ${token}` } : {}) }, signal: timeout() });
  if (!res.ok) throw new Error(`GitHub API ${res.status}`);
  return res.json();
}

export async function observe(c: Claim): Promise<unknown> {
  switch (c.probe) {
    case "json": {
      const res = await get(c.url!, { headers: { "user-agent": "ay-claims", ...c.headers } });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return pick(await res.json(), c.path ?? "");
    }
    case "gh": return pick(await ghApi(c.url!), c.path ?? "");
    case "regex": {
      const res = await get(c.url!, { headers: { "user-agent": "ay-claims" } });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return new RegExp(c.pattern!, "m").exec(await res.text())?.[1];
    }
    case "status": return (await get(c.url!, { method: "HEAD", redirect: "follow", headers: { "user-agent": "ay-claims" } })).status;
    case "git-refs": {
      const proc = Bun.spawn(["git", "ls-remote", "--refs", c.url!], { stdout: "pipe", stderr: "ignore" });
      const [out, code] = await Promise.all([new Response(proc.stdout).text(), proc.exited]);
      if (code !== 0) throw new Error("git ls-remote failed");
      return out.split("\n").filter(Boolean).map((l) => l.split("\t")[1]!.replace(/^refs\/(heads|tags)\//, ""));
    }
  }
}

const show = (v: unknown): string => (Array.isArray(v) ? `${v.length} item(s): ${v.slice(0, 3).join(", ")}${v.length > 3 ? ", ..." : ""}` : String(v)).slice(0, 120);

export async function run(c: Claim, today = new Date().toISOString().slice(0, 10)): Promise<Result> {
  try {
    const observed = await observe(c);
    return { id: c.id, text: c.text, status: judge(c.op, observed, c.value) ? "proved" : "failed", observed: show(observed), checked: today };
  } catch (e) {
    return { id: c.id, text: c.text, status: "error", observed: String((e as Error).message).slice(0, 120), checked: today };
  }
}

export const table = (results: Result[]): string =>
  ["# Claims", "", "<!-- Generated by tools/claims.ts from .private/claims.toml; do not edit. -->", "", `${results.filter((r) => r.status === "proved").length} proved, ${results.filter((r) => r.status === "failed").length} failed, ${results.filter((r) => r.status === "error").length} unreadable.`, "", "| Claim | Status | Observed | Checked |", "|---|---|---|---|", ...results.map((r) => `| ${r.text} (\`${r.id}\`) | ${r.status} | ${r.observed.replaceAll("|", "/")} | ${r.checked} |`), ""].join("\n");

export async function loadClaims(dir = root): Promise<Claim[]> {
  return (Bun.TOML.parse(await Bun.file(privatePath("claims.toml", dir)).text()) as { claim: Claim[] }).claim;
}

if (import.meta.main) {
  const only = process.argv.find((a) => a.startsWith("--id="))?.slice(5);
  const claims = (await loadClaims()).filter((c) => !only || c.id === only);
  const results: Result[] = [];
  for (let i = 0; i < claims.length; i += 6) results.push(...(await Promise.all(claims.slice(i, i + 6).map((c) => run(c)))));
  if (only) console.log(results[0]);
  else {
    await Bun.write(privatePath("claims.md", root, true), table(results));
    await Bun.write(privatePath("claims-status.json", root, true), JSON.stringify(results, null, 2) + "\n");
    console.log(`${results.filter((r) => r.status === "proved").length}/${results.length} proved`);
    for (const r of results.filter((r) => r.status !== "proved")) console.error(`${r.status}: ${r.id}: ${r.observed}`);
  }
  process.exit(results.some((r) => r.status === "failed") ? 1 : 0);
}
