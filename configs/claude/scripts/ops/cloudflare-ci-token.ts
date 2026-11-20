#!/usr/bin/env bun
// Mints an account-owned Cloudflare API token for CI (Workers, R2, tunnels read, zones) and stores it, with the account
// id, as GitHub Actions secrets of the given repositories. The token never touches stdout or disk.
//   CLOUDFLARE_API_TOKEN=<token able to create account tokens> bun scripts/ops/cloudflare-ci-token.ts <owner/repo>...
// The account is discovered from the token; permission groups are looked up by name.
const repos = process.argv.slice(2);
const tok = process.env.CLOUDFLARE_API_TOKEN;
if (!tok || !repos.length) {
  console.error(
    "usage: CLOUDFLARE_API_TOKEN=... bun scripts/ops/cloudflare-ci-token.ts <owner/repo>...",
  );
  process.exit(2);
}

interface Envelope<T> {
  success: boolean;
  errors: unknown[];
  result: T;
}
async function cf<T>(method: string, path: string, body?: unknown): Promise<T> {
  const r = await fetch(`https://api.cloudflare.com/client/v4${path}`, {
    method,
    headers: {
      authorization: `Bearer ${tok}`,
      "content-type": "application/json",
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const j = (await r.json()) as Envelope<T>;
  if (!j.success)
    throw new Error(`${method} ${path}: ${JSON.stringify(j.errors)}`);
  return j.result;
}

const [account] = await cf<{ id: string }[]>("GET", "/accounts");
if (!account) throw new Error("the token sees no account");
const groups = await cf<{ id: string; name: string }[]>(
  "GET",
  `/accounts/${account.id}/tokens/permission_groups`,
);
const pick = (names: string[]) =>
  names.map((n) => {
    const g = groups.find((x) => x.name === n);
    if (!g) throw new Error(`missing permission group ${n}`);
    return { id: g.id };
  });
const acct = `com.cloudflare.api.account.${account.id}`;
const tokenName = `github-ci-${new Date().toISOString().slice(0, 10)}`;
const minted = await cf<{ id: string; value: string }>(
  "POST",
  `/accounts/${account.id}/tokens`,
  {
    name: tokenName,
    // Least privilege and short-lived: re-run this script to rotate.
    expires_on: new Date(Date.now() + 90 * 86_400_000)
      .toISOString()
      .replace(/\.\d{3}Z$/, "Z"),
    policies: [
      {
        effect: "allow",
        resources: { [acct]: "*" },
        permission_groups: pick([
          "Workers Scripts Write",
          "Workers R2 Storage Read",
          "Cloudflare Tunnel Read",
          "Account Settings Read",
        ]),
      },
      {
        effect: "allow",
        resources: { [acct]: { "com.cloudflare.api.account.zone.*": "*" } },
        permission_groups: pick([
          "Zone Read",
          "Workers Routes Write",
          "DNS Read",
        ]),
      },
    ],
  },
);

let failed = 0;
await Promise.all(
  repos.flatMap((repo) =>
    (
      [
        ["CLOUDFLARE_API_TOKEN", minted.value],
        ["CLOUDFLARE_ACCOUNT_ID", account.id],
      ] as [string, string][]
    ).map(async ([k, v]) => {
      const p = Bun.spawn(["gh", "secret", "set", k, "-R", repo], {
        stdin: new Blob([v]),
        stdout: "ignore",
        stderr: "pipe",
      });
      const ok = (await p.exited) === 0;
      if (!ok) failed++;
      console.log(`${ok ? "set   " : "FAILED"} ${repo} ${k}`);
    }),
  ),
);
// Rotation: once every repository holds the new token, delete the older github-ci-* tokens.
if (!failed) {
  const all = await cf<{ id: string; name: string }[]>(
    "GET",
    `/accounts/${account.id}/tokens`,
  );
  for (const t of all) {
    if (!t.name.startsWith("github-ci-") || t.id === minted.id) continue;
    await cf("DELETE", `/accounts/${account.id}/tokens/${t.id}`);
    console.log(`deleted ${t.name}`);
  }
}
process.exit(failed ? 1 : 0);

export {};
