import { describe, expect, test } from "bun:test";
import { join } from "node:path";
import { newestSnapshot, parseConfig, renderUnits, repositoryUrl } from "./offsite-backup.ts";

const cfg = parseConfig(await Bun.file(join(import.meta.dir, "..", "..", "..", "cluster", "backup.toml")).text());

describe("offsite-backup", () => {
  test("config covers the shenron snapshot and the three databases", () => {
    expect(cfg.sources.map((s) => s.name).sort()).toEqual(["nie", "rg", "shenron-container", "shenron_site"]);
    expect(cfg.offsite.bucket).not.toContain("sccache");
  });

  test("repository url and units come only from the config", () => {
    expect(repositoryUrl(cfg, "acct")).toBe(`s3:https://acct.r2.cloudflarestorage.com/${cfg.offsite.bucket}/${cfg.offsite.repository_path}`);
    const { service, timer } = renderUnits(cfg);
    expect(timer).toContain(`OnCalendar=${cfg.offsite.schedule}`);
    expect(service).toContain("offsite-backup.ts run");
  });

  test("newest snapshot ignores non-timestamp names", () => {
    expect(newestSnapshot([".lock", "20261120T030400000001Z", "20261121T030456372851Z", "tmp"])).toBe("20261121T030456372851Z");
    expect(newestSnapshot([".lock"])).toBeUndefined();
  });
});
