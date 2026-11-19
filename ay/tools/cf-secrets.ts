#!/usr/bin/env bun
// Centralizes, deduplicates, and synchronizes all credentials, .env files, and systemd-creds across the PC and the fleet hosts (ay.toml [hosts])
// into Cloudflare Secrets Store + Worker Secret Bindings + `.private/master-secrets.env` (0o600).
// NEVER prints or logs secret values — only key names, byte lengths, and sync counts.
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import {
  AyCfManifestSchema,
  captureLiveSnapshot,
  createCloudflareClient,
  type AyCfManifest,
} from "../packages/cf-control-plane/src/index.ts";
import { privateDir as privateRoot, readAy } from "./local.ts";

const root = resolve(import.meta.dir, "..");
const argv = Bun.argv.slice(2);
const applyMode = argv.includes("--apply") || argv.includes("--sync");
const collectVps = !argv.includes("--no-vps");

const SECRET_KEY_RE =
  /(TOKEN|SECRET|PASSWORD|PASS|API_KEY|PRIVATE_KEY|AUTH|CREDENTIAL|DATABASE_URL|ADMIN_KEY|NUGET_API_KEY|ACCOUNT_ID|CLIENT_ID|ORG_ID|ACCESS_KEY|WEBHOOK|MASTER_KEY|RESEND_KEY|SA_KEY|SESSION_KEY|ENCRYPTION_KEY|PUBLISHABLE_KEY|ANON_KEY|PUBLIC_KEY|MOT_DE_PASSE)/i;

const EXCLUDE_PREFIX_RE =
  /^(PATH|HOME|USER|SHELL|PWD|OLDPWD|TERM|LANG|LC_|PORT|HOST$|NODE_ENV|BUN$|DENO$|NODE$|CARGO_HOME|CARGO_TARGET|RUSTUP|RUSTC|CC$|CXX$|CMAKE_|CUDA_|VULKAN_|GOPATH|GOPROXY|LUA_|ANDROID_|GHIDRA_|PYGHIDRA_|WT_|PROCESSOR_|STARSHIP_|ATUIN_|ANTIGRAVITY_|SCCACHE_)/;

export function parseDotEnvText(content: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const rawLine of content.split(/\r?\n/)) {
    const trimmed = rawLine.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const norm = trimmed.startsWith("export ") ? trimmed.slice(7).trim() : trimmed;
    const eq = norm.indexOf("=");
    if (eq <= 0) continue;
    const key = norm.slice(0, eq).trim();
    if (!/^[A-Z][A-Z0-9_]{2,127}$/.test(key)) continue;
    let val = norm.slice(eq + 1).trim();
    if ((val.startsWith('"') && val.endsWith('"')) || (val.startsWith("'") && val.endsWith("'"))) {
      val = val.slice(1, -1);
    } else {
      const commentIdx = val.search(/\s+#/);
      if (commentIdx >= 0) val = val.slice(0, commentIdx).trim();
    }
    if (val) out[key] = val;
  }
  return out;
}

