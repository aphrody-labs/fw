#!/usr/bin/env bun
// Quota-aware routing and ceiling monitor across Cloudflare (Workers/R2/KV/D1),
// GitHub Actions (minutes and 10 GiB per-repo cache), and the VPS fleet roles (prod/build/test).
//   bun tools/cf-quota.ts [--live [--with-ssh [--no-gh [--tier free|paid [--json
// Exit codes: 0 = ok, 1 = warn (soft threshold / offload recommended), 2 = critical (ceiling hit).
import { readAy, repoRoot } from "./local.ts";

export type CloudflareTier = "free" | "paid";
export type HostRole = "prod" | "build" | "test";
export type Severity = "ok" | "warn" | "critical";

export interface CloudflarePolicy {
  tier: CloudflareTier;
  workersFreeDailyRequests: number;
  workersPaidSoftDailyRequests: number;
  workersSoftCap: number;
  r2StorageSoftBytes: number;
  r2ClassASoftMonthly: number;
  r2ClassBSoftMonthly: number;
  kvMaxNamespaces: number;
  kvDailyReadsCeiling: number;
  kvDailyWritesCeiling: number;
  d1MaxDatabases: number;
  d1StorageCeilingBytes: number;
  d1DailyRowsReadCeiling: number;
  d1DailyRowsWrittenCeiling: number;
  warnRatio: number;
  criticalRatio: number;
}

export interface GithubActionsPolicy {
  monthlyMinutesCeiling: number;
  repoCacheCeilingBytes: number;
  offloadRatio: number;
  criticalRatio: number;
}

export interface VpsFleetPolicy {
  diskWarnPct: number;
  diskCriticalPct: number;
  loadPerCpuWarn: number;
  loadPerCpuCritical: number;
}

export interface QuotaPolicy {
  cloudflare: CloudflarePolicy;
  githubActions: GithubActionsPolicy;
  vpsFleet: VpsFleetPolicy;
}

export interface CloudflareTelemetry {
  tier?: CloudflareTier;
  workersCount: number;
  dailyWorkerRequests: number;
  dailyWorkerErrors?: number;
  r2StorageBytes: number;
  r2ClassAMonthly: number;
  r2ClassBMonthly: number;
  kvNamespaces?: number;
  kvDailyReads?: number;
  kvDailyWrites?: number;
  d1Databases?: number;
  d1StorageBytes?: number;
  d1DailyRowsRead?: number;
  d1DailyRowsWritten?: number;
}

export interface RepoCacheUsage {
  repo: string;
  cacheBytes: number;
}

export interface GithubActionsTelemetry {
  monthlyMinutesUsed: number;
  monthlyMinutesLimit?: number;
  repos: RepoCacheUsage[];
}

export interface VpsHostTelemetry {
  role: HostRole;
  reachable: boolean;
  diskUsedPct?: number;
  load1m?: number;
  cpus?: number;
}

export interface QuotaTelemetry {
  cloudflare: CloudflareTelemetry;
  githubActions: GithubActionsTelemetry;
  vpsFleet: VpsHostTelemetry[];
}

export interface QuotaFinding {
  domain: "cloudflare" | "github" | "fleet";
  metric: string;
  severity: Severity;
  used: number;
  limit: number;
  ratio: number;
  detail: string;
}

export interface RoutingDecision {
  edgeStatic: "cloudflare-r2" | "prod-origin" | "test-standby";
  edgeCompute: "cloudflare-workers" | "prod-origin" | "test-standby";
  ciRunner: "github-hosted" | "self-hosted-build" | "self-hosted-test";
  buildTarget: "build" | "test" | "github-hosted";
  reasons: string[];
}

export interface QuotaEvaluation {
  ok: boolean;
  severity: Severity;
  exitCode: 0 | 1 | 2;
  tier: CloudflareTier;
  findings: QuotaFinding[];
  routing: RoutingDecision;
  summary: string;
}

const GIB = 1024 ** 3;

