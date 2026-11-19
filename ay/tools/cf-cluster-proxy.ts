#!/usr/bin/env bun
// Renders production OVH edge_proxy (Pingora/Nginx), Hostinger origin nftables, Mullvad UID routes, and cloudflared configs into .private/generated/.
import { join, resolve } from "node:path";
import { AyCfManifestSchema, generateClusterConfigs } from "../packages/cf-control-plane/src/index.ts";
import { privateDir as privateRoot } from "./local.ts";

const root = resolve(import.meta.dir, "..");
const manifest = AyCfManifestSchema.parse(Bun.TOML.parse(await Bun.file(join(root, "packages/cf-control-plane/cf.toml")).text()));
const localFile = Bun.file(join(root, "cf.local.toml"));
let tunnelIds: Record<string, string> = {};
if (await localFile.exists()) {
  const local = Bun.TOML.parse(await localFile.text()) as {
    cloudflare?: { tunnels?: Record<string, string> };
  };
  tunnelIds = local.cloudflare?.tunnels ?? {};
}

const outDir = join(privateRoot(root), "generated");
const cfgs = generateClusterConfigs(manifest, tunnelIds);

await Promise.all([
  Bun.write(join(outDir, "pingora-edge-proxy.yaml"), cfgs.pingoraYaml),
  Bun.write(join(outDir, "nginx-edge-proxy.conf"), cfgs.nginxEdgeProxyConf),
  Bun.write(join(outDir, "nftables-origin.conf"), cfgs.nftablesOriginConf),
  Bun.write(join(outDir, "mullvad-uid-routes.sh"), cfgs.mullvadUidScript),
  Bun.write(join(outDir, "cloudflared-ha.yaml"), cfgs.cloudflaredHaYaml),
]);

console.log(`Generated 5 cluster/proxy/VPN/tunnel configs in ${outDir}`);
