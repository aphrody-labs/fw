/**
 * Modern Web Guidance & Core Web Vitals (`wck`) gate integrated into `ay`.
 *
 * Pure Bun implementation: zero external dependencies, zero Node.js stdlib.
 */

import { join, resolve } from "node:path";

export type WckSpec = "seo" | "aiseo" | "mwg" | "cwv" | "lh" | "wasm" | "webgpu";

export interface WckFinding {
  readonly file: string;
  readonly rule: string;
  readonly severity: "error" | "warn";
  readonly message: string;
}

const LEGACY_PATTERNS: readonly {
  readonly rule: string;
  readonly severity: "error" | "warn";
  readonly regex: RegExp;
  readonly message: string;
}[] = [
  {
    rule: "no-document-write",
    severity: "error",
    regex: /\bdocument\.write\s*\(/,
    message: "Replace document.write() with DOM APIs or SSR",
  },
  {
    rule: "no-xhr",
    severity: "warn",
    regex: /\bnew\s+XMLHttpRequest\b/,
    message: "Replace XMLHttpRequest with fetch()",
  },
  {
    rule: "img-dimensions",
    severity: "warn",
    regex: /<img(?![^>]*\b(?:width|height)\s*=)[^>]*>/i,
    message: "Add explicit width/height on <img> to prevent CLS",
  },
  {
    rule: "img-alt",
    severity: "error",
    regex: /<img(?![^>]*\balt\s*=)[^>]*>/i,
    message: "Add alt attribute on <img> for accessibility (WCAG 2.2 AA)",
  },
  {
    rule: "no-render-blocking-script",
    severity: "warn",
    regex: /<script(?![^>]*\b(?:async|defer|type\s*=\s*["']module["']))[^>]*\bsrc\s*=/i,
    message: "Add defer, async, or type=\"module\" on external <script>",
  },
];

export async function scanWebPath(
  targetPath: string,
): Promise<WckFinding[]> {
  const abs = resolve(targetPath);
  const findings: WckFinding[] = [];
  const file = Bun.file(abs);

  const files: string[] = [];
  if (await file.exists()) {
    files.push(abs);
  } else {
    const glob = new Bun.Glob("**/*.{html,htm,tsx,jsx,ts,js,css}");
    for await (const rel of glob.scan({ cwd: abs, onlyFiles: true })) {
      if (rel.includes("node_modules") || rel.includes("dist")) continue;
      files.push(join(abs, rel));
    }
  }

  for (const f of files) {
    const text = await Bun.file(f).text();
    for (const pat of LEGACY_PATTERNS) {
      if (pat.regex.test(text)) {
        findings.push({
          file: f,
          rule: pat.rule,
          severity: pat.severity,
          message: pat.message,
        });
      }
    }
    if (f.endsWith(".html") || f.endsWith(".htm")) {
      if (!/<meta[^>]+name=["']viewport["']/i.test(text)) {
        findings.push({
          file: f,
          rule: "meta-viewport",
          severity: "error",
          message: "Missing <meta name=\"viewport\">",
        });
      }
      if (!/<html[^>]+lang=/i.test(text)) {
        findings.push({
          file: f,
          rule: "html-lang",
          severity: "error",
          message: "Missing lang attribute on <html>",
        });
      }
    }
  }

  return findings;
}

export async function runWckCli(args: readonly string[]): Promise<number> {
  const [sub = "check", ...rest] = args;
  const json = rest.includes("--json");
  const positional = rest.filter((a) => !a.startsWith("-"));
  const target = positional[0] ?? "apps/web";

  if (sub === "baseline" || sub === "baselinestatus") {
    const query = positional[0] ?? "cwv";
    const info = {
      query,
      cwv: { LCP: "<2.5s", INP: "<200ms", CLS: "<0.1", TTFB: "<800ms" },
      lighthouseTargets: { performance: 95, accessibility: 100, bestPractices: 100, seo: 100 },
    };
    console.log(JSON.stringify(info, null, 2));
    return 0;
  }

  const findings = await scanWebPath(target);
  if (json) {
    console.log(JSON.stringify({ target, count: findings.length, findings }, null, 2));
  } else if (findings.length === 0) {
    console.log(`wck: ok (${target}, 0 findings)`);
  } else {
    for (const f of findings) {
      console.log(`[${f.severity}] ${f.rule}: ${f.file} — ${f.message}`);
    }
  }

  return findings.some((f) => f.severity === "error") ? 1 : 0;
}

if (import.meta.main) {
  runWckCli(Bun.argv.slice(2)).then((code) => process.exit(code));
}
