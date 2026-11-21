import { describe, expect, test } from "bun:test";
import {
  addSshAlias,
  loadConfig,
  nodeName,
  place,
  redact,
  renderClient,
  renderMeshBlock,
  renderScheduler,
  renderServer,
  renderSlice,
  renderUnit,
  schedulerUrl,
  serverHosts,
  type Measure,
  type Secrets,
} from "./build-cluster.ts";

const secrets = (hosts: string[]): Secrets => ({ clientToken: "ct", hs256Key: "key", serverTokens: Object.fromEntries(hosts.map((h) => [h, `tok-${h}`])) });
const measure = (node: string, vcpu: number, load1: number, load5: number, memAvailGiB: number, diskFreeGiB = 100): Measure => ({ node, vcpu, load1, load5, memAvailGiB, diskFreeGiB });

describe("build-cluster", async () => {
  const cfg = await loadConfig();

  test("node names are derived from the prefix and the index, never from a role", () => {
    for (const [name, host] of Object.entries(cfg.hosts)) {
      expect(name).toBe(nodeName(cfg, host));
      expect(host.ssh).toBe(name);
      expect(host.roles).toEqual(cfg.hosts[Object.keys(cfg.hosts)[0]!]!.roles);
    }
  });

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

  test("every server runs in the same slice, sized from the node's RAM", () => {
    for (const host of serverHosts(cfg)) expect(renderUnit("server", cfg, host, "/bin/sccache-dist")).toContain(`Slice=${cfg.policy.build_slice}`);
    expect(renderUnit("scheduler", cfg, cfg.scheduler.host, "/bin/x")).not.toContain("Slice=");
    expect(renderSlice(cfg, 40 * 2 ** 30)).toContain("MemoryHigh=16G");
  });

  test("client config points at the scheduler and redact hides secrets", () => {
    const client = renderClient(cfg, cfg.scheduler.host, secrets([]));
    expect(client).toContain(`scheduler_url = "${schedulerUrl(cfg)}"`);
    expect(redact(client)).not.toContain('"ct"');
  });

  test("the mesh block names every node and keeps lines it does not own", () => {
    const block = renderMeshBlock(cfg, ["10.200.77.1 fr1.mesh", "10.200.77.3 spare.mesh", "10.200.77.5 prd.mesh"]);
    expect(block).toContain("10.200.77.1 n1.mesh fr1.mesh");
    expect(block).toContain("10.200.77.5 n2.mesh prd.mesh");
    expect(block).toContain("10.200.77.3 spare.mesh");
    expect(block.filter((l) => l.startsWith("10.200.77.1 ")).length).toBe(1);
  });

  test("ssh aliases are added beside the old ones, once", () => {
    const text = 'Match host fr1,f1p exec "x"\n  HostName 1\nHost f1p vps fr1\n  User u\nHost other\n';
    const first = addSshAlias(text, "n1", ["fr1", "n1"]);
    expect(first.changed).toBe(2);
    expect(first.text).toContain("Host f1p vps fr1 n1");
    expect(first.text).toContain("Match host fr1,f1p,n1 exec");
    expect(first.text).toContain("Host other\n");
    expect(addSshAlias(first.text, "n1", ["fr1", "n1"]).changed).toBe(0);
  });

  test("placement picks the node with the most free cores that has the memory and disk", () => {
    const loaded = measure("n1", 12, 24, 20, 20);
    const idle = measure("n2", 8, 3, 3, 12);
    expect(place([loaded, idle]).node).toBe("n2");
    expect(place([measure("n1", 12, 2, 2, 20), idle]).node).toBe("n1");
    expect(place([measure("n1", 12, 2, 2, 4), idle], { memGiB: 8 }).node).toBe("n2");
    expect(() => place([loaded, idle], { memGiB: 64 })).toThrow("burst");
  });
});
