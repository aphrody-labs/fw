#!/usr/bin/env bun
/** Re-record the fixtures from the live services: `bun test/record.ts [name...]`. Bodies are trimmed to small real samples. */
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { stripXssi } from "../src/http.ts";
import { FIXTURES, fixturePath } from "./fixtures.ts";

const only = new Set(process.argv.slice(2));
for (const f of FIXTURES) {
  if (only.size && !only.has(f.name)) continue;
  const res = await fetch(f.url, { headers: { accept: "application/json" }, signal: AbortSignal.timeout(120_000) });
  let body = await res.text();
  if (f.ext === "html") body = body.replace(/<script[\s\S]*?<\/script>/g, "");
  else if (f.trim) {
    const xssi = body.startsWith(")]}'") ? ")]}'\n" : "";
    body = xssi + JSON.stringify(f.trim(JSON.parse(stripXssi(body))));
  }
  mkdirSync(dirname(fixturePath(f)), { recursive: true });
  writeFileSync(fixturePath(f), body);
  console.log(`${res.status} ${f.name} ${body.length} bytes`);
}