export const DEFAULT_QUOTA_POLICY: QuotaPolicy = {
  cloudflare: {
    tier: "free",
    workersFreeDailyRequests: 100_000,
    workersPaidSoftDailyRequests: 1_000_000,
    workersSoftCap: 15,
    r2StorageSoftBytes: 10 * GIB,
    r2ClassASoftMonthly: 1_000_000,
    r2ClassBSoftMonthly: 10_000_000,
    kvMaxNamespaces: 100,
    kvDailyReadsCeiling: 100_000,
    kvDailyWritesCeiling: 1_000,
    d1MaxDatabases: 10,
    d1StorageCeilingBytes: 5 * GIB,
    d1DailyRowsReadCeiling: 5_000_000,
    d1DailyRowsWrittenCeiling: 100_000,
    warnRatio: 0.75,
    criticalRatio: 0.9,
  },
  githubActions: {
    monthlyMinutesCeiling: 2_000,
    repoCacheCeilingBytes: 10 * GIB,
    offloadRatio: 0.75,
    criticalRatio: 0.9,
  },
  vpsFleet: {
    diskWarnPct: 75,
    diskCriticalPct: 88,
    loadPerCpuWarn: 0.85,
    loadPerCpuCritical: 1.25,
  },
};

export type PartialQuotaPolicy = {
  cloudflare?: Partial<CloudflarePolicy>;
  githubActions?: Partial<GithubActionsPolicy>;
  vpsFleet?: Partial<VpsFleetPolicy>;
};

export function mergePolicy(override?: PartialQuotaPolicy): QuotaPolicy {
  return {
    cloudflare: { ...DEFAULT_QUOTA_POLICY.cloudflare, ...override?.cloudflare },
    githubActions: { ...DEFAULT_QUOTA_POLICY.githubActions, ...override?.githubActions },
    vpsFleet: { ...DEFAULT_QUOTA_POLICY.vpsFleet, ...override?.vpsFleet },
  };
}

const RANK: Record<Severity, number> = { ok: 0, warn: 1, critical: 2 };
const maxSev = (a: Severity, b: Severity): Severity => (RANK[a] >= RANK[b] ? a : b);

function checkRatio(
  domain: QuotaFinding["domain"],
  metric: string,
  used: number,
  limit: number,
  warnRatio: number,
  criticalRatio: number,
  unit = "",
  capAtWarn = false,
): QuotaFinding {
  const ratio = limit > 0 ? used / limit : 0;
  let severity: Severity = "ok";
  if (ratio >= criticalRatio) severity = capAtWarn ? "warn" : "critical";
  else if (ratio >= warnRatio) severity = "warn";
  const pct = (ratio * 100).toFixed(1);
  const suffix = unit ? ` ${unit}` : "";
  return {
    domain,
    metric,
    severity,
    used,
    limit,
    ratio: Number(ratio.toFixed(4)),
    detail: `${metric}: ${used.toLocaleString("en-US")}${suffix} / ${limit.toLocaleString("en-US")}${suffix} (${pct}%)`,
  };
}

function hostHealthy(host: VpsHostTelemetry | undefined, policy: VpsFleetPolicy): boolean {
  if (!host || !host.reachable) return false;
  if ((host.diskUsedPct ?? 0) >= policy.diskCriticalPct) return false;
  const cpus = host.cpus && host.cpus > 0 ? host.cpus : 1;
  if (host.load1m !== undefined && host.load1m / cpus >= policy.loadPerCpuCritical) return false;
  return true;
}