export function collectAllCredentials(
  manifest: AyCfManifest,
  opts: { withVps: boolean },
): Map<string, { value: string; sources: Set<string> }> {
  const catalog = new Map<string, { value: string; sources: Set<string> }>();
  const aliases = manifest.secrets_store?.aliases ?? {};

  const record = (rawKey: string, rawVal: string, source: string) => {
    const k = rawKey.trim();
    const v = rawVal.trim();
    if (!k || !v || !/^[A-Z][A-Z0-9_]{2,127}$/.test(k)) return;
    if (EXCLUDE_PREFIX_RE.test(k)) return;
    if (!SECRET_KEY_RE.test(k)) return;

    const canonical = aliases[k] ?? k;
    for (const targetKey of new Set([k, canonical])) {
      const existing = catalog.get(targetKey);
      if (!existing) {
        catalog.set(targetKey, { value: v, sources: new Set([source]) });
      } else {
        existing.sources.add(source);
      }
    }
  };

  // 1. Local Bun.env
  for (const [k, v] of Object.entries(Bun.env)) {
    if (v) record(k, v, "pc:env");
  }

  // 2. Local C:\src/*/.env files
  const srcRoot = resolve(root, "..");
  if (existsSync(srcRoot)) {
    for (const d of readdirSync(srcRoot, { withFileTypes: true })) {
      if (!d.isDirectory()) continue;
      for (const rel of [".env", ".env.local", ".env.production", ".private/master-secrets.env"]) {
        const p = join(srcRoot, d.name, rel);
        if (existsSync(p)) {
          for (const [k, v] of Object.entries(parseDotEnvText(readFileSync(p, "utf8")))) {
            record(k, v, `pc:${d.name}`);
          }
        }
      }
    }
  }

  // 3. Remote fleet hosts (ay.toml [hosts])
  if (opts.withVps) {
    const remoteScript = `
for f in $(find /root /home /opt /srv /etc -maxdepth 4 \\( -name '.env' -o -name '.env.production' -o -name '.env.local' -o -name '*.env' \\) 2>/dev/null); do
  if [ -r "$f" ]; then echo "___FILE:$f"; cat "$f" 2>/dev/null || true; fi
done
for cdir in /etc/credstore.encrypted /root/.config/credstore.encrypted /home/*/.config/credstore.encrypted; do
  if [ -d "$cdir" ]; then
    for cf in "$cdir"/*; do
      [ -f "$cf" ] || continue
      cname=$(basename "$cf")
      echo "___CRED:$cdir/$cname"
      systemd-creds decrypt --name="$cname" "$cf" - 2>/dev/null || systemd-creds --user decrypt --name="$cname" "$cf" - 2>/dev/null || true
      echo ""
    done
  fi
done
`;
    for (const host of Object.values((readAy(root).hosts ?? {}) as Record<string, string>)) {
      const p = Bun.spawnSync(["ssh", "-o", "ConnectTimeout=8", "-o", "BatchMode=yes", host, remoteScript]);
      if (p.exitCode !== 0) continue;
      const out = new TextDecoder().decode(p.stdout);
      const blocks = out.split(/^___(FILE|CRED):/m);
      for (let i = 1; i < blocks.length; i += 2) {
        const kind = blocks[i];
        const rest = blocks[i + 1] ?? "";
        const nl = rest.indexOf("\n");
        if (nl < 0) continue;
        const path = rest.slice(0, nl).trim();
        const body = rest.slice(nl + 1);
        if (kind === "FILE") {
          for (const [k, v] of Object.entries(parseDotEnvText(body))) {
            record(k, v, `${host}:${path}`);
          }
        } else if (kind === "CRED") {
          const cname = path.split("/").pop() ?? "";
          const val = body.trim();
          if (val && !val.includes("\n")) {
            record(cname.toUpperCase().replace(/[^A-Z0-9_]/g, "_"), val, `${host}:credstore`);
          }
        }
      }
    }
  }

  // Propagate reverse aliases so Worker secret contracts (e.g. DEPLOY_TOKEN, NEON_DATABASE_URL, OWNER_GOOGLE_ACCOUNT) are always populated
  for (const [aliasKey, canonicalKey] of Object.entries(aliases)) {
    if (!catalog.has(aliasKey) && catalog.has(canonicalKey)) {
      const c = catalog.get(canonicalKey)!;
      catalog.set(aliasKey, { value: c.value, sources: new Set([...c.sources, `alias:${canonicalKey}`]) });
    }
  }
  if (!catalog.has("OAUTH_SECRET") && catalog.has("BETTER_AUTH_SECRET")) {
    const c = catalog.get("BETTER_AUTH_SECRET")!;
    catalog.set("OAUTH_SECRET", { value: c.value, sources: new Set([...c.sources, "alias:BETTER_AUTH_SECRET"]) });
  }
  if (!catalog.has("OWNER_GOOGLE_ACCOUNT") && catalog.has("AUTH_EMAIL_FROM")) {
    const c = catalog.get("AUTH_EMAIL_FROM")!;
    catalog.set("OWNER_GOOGLE_ACCOUNT", { value: c.value, sources: new Set([...c.sources, "alias:AUTH_EMAIL_FROM"]) });
  }

  return catalog;
}

interface CfStoreSecretItem {
  id: string;
  name: string;
  status: string;
}

async function listSecretsStoreSecrets(
  apiToken: string,
  accountId: string,
  storeId: string,
): Promise<CfStoreSecretItem[]> {
  const res = await fetch(
    `https://api.cloudflare.com/client/v4/accounts/${accountId}/secrets_store/stores/${storeId}/secrets?per_page=100`,
    { headers: { Authorization: `Bearer ${apiToken}` } },
  );
  const data = (await res.json()) as { result?: CfStoreSecretItem[] };
  return data.result ?? [];
}

