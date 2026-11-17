#!/usr/bin/env bun
// Unified deployment planner & executor across `ay.toml` products, Cloudflare edge resources, and fleet host roles.
// Never modifies frozen root domains (`ay.toml` [domains]: DNS, zones, and custom routes stay read-only).
// Generated plans are written strictly to `.private/deploy-plan.{json,md}`.
//   bun tools/deploy.ts [--stage dev|stg|prod] [--product <key>] [--json] [--apply]
import { CLOUDFLARE_OS_GATEKEEPERS } from "../packages/edge-stack/src/index.ts";
import { privatePath, readAy, repoRoot } from "./local.ts";

export type Stage = "dev" | "stg" | "prod";

export interface DeployUnit {
  id: string;
  product: string;
  ownerRepo: string;
  targetKind: "cloudflare-worker" | "cloudflare-r2" | "host-service" | "sdk-artifact";
  resourceName: string;
  stage: Stage;
  frozenDomainProtected: boolean;
  command: string[];
}

export interface DeployPlan {
  productName: string;
  stage: Stage;
  dryRun: boolean;
  frozenDomains: string[];
  hostRolesConfigured: string[];
  units: DeployUnit[];
}

const VALID_STAGES = new Set<Stage>(["dev", "stg", "prod"]);

export function assertNoFrozenDomainMutation(targetRoute: string, frozenDomains: string[]): void {
  const lower = targetRoute.toLowerCase();
  for (const dom of frozenDomains) {
    const d = dom.toLowerCase();
    if (lower === d || lower.endsWith(`.${d}`) || lower.includes(`/${d}`)) {
      throw new Error(`refusing route or DNS mutation on frozen domain ${dom} (owned by ay.toml [domains])`);
    }
  }
}

export function buildDeployPlan(
  cfg: Record<string, unknown>,
  opts: { stage?: Stage; product?: string; dryRun?: boolean } = {},
): DeployPlan {
  const stage: Stage = opts.stage ?? "prod";
  if (!VALID_STAGES.has(stage)) throw new Error(`invalid stage: ${stage}`);
  const dryRun = opts.dryRun ?? true;

  const productMeta = (cfg.product ?? {}) as { name?: string };
  const domains = (cfg.domains ?? {}) as { root?: string; app?: string };
  const frozenDomains = [domains.root, domains.app].filter((x): x is string => Boolean(x));
  const products = (cfg.products ?? {}) as Record<string, { owner: string; title: string }>;
  const repos = (cfg.repos ?? {}) as Record<string, { slug: string }>;
  const hosts = Object.keys((cfg.hosts ?? {}) as Record<string, string>).sort();

  const units: DeployUnit[] = [];
  for (const [key, prod] of Object.entries(products)) {
    if (opts.product && opts.product !== key) continue;
    const ownerSlug = repos[prod.owner]?.slug ?? prod.owner;
    const resourceName = `ay-${key}-${stage}`;
    units.push({
      id: `product:${key}`,
      product: key,
      ownerRepo: ownerSlug,
      targetKind: key === "cloud" || key === "search" ? "cloudflare-worker" : key === "os" ? "host-service" : "sdk-artifact",
      resourceName,
      stage,
      frozenDomainProtected: true,
      command:
        key === "cloud" || key === "search"
          ? ["bunx", "wrangler", "deploy", "--name", resourceName, ...(dryRun ? ["--dry-run"] : [])]
          : ["bun", "run", "check"],
    });
  }

  if (!opts.product || opts.product === "cloud") {
    for (const gk of CLOUDFLARE_OS_GATEKEEPERS) {
      const workerName = gk.workerName.replace(/-prod$/, `-${stage}`);
      units.push({
        id: `gatekeeper:${gk.id}`,
        product: "cloud",
        ownerRepo: repos.ay?.slug ?? "aphrody-dev/ay",
        targetKind: "cloudflare-worker",
        resourceName: workerName,
        stage,
        frozenDomainProtected: true,
        command: ["bunx", "wrangler", "deploy", "--name", workerName, ...(dryRun ? ["--dry-run"] : [])],
      });
    }
  }

  return {
    productName: productMeta.name ?? "aphrody",
    stage,
    dryRun,
    frozenDomains,
    hostRolesConfigured: hosts,
    units,
  };
}

export function renderDeployMarkdown(plan: DeployPlan): string {
  return [
    `# Unified Deploy Plan (${plan.stage}, ${plan.dryRun ? "dry-run" : "apply"})`,
    "",
    `- Frozen domains (read-only): ${plan.frozenDomains.join(", ") || "none"}`,
    `- Configured host roles: ${plan.hostRolesConfigured.join(", ") || "none (local only)"}`,
    `- Deployable units: ${plan.units.length}`,
    "",
    "| Unit | Product | Owner | Kind | Resource |",
    "| --- | --- | --- | --- | --- |",
    ...plan.units.map((u) => `| \`${u.id}\` | ${u.product} | \`${u.ownerRepo}\` | ${u.targetKind} | \`${u.resourceName}\` |`),
    "",
  ].join("\n");
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  const opt = (flag: string) => {
    const i = args.indexOf(flag);
    return i >= 0 ? args[i + 1] : undefined;
  };
  const stage = (opt("--stage") ?? "prod") as Stage;
  const product = opt("--product");
  const dryRun = !args.includes("--apply");
  const cfg = readAy(repoRoot);
  const plan = buildDeployPlan(cfg, { stage, product, dryRun });
  await Bun.write(privatePath("deploy-plan.json", repoRoot, true), JSON.stringify(plan, null, 2) + "\n");
  await Bun.write(privatePath("deploy-plan.md", repoRoot, true), renderDeployMarkdown(plan));
  if (args.includes("--json")) {
    console.log(JSON.stringify(plan, null, 2));
  } else {
    console.log(`deploy plan ready: ${plan.units.length} units (stage=${plan.stage}, dryRun=${plan.dryRun})`);
  }
}