/** Pure quota-aware routing & ceiling evaluation across Cloudflare, GitHub Actions, and VPS fleet roles. */
export function evaluateQuotaRouting(
  telemetry: QuotaTelemetry,
  policyOverride?: PartialQuotaPolicy,
): QuotaEvaluation {
  const policy = mergePolicy(policyOverride);
  const cfPol = policy.cloudflare;
  const ghPol = policy.githubActions;
  const fleetPol = policy.vpsFleet;
  const cf = telemetry.cloudflare;
  const tier: CloudflareTier = cf.tier ?? cfPol.tier;
  const findings: QuotaFinding[] = [];

  const reqCeiling = tier === "paid" ? cfPol.workersPaidSoftDailyRequests : cfPol.workersFreeDailyRequests;
  findings.push(
    checkRatio(
      "cloudflare",
      tier === "paid" ? "workers_daily_requests_soft" : "workers_free_daily_requests",
      cf.dailyWorkerRequests,
      reqCeiling,
      cfPol.warnRatio,
      cfPol.criticalRatio,
      "req/day",
      tier === "paid",
    ),
  );
  if ((cf.dailyWorkerErrors ?? 0) > 0 && cf.dailyWorkerRequests > 0) {
    const errRate = (cf.dailyWorkerErrors ?? 0) / cf.dailyWorkerRequests;
    findings.push({
      domain: "cloudflare",
      metric: "workers_error_rate",
      severity: errRate >= 0.05 ? "critical" : errRate >= 0.01 ? "warn" : "ok",
      used: cf.dailyWorkerErrors ?? 0,
      limit: cf.dailyWorkerRequests,
      ratio: Number(errRate.toFixed(4)),
      detail: `workers_error_rate: ${cf.dailyWorkerErrors} errors / ${cf.dailyWorkerRequests} req (${(errRate * 100).toFixed(2)}%)`,
    });
  }
  findings.push(
    checkRatio("cloudflare", "workers_count", cf.workersCount, cfPol.workersSoftCap, 0.85, 1.0, "scripts", true),
  );
  findings.push(
    checkRatio(
      "cloudflare",
      "r2_storage_soft",
      cf.r2StorageBytes,
      cfPol.r2StorageSoftBytes,
      cfPol.warnRatio,
      1.0,
      "bytes",
      tier === "paid",
    ),
  );
  findings.push(
    checkRatio(
      "cloudflare",
      "r2_class_a_monthly",
      cf.r2ClassAMonthly,
      cfPol.r2ClassASoftMonthly,
      cfPol.warnRatio,
      1.0,
      "ops/mo",
      tier === "paid",
    ),
  );
  findings.push(
    checkRatio(
      "cloudflare",
      "r2_class_b_monthly",
      cf.r2ClassBMonthly,
      cfPol.r2ClassBSoftMonthly,
      cfPol.warnRatio,
      1.0,
      "ops/mo",
      tier === "paid",
    ),
  );
  if (cf.kvNamespaces !== undefined) {
    findings.push(checkRatio("cloudflare", "kv_namespaces", cf.kvNamespaces, cfPol.kvMaxNamespaces, cfPol.warnRatio, cfPol.criticalRatio));
  }
  if (cf.kvDailyReads !== undefined) {
    findings.push(
      checkRatio("cloudflare", "kv_daily_reads", cf.kvDailyReads, cfPol.kvDailyReadsCeiling, cfPol.warnRatio, cfPol.criticalRatio, "reads/day", tier === "paid"),
    );
  }
  if (cf.kvDailyWrites !== undefined) {
    findings.push(
      checkRatio("cloudflare", "kv_daily_writes", cf.kvDailyWrites, cfPol.kvDailyWritesCeiling, cfPol.warnRatio, cfPol.criticalRatio, "writes/day", tier === "paid"),
    );
  }
  if (cf.d1Databases !== undefined) {
    findings.push(checkRatio("cloudflare", "d1_databases", cf.d1Databases, cfPol.d1MaxDatabases, cfPol.warnRatio, cfPol.criticalRatio));
  }
  if (cf.d1StorageBytes !== undefined) {
    findings.push(checkRatio("cloudflare", "d1_storage", cf.d1StorageBytes, cfPol.d1StorageCeilingBytes, cfPol.warnRatio, cfPol.criticalRatio, "bytes"));
  }
  if (cf.d1DailyRowsRead !== undefined) {
    findings.push(
      checkRatio("cloudflare", "d1_daily_rows_read", cf.d1DailyRowsRead, cfPol.d1DailyRowsReadCeiling, cfPol.warnRatio, cfPol.criticalRatio, "rows/day", tier === "paid"),
    );
  }
  if (cf.d1DailyRowsWritten !== undefined) {
    findings.push(
      checkRatio("cloudflare", "d1_daily_rows_written", cf.d1DailyRowsWritten, cfPol.d1DailyRowsWrittenCeiling, cfPol.warnRatio, cfPol.criticalRatio, "rows/day", tier === "paid"),
    );
  }

  const ghLimit = telemetry.githubActions.monthlyMinutesLimit ?? ghPol.monthlyMinutesCeiling;
  const ghMinutesFinding = checkRatio(
    "github",
    "actions_monthly_minutes",
    telemetry.githubActions.monthlyMinutesUsed,
    ghLimit,
    ghPol.offloadRatio,
    ghPol.criticalRatio,
    "min",
  );
  findings.push(ghMinutesFinding);

  let worstCacheRatio = 0;
  for (const r of telemetry.githubActions.repos) {
    const f = checkRatio(
      "github",
      `actions_cache:${r.repo}`,
      r.cacheBytes,
      ghPol.repoCacheCeilingBytes,
      ghPol.offloadRatio,
      ghPol.criticalRatio,
      "bytes",
    );
    worstCacheRatio = Math.max(worstCacheRatio, f.ratio);
    findings.push(f);
  }

  const byRole = new Map<HostRole, VpsHostTelemetry>();
  for (const h of telemetry.vpsFleet) {
    byRole.set(h.role, h);
    if (!h.reachable) {
      findings.push({
        domain: "fleet",
        metric: `host_reachable:${h.role}`,
        severity: h.role === "prod" ? "critical" : "warn",
        used: 0,
        limit: 1,
        ratio: 1,
        detail: `fleet role "${h.role}" unreachable`,
      });
      continue;
    }
    if (h.diskUsedPct !== undefined) {
      const sev: Severity =
        h.diskUsedPct >= fleetPol.diskCriticalPct ? "critical" : h.diskUsedPct >= fleetPol.diskWarnPct ? "warn" : "ok";
      findings.push({
        domain: "fleet",
        metric: `disk_pct:${h.role}`,
        severity: sev,
        used: h.diskUsedPct,
        limit: 100,
        ratio: Number((h.diskUsedPct / 100).toFixed(4)),
        detail: `fleet role "${h.role}" disk ${h.diskUsedPct}%`,
      });
    }
    if (h.load1m !== undefined) {
      const cpus = h.cpus && h.cpus > 0 ? h.cpus : 1;
      const perCpu = h.load1m / cpus;
      const sev: Severity =
        perCpu >= fleetPol.loadPerCpuCritical ? "critical" : perCpu >= fleetPol.loadPerCpuWarn ? "warn" : "ok";
      findings.push({
        domain: "fleet",
        metric: `load_per_cpu:${h.role}`,
        severity: sev,
        used: Number(perCpu.toFixed(2)),
        limit: fleetPol.loadPerCpuCritical,
        ratio: Number((perCpu / fleetPol.loadPerCpuCritical).toFixed(4)),
        detail: `fleet role "${h.role}" load1m=${h.load1m} (${cpus} CPUs, ${perCpu.toFixed(2)}/cpu)`,
      });
    }
  }

  const prodHealthy = hostHealthy(byRole.get("prod"), fleetPol);
  const buildHealthy = hostHealthy(byRole.get("build"), fleetPol);
  const testHealthy = hostHealthy(byRole.get("test"), fleetPol);

  const reasons: string[] = [];
  let edgeCompute: RoutingDecision["edgeCompute"] = "cloudflare-workers";
  let edgeStatic: RoutingDecision["edgeStatic"] = "cloudflare-r2";

  const reqRatio = reqCeiling > 0 ? cf.dailyWorkerRequests / reqCeiling : 0;
  if (tier === "free" && reqRatio >= cfPol.criticalRatio) {
    edgeCompute = prodHealthy ? "prod-origin" : testHealthy ? "test-standby" : "cloudflare-workers";
    reasons.push(`Cloudflare Workers free daily request ceiling (${(reqRatio * 100).toFixed(1)}%) -> failover to ${edgeCompute}`);
  }

  const r2Exceeded =
    cf.r2StorageBytes > cfPol.r2StorageSoftBytes ||
    cf.r2ClassAMonthly > cfPol.r2ClassASoftMonthly ||
    cf.r2ClassBMonthly > cfPol.r2ClassBSoftMonthly;
  if (r2Exceeded && tier === "free") {
    edgeStatic = prodHealthy ? "prod-origin" : testHealthy ? "test-standby" : "cloudflare-r2";
    reasons.push(`R2 free threshold exceeded -> offload static/cold assets to ${edgeStatic}`);
  } else if (r2Exceeded) {
    reasons.push("R2 exceeds 10 GiB / Class A 1M / Class B 10M free allowance (R2 Paid active; consider cold-archive lifecycle)");
  }

  let ciRunner: RoutingDecision["ciRunner"] = "github-hosted";
  let buildTarget: RoutingDecision["buildTarget"] = buildHealthy ? "build" : testHealthy ? "test" : "github-hosted";
  const shouldOffloadGh = ghMinutesFinding.ratio >= ghPol.offloadRatio || worstCacheRatio >= ghPol.offloadRatio;
  if (shouldOffloadGh) {
    if (buildHealthy) {
      ciRunner = "self-hosted-build";
      buildTarget = "build";
      reasons.push("GitHub Actions quota >= offload threshold -> route CI/builds to fleet build role");
    } else if (testHealthy) {
      ciRunner = "self-hosted-test";
      buildTarget = "test";
      reasons.push("GitHub Actions quota >= offload threshold and build role unhealthy -> route CI to fleet test role");
    } else {
      reasons.push("GitHub Actions quota >= offload threshold, but fleet build/test roles unavailable; staying on github-hosted");
    }
  } else if (!buildHealthy && testHealthy) {
    buildTarget = "test";
    reasons.push("Fleet build role unavailable/overloaded -> fail over heavy builds to test role");
  }

  let severity: Severity = "ok";
  for (const f of findings) severity = maxSev(severity, f.severity);
  const exitCode: 0 | 1 | 2 = severity === "critical" ? 2 : severity === "warn" ? 1 : 0;

  return {
    ok: severity === "ok",
    severity,
    exitCode,
    tier,
    findings,
    routing: {
      edgeStatic,
      edgeCompute,
      ciRunner,
      buildTarget,
      reasons,
    },
    summary: `severity=${severity} tier=${tier} edge=${edgeCompute}/${edgeStatic} ci=${ciRunner} findings=${findings.filter((f) => f.severity !== "ok").length}/${findings.length}`,
  };
}