async function upsertSecretsStoreSecret(
  apiToken: string,
  accountId: string,
  storeId: string,
  existingId: string | undefined,
  name: string,
  value: string,
  comment: string,
): Promise<{ ok: boolean; action: "created" | "updated" | "failed"; error?: string }> {
  if (existingId) {
    const res = await fetch(
      `https://api.cloudflare.com/client/v4/accounts/${accountId}/secrets_store/stores/${storeId}/secrets/${existingId}`,
      {
        method: "PATCH",
        headers: {
          Authorization: `Bearer ${apiToken}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ value, scopes: ["workers"], comment }),
      },
    );
    const j = (await res.json()) as { success?: boolean; errors?: Array<{ message?: string }> };
    return j.success
      ? { ok: true, action: "updated" }
      : { ok: false, action: "failed", error: j.errors?.[0]?.message ?? `HTTP ${res.status}` };
  }

  const res = await fetch(
    `https://api.cloudflare.com/client/v4/accounts/${accountId}/secrets_store/stores/${storeId}/secrets`,
    {
      method: "POST",
      headers: {
        Authorization: `Bearer ${apiToken}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify([{ name, value, scopes: ["workers"], comment }]),
    },
  );
  const j = (await res.json()) as { success?: boolean; errors?: Array<{ message?: string }> };
  return j.success
    ? { ok: true, action: "created" }
    : { ok: false, action: "failed", error: j.errors?.[0]?.message ?? `HTTP ${res.status}` };
}

async function putWorkerSecretBinding(
  apiToken: string,
  accountId: string,
  scriptName: string,
  secretName: string,
  secretValue: string,
): Promise<boolean> {
  const res = await fetch(
    `https://api.cloudflare.com/client/v4/accounts/${accountId}/workers/scripts/${scriptName}/secrets`,
    {
      method: "PUT",
      headers: {
        Authorization: `Bearer ${apiToken}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        name: secretName,
        text: secretValue,
        type: "secret_text",
      }),
    },
  );
  const j = (await res.json()) as { success?: boolean };
  return Boolean(j.success);
}

if (import.meta.main) {
  const manifest = AyCfManifestSchema.parse(Bun.TOML.parse(readFileSync(join(root, "packages/cf-control-plane/cf.toml"), "utf8")));
  const localCfg = Bun.TOML.parse(readFileSync(join(root, "cf.local.toml"), "utf8")) as {
    cloudflare?: { account_id?: string };
  };
  const accountId = localCfg.cloudflare?.account_id ?? Bun.env.CLOUDFLARE_ACCOUNT_ID;
  if (!accountId) throw new Error("Run `bun tools/cf-bootstrap-local.ts` first");

  const client = createCloudflareClient();
  const apiToken = client.apiToken ?? Bun.env.CLOUDFLARE_API_TOKEN ?? "";
  const snap = await captureLiveSnapshot(client, accountId);

  // 1. Audit Worker secret bindings
  let missingWorkerSecrets = 0;
  const catalog = applyMode || argv.includes("--inventory")
    ? collectAllCredentials(manifest, { withVps: collectVps })
    : new Map<string, { value: string; sources: Set<string> }>();

  if (catalog.size > 0) {
    const privateDir = privateRoot(root);
    mkdirSync(privateDir, { recursive: true });
    const sortedEntries = [...catalog.entries()].sort((a, b) => a[0].localeCompare(b[0]));
    const envLines = [
      `# Auto-generated centralized master secrets vault (${new Date().toISOString()}). GIT-IGNORED (0o600).`,
      ...sortedEntries.map(([k, info]) => `${k}=${JSON.stringify(info.value)}`),
      "",
    ];
    writeFileSync(join(privateDir, "master-secrets.env"), envLines.join("\n"), { mode: 0o600 });

    const manifestSummary = sortedEntries.map(([k, info]) => ({
      name: k,
      length: info.value.length,
      sha256_prefix: new Bun.CryptoHasher("sha256").update(info.value).digest("hex").slice(0, 12),
      sources: [...info.sources],
    }));
    writeFileSync(
      join(privateDir, "secrets-inventory.json"),
      JSON.stringify({ capturedAt: new Date().toISOString(), count: manifestSummary.length, secrets: manifestSummary }, null, 2) + "\n",
      { mode: 0o600 },
    );
  }

  let workerSecretsSynced = 0;
  for (const [name, contract] of Object.entries(manifest.workers)) {
    const live = snap.workers.find((w) => w.id === name);
    if (!live) {
      console.error(`MISSING WORKER: ${name}`);
      missingWorkerSecrets++;
      continue;
    }
    const present = new Set(live.secrets);
    const missing = contract.secrets.filter((s) => !present.has(s));
    if (missing.length && applyMode) {
      for (const secName of missing) {
        const entry = catalog.get(secName);
        if (entry) {
          const ok = await putWorkerSecretBinding(apiToken, accountId, name, secName, entry.value);
          if (ok) {
            present.add(secName);
            workerSecretsSynced++;
          }
        }
      }
    }
    const stillMissing = contract.secrets.filter((s) => !present.has(s));
    if (stillMissing.length) {
      missingWorkerSecrets += stillMissing.length;
      console.error(`${name}: missing secret bindings -> ${stillMissing.join(", ")}`);
    } else {
      console.log(`${name}: ${present.size} secret(s) verified`);
    }
  }

  // 2. Centralize into Cloudflare Secrets Store (`default-secrets-store`)
  const storeSpec = manifest.secrets_store;
  if (storeSpec) {
    const storesRes = await fetch(`https://api.cloudflare.com/client/v4/accounts/${accountId}/secrets_store/stores`, {
      headers: { Authorization: `Bearer ${apiToken}` },
    });
    const storesData = (await storesRes.json()) as { result?: Array<{ id: string; name: string }> };
    let store = (storesData.result ?? []).find((s) => s.name === storeSpec.store_name) ?? storesData.result?.[0];
    if (!store && applyMode) {
      const createRes = await fetch(`https://api.cloudflare.com/client/v4/accounts/${accountId}/secrets_store/stores`, {
        method: "POST",
        headers: { Authorization: `Bearer ${apiToken}`, "Content-Type": "application/json" },
        body: JSON.stringify({ name: storeSpec.store_name }),
      });
      const createData = (await createRes.json()) as { result?: { id: string; name: string } };
      store = createData.result;
    }

    if (store) {
      const existingList = await listSecretsStoreSecrets(apiToken, accountId, store.id);
      const existingByName = new Map(existingList.map((s) => [s.name, s.id]));

      if (applyMode && catalog.size > 0) {
        // Prioritize required_keys up to quota_limit - 1, then pack any remaining into bundle_overflow_key
        const maxIndividual = storeSpec.quota_limit - 1;
        const prioritized: string[] = [];
        for (const reqKey of storeSpec.required_keys) {
          if (catalog.has(reqKey) && !prioritized.includes(reqKey) && prioritized.length < maxIndividual) {
            prioritized.push(reqKey);
          }
        }
        for (const k of catalog.keys()) {
          if (!prioritized.includes(k) && prioritized.length < maxIndividual) {
            prioritized.push(k);
          }
        }

        let created = 0;
        let updated = 0;
        for (const key of prioritized) {
          const info = catalog.get(key)!;
          const res = await upsertSecretsStoreSecret(
            apiToken,
            accountId,
            store.id,
            existingByName.get(key),
            key,
            info.value,
            `cf-secrets centralized (${[...info.sources].slice(0, 2).join(",")})`,
          );
          if (res.ok) {
            if (res.action === "created") created++;
            else updated++;
          }
        }

        // Pack overflow + alias dictionary into bundle_overflow_key
        const overflowObj: Record<string, string> = {};
        for (const [k, info] of catalog.entries()) {
          if (!prioritized.includes(k)) {
            overflowObj[k] = info.value;
          }
        }
        const bundleJson = JSON.stringify(overflowObj);
        const bundleRes = await upsertSecretsStoreSecret(
          apiToken,
          accountId,
          store.id,
          existingByName.get(storeSpec.bundle_overflow_key),
          storeSpec.bundle_overflow_key,
          bundleJson,
          `cf-secrets overflow bundle (${Object.keys(overflowObj).length} keys)`,
        );
        if (bundleRes.ok) {
          if (bundleRes.action === "created") created++;
          else updated++;
        }

        const afterList = await listSecretsStoreSecrets(apiToken, accountId, store.id);
        console.log(
          `Secrets Store (${store.name}): total=${afterList.length}/${storeSpec.quota_limit} (created=${created}, updated=${updated}, catalog=${catalog.size}, worker_bindings_synced=${workerSecretsSynced})`,
        );
      } else {
        console.log(
          `Secrets Store (${store.name}): ${existingList.length}/${storeSpec.quota_limit} active secret(s) (pass --apply to centralize all ${storeSpec.required_keys.length} declared secrets)`,
        );
      }
    }
  }

  process.exit(missingWorkerSecrets ? 1 : 0);
}
