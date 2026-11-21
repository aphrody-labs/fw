import { describe, expect, test } from "bun:test";
import { loadConfig, redact, renderClient, renderScheduler, renderServer, renderUnit, schedulerUrl, serverHosts, type Secrets } from "./build-cluster.ts";

const secrets = (hosts: string[]): Secrets => ({ clientToken: "ct", hs256Key: "key", serverTokens: Object.fromEntries(hosts.map((h) => [h, `tok-${h}`])) });

describe("build-cluster", async () => {
  const cfg = await loadConfig();

  test("every address and path is read from cluster/build.toml", () => {
    const hub = cfg.hosts[cfg.scheduler.host]!;
    expect(schedulerUrl(cfg)).toBe(`http://${hub.address}:${cfg.sccache_dist.scheduler_port}`);
    expect(renderScheduler(cfg, secrets([]))).toContain(`public_addr = "${hub.address}:${cfg.sccache_dist.scheduler_port}"`);
  });

  test("each server gets its own token, address and work dir", () => {
    const hosts = serverHosts(cfg);
    expect(hosts.length).toBeGreaterThanOrEqual(2);
    for (const host of hosts) {
      const toml = renderServer(cfg, host, secrets(hosts));
      expect(toml).toContain(`token = "tok-${host}"`);
      expect(toml).toContain(`public_addr = "${cfg.hosts[host]!.address}:${cfg.sccache_dist.server_port}"`);
      expect(toml).toContain(`build_dir = "${cfg.hosts[host]!.work}/tmp/sccache-dist-build"`);
    }
  });

  test("shared hosts run the server inside their slice, the factory does not", () => {
    for (const host of serverHosts(cfg)) {
      const unit = renderUnit("server", cfg, host, "/bin/sccache-dist");
      expect(unit.includes("Slice=")).toBe(Boolean(cfg.hosts[host]!.slice));
    }
    expect(renderUnit("scheduler", cfg, cfg.scheduler.host, "/bin/x")).not.toContain("Slice=");
  });

  test("client config points at the scheduler and redact hides secrets", () => {
    const client = renderClient(cfg, cfg.scheduler.host, secrets([]));
    expect(client).toContain(`scheduler_url = "${schedulerUrl(cfg)}"`);
    expect(redact(client)).not.toContain('"ct"');
  });
});