const R2_CLASS_A = new Set([
  "PutObject",
  "CopyObject",
  "CompleteMultipartUpload",
  "CreateMultipartUpload",
  "UploadPart",
  "UploadPartCopy",
  "ListBuckets",
  "ListObjects",
  "ListMultipartUploads",
  "PutBucket",
  "DeleteBucket",
  "PutBucketCors",
  "PutBucketLifecycleConfiguration",
]);

const R2_CLASS_B = new Set([
  "GetObject",
  "HeadObject",
  "HeadBucket",
  "GetBucketCors",
  "GetBucketLifecycleConfiguration",
  "GetBucketVersioning",
  "ReportUsageSummary",
]);

async function cfApi<T = any>(token: string, path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`https://api.cloudflare.com/client/v4${path}`, {
    ...init,
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
      ...(init?.headers ?? {}),
    },
    signal: AbortSignal.timeout(12_000),
  });
  return (await res.json()) as T;
}

export async function probeCloudflareLive(
  accountId = Bun.env.CLOUDFLARE_ACCOUNT_ID ?? "",
  apiToken = Bun.env.CLOUDFLARE_API_TOKEN ?? "",
  tierOverride?: CloudflareTier,
): Promise<CloudflareTelemetry> {
  if (!accountId || !apiToken) {
    throw new Error("CLOUDFLARE_ACCOUNT_ID and CLOUDFLARE_API_TOKEN are required for live Cloudflare probe");
  }
  const now = new Date();
  const since = new Date(now.getTime() - 24 * 3600 * 1000).toISOString();
  const monthStart = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), 1)).toISOString().slice(0, 10);

  const gqlQuery = {
    query: `query($acc: String!, $since: Time!, $monthStart: Date!) {
      viewer {
        accounts(filter: { accountTag: $acc }) {
          workersInvocationsAdaptive(limit: 200, filter: { datetime_geq: $since }) {
            dimensions { status }
            sum { requests errors }
          }
          r2OperationsAdaptiveGroups(limit: 500, filter: { date_geq: $monthStart }) {
            dimensions { actionType }
            sum { requests }
          }
          r2StorageAdaptiveGroups(limit: 100, filter: { date_geq: $monthStart }) {
            dimensions { bucketName }
            max { payloadSize metadataSize }
          }
        }
      }
    }`,
    variables: { acc: accountId, since, monthStart },
  };

  const [scripts, kv, d1, subs, gql] = await Promise.all([
    cfApi(apiToken, `/accounts/${accountId}/workers/scripts`),
    cfApi(apiToken, `/accounts/${accountId}/storage/kv/namespaces?per_page=100`),
    cfApi(apiToken, `/accounts/${accountId}/d1/database?per_page=100`),
    cfApi(apiToken, `/accounts/${accountId}/subscriptions`),
    cfApi(apiToken, `/graphql`, { method: "POST", body: JSON.stringify(gqlQuery) }),
  ]);

  const hasWorkersPaid = (subs.result ?? []).some(
    (s: any) => s?.product?.name === "prod_workers" && s?.state === "Paid",
  );
  const accData = gql?.data?.viewer?.accounts?.[0] ?? {};

  let dailyWorkerRequests = 0;
  let dailyWorkerErrors = 0;
  for (const row of accData.workersInvocationsAdaptive ?? []) {
    dailyWorkerRequests += Number(row?.sum?.requests ?? 0);
    dailyWorkerErrors += Number(row?.sum?.errors ?? 0);
  }

  let r2ClassAMonthly = 0;
  let r2ClassBMonthly = 0;
  for (const row of accData.r2OperationsAdaptiveGroups ?? []) {
    const action = String(row?.dimensions?.actionType ?? "");
    const count = Number(row?.sum?.requests ?? 0);
    if (R2_CLASS_A.has(action)) r2ClassAMonthly += count;
    else if (R2_CLASS_B.has(action)) r2ClassBMonthly += count;
  }

  const bucketMax = new Map<string, number>();
  for (const row of accData.r2StorageAdaptiveGroups ?? []) {
    const name = String(row?.dimensions?.bucketName ?? "");
    const size = Number(row?.max?.payloadSize ?? 0) + Number(row?.max?.metadataSize ?? 0);
    bucketMax.set(name, Math.max(bucketMax.get(name) ?? 0, size));
  }
  let r2StorageBytes = 0;
  for (const v of bucketMax.values()) r2StorageBytes += v;

  const d1List = d1.result ?? [];
  const d1StorageBytes = d1List.reduce((acc: number, db: any) => acc + Number(db?.file_size ?? 0), 0);

  return {
    tier: tierOverride ?? (hasWorkersPaid ? "paid" : "free"),
    workersCount: (scripts.result ?? []).length,
    dailyWorkerRequests,
    dailyWorkerErrors,
    r2StorageBytes,
    r2ClassAMonthly,
    r2ClassBMonthly,
    kvNamespaces: (kv.result ?? []).length,
    d1Databases: d1List.length,
    d1StorageBytes,
  };
}

