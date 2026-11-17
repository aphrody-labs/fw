// Publishing, derived from [serve] of acceptance.toml: the Worker config, the tunnel config, and the checks that decide
// whether the hostname can be served. Without --apply nothing leaves this machine (wrangler runs with --dry-run).
//   bun testing/acceptance.ts deploy            plan: write out/acceptance/deploy/{wrangler.jsonc,tunnel.yml}, bundle the Worker
//   bun testing/acceptance.ts deploy --apply    wrangler deploy (the route and the secret), nothing else
// The tunnel and its DNS record are not created here: the zone's DNS is managed elsewhere and the plan says what it needs.
import { join } from "node:path";
import { manifest, root } from "../acceptance.ts";
import { spawn, mkdirp } from "./sys.ts";

export async function deploy(apply: boolean) {
  const { host, origin, port } = manifest.serve;
  const zone = host.split(".").slice(-2).join(".");
  const dir = join(root, "out/acceptance/deploy");
  await mkdirp(dir);
  const wrangler = {
    $schema: "node_modules/wrangler/config-schema.json",
    name: `browser-acceptance`,
    main: Bun.fileURLToPath(new URL("./worker.ts", import.meta.url)),
    compatibility_date: new Date().toISOString().slice(0, 10),
    routes: [{ pattern: `${host}/*`, zone_name: zone }],
    vars: { ORIGIN: origin },
    observability: { enabled: true },
  };
  await Bun.write(join(dir, "wrangler.jsonc"), JSON.stringify(wrangler, null, 2) + "\n");
  await Bun.write(
    join(dir, "tunnel.yml"),
    `# cloudflared ingress for the dashboard: \`cloudflared tunnel --config tunnel.yml run <tunnel>\`\ningress:\n  - hostname: ${origin}\n    service: http://127.0.0.1:${port}\n  - service: http_status:404\n`,
  );
  const wr = Bun.which("wrangler");
  if (!wr) throw new Error("wrangler not found (bun add -g wrangler)");
  const code = await spawn([wr, "deploy", "--config", join(dir, "wrangler.jsonc"), ...(apply ? [] : ["--dry-run"])], {
    cwd: dir,
    stdout: "inherit",
    stderr: "inherit",
  }).exited;
  const [route, origin_dns] = await Promise.all(
    [host, origin].map((name) =>
      fetch(`https://cloudflare-dns.com/dns-query?name=${name}&type=A`, { headers: { accept: "application/dns-json" } })
        .then((r) => r.json() as Promise<{ Answer?: unknown[] }>)
        .then((j) => j.Answer?.length ?? 0),
    ),
  );
  console.log(
    `${host}: ${route ? "has an address record (the Worker route can serve it)" : "no address record: the route has nothing to attach to"}`,
  );
  console.log(
    `${origin}: ${origin_dns ? "has an address record" : "no record yet: the tunnel needs a CNAME <tunnel-id>.cfargotunnel.com (proxied) before the Worker can reach the origin"}`,
  );
  console.log(
    `secret: set ACCEPTANCE_KEY on both sides (wrangler secret put ACCEPTANCE_KEY --config ${join(dir, "wrangler.jsonc")}; same value in the environment of \`serve\`)`,
  );
  return code;
}
