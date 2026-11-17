import { describe, expect, test } from "bun:test";
import { readAy, repoRoot } from "./local.ts";
import { assertNoFrozenDomainMutation, buildDeployPlan, renderDeployMarkdown } from "./deploy.ts";

describe("unified deploy planner (tools/deploy.ts)", () => {
  test("builds stage-qualified units for every ay.toml product and Cloudflare-OS gatekeeper", () => {
    const cfg = readAy(repoRoot);
    const plan = buildDeployPlan(cfg, { stage: "stg", dryRun: true });
    expect(plan.dryRun).toBe(true);
    expect(plan.stage).toBe("stg");
    expect(plan.frozenDomains).toContain("aphrody.com");
    expect(plan.frozenDomains).toContain("aphrody.app");
    expect(plan.units.some((u) => u.id === "product:browser")).toBe(true);
    expect(plan.units.some((u) => u.id === "gatekeeper:github" && u.resourceName === "ay-gatekeeper-github-stg")).toBe(true);
    const md = renderDeployMarkdown(plan);
    expect(md).toContain("ay-gatekeeper-github-stg");
  });

  test("refuses any route or DNS mutation touching frozen domains", () => {
    expect(() => assertNoFrozenDomainMutation("api.aphrody.com", ["aphrody.com", "aphrody.app"])).toThrow(/frozen domain/);
    expect(() => assertNoFrozenDomainMutation("aphrody.app", ["aphrody.com", "aphrody.app"])).toThrow(/frozen domain/);
    expect(() => assertNoFrozenDomainMutation("ay-cloud-stg.workers.dev", ["aphrody.com", "aphrody.app"])).not.toThrow();
  });
});