function runSync(cmd: string[]): { ok: boolean; stdout: string } {
  try {
    const p = Bun.spawnSync(cmd, { stdout: "pipe", stderr: "ignore" });
    return { ok: p.exitCode === 0, stdout: new TextDecoder().decode(p.stdout).trim() };
  } catch {
    return { ok: false, stdout: "" };
  }
}

export function probeGithubActionsLive(repos: string[], org?: string): GithubActionsTelemetry {
  let monthlyMinutesUsed = 0;
  let monthlyMinutesLimit: number | undefined;
  if (org) {
    const billing = runSync(["gh", "api", `/orgs/${org}/settings/billing/actions`]);
    if (billing.ok && billing.stdout) {
      try {
        const j = JSON.parse(billing.stdout) as {
          total_minutes_used?: number;
          included_minutes?: number;
        };
        if (typeof j.total_minutes_used === "number") monthlyMinutesUsed = j.total_minutes_used;
        if (typeof j.included_minutes === "number" && j.included_minutes > 0) {
          monthlyMinutesLimit = j.included_minutes;
        }
      } catch {
        // Token may lack org billing scope; keep default 0.
      }
    }
  }
  const repoCaches: RepoCacheUsage[] = [];
  for (const slug of repos) {
    const res = runSync(["gh", "api", `/repos/${slug}/actions/cache/usage`]);
    if (!res.ok || !res.stdout) continue;
    try {
      const j = JSON.parse(res.stdout) as { active_caches_size_in_bytes?: number };
      if (typeof j.active_caches_size_in_bytes === "number") {
        repoCaches.push({ repo: slug, cacheBytes: j.active_caches_size_in_bytes });
      }
    } catch {
      // Ignore malformed responses.
    }
  }
  return { monthlyMinutesUsed, monthlyMinutesLimit, repos: repoCaches };
}

export function probeVpsFleetLive(hosts: Partial<Record<HostRole, string>>): VpsHostTelemetry[] {
  const roles: HostRole[] = ["prod", "build", "test"];
  const out: VpsHostTelemetry[] = [];
  for (const role of roles) {
    const alias = hosts[role];
    if (!alias) continue;
    const res = runSync([
      "ssh",
      "-o",
      "ConnectTimeout=4",
      "-o",
      "BatchMode=yes",
      alias,
      "df -P / | awk 'NR==2{print $5}'; cut -d' ' -f1 /proc/loadavg; nproc",
    ]);
    if (!res.ok || !res.stdout) {
      out.push({ role, reachable: false });
      continue;
    }
    const lines = res.stdout.split(/\r?\n/).map((l) => l.trim());
    const diskUsedPct = Number.parseFloat((lines[0] ?? "").replace("%", ""));
    const load1m = Number.parseFloat(lines[1] ?? "");
    const cpus = Number.parseInt(lines[2] ?? "", 10);
    out.push({
      role,
      reachable: true,
      diskUsedPct: Number.isFinite(diskUsedPct) ? diskUsedPct : undefined,
      load1m: Number.isFinite(load1m) ? load1m : undefined,
      cpus: Number.isFinite(cpus) && cpus > 0 ? cpus : undefined,
    });
  }
  return out;
}

export async function collectLiveTelemetry(opts: {
  withGh?: boolean;
  withSsh?: boolean;
  tier?: CloudflareTier;
  root?: string;
} = {}): Promise<QuotaTelemetry> {
  const cfg = readAy(opts.root ?? repoRoot) as {
    orgs?: { dev?: string };
    hosts?: Partial<Record<HostRole, string>>;
    repos?: Record<string, { slug?: string }>;
  };
  const cloudflare = await probeCloudflareLive(undefined, undefined, opts.tier);

  const repoSlugs = Object.values(cfg.repos ?? {})
    .map((r) => r.slug)
    .filter((s): s is string => Boolean(s))
    .slice(0, 6);
  const githubActions = opts.withGh
    ? probeGithubActionsLive(repoSlugs, cfg.orgs?.dev)
    : { monthlyMinutesUsed: 0, repos: [] };

  const vpsFleet = opts.withSsh ? probeVpsFleetLive(cfg.hosts ?? {}) : [];
  return { cloudflare, githubActions, vpsFleet };
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  const jsonMode = args.includes("--json");
  const liveMode = args.includes("--live");
  const withGh = !args.includes("--no-gh");
  const withSsh = args.includes("--with-ssh");
  const tierIdx = args.indexOf("--tier");
  const tierArg = tierIdx >= 0 ? (args[tierIdx + 1] as CloudflareTier | undefined) : undefined;

  const telemetry: QuotaTelemetry = liveMode
    ? await collectLiveTelemetry({ withGh, withSsh, tier: tierArg })
    : {
        cloudflare: {
          tier: tierArg ?? "free",
          workersCount: 0,
          dailyWorkerRequests: 0,
          r2StorageBytes: 0,
          r2ClassAMonthly: 0,
          r2ClassBMonthly: 0,
        },
        githubActions: { monthlyMinutesUsed: 0, repos: [] },
        vpsFleet: [],
      };

  const result = evaluateQuotaRouting(telemetry, tierArg ? { cloudflare: { tier: tierArg } } : undefined);
  if (jsonMode) {
    console.log(JSON.stringify({ telemetry, evaluation: result }, null, 2));
  } else {
    console.log(result.summary);
    for (const f of result.findings.filter((x) => x.severity !== "ok")) {
      console.log(`  [${f.severity.toUpperCase()}] ${f.detail}`);
    }
    for (const r of result.routing.reasons) {
      console.log(`  -> ${r}`);
    }
  }
  process.exit(result.exitCode);
}
