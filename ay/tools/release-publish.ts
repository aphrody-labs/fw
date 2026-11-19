#!/usr/bin/env bun
// Publishes the artifacts of one release into the releases bucket layout of apps/edge/src/manifest.ts: the assets, SHA256SUMS, release.json,
// the channel pointer and the refreshed global index. The matrix that classifies an artifact (os, arch, abi, kind) is release.toml.
//   bun tools/release-publish.ts publish --product <key> --version <v> --channel <c> --tag <t> --commit <sha> --dir <artifacts>
//        [--out <dir>] [--force] [--date <iso>]        without --out the objects go to R2 (CLOUDFLARE_ACCOUNT_ID, R2_ACCESS_KEY_ID, R2_SECRET_ACCESS_KEY)
//   bun tools/release-publish.ts matrix --product <key>   the build targets of a product as a GitHub Actions matrix (JSON on stdout)
// Immutable objects (assets, SHA256SUMS, release.json) are written once: a second run with the same bytes is a no-op, different bytes fail
// unless --force. The channel pointer and manifest.json are mutable and always rewritten.
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { S3Client } from "bun";
import {
  type AssetKind,
  type ChannelPointer,
  type IndexManifest,
  type ReleaseAsset,
  type ReleaseManifest,
  SCHEMA_INDEX,
  SCHEMA_RELEASE,
  foldIndex,
  isChannel,
  releaseKeys,
  sha256sums,
  validateRelease,
} from "../apps/edge/src/manifest.ts";

const root = resolve(import.meta.dir, "..");

export interface Target {
  asset: string;
  os: string;
  arch: string;
  abi?: string;
  kind: AssetKind;
  runner: string;
  ship?: boolean;
  [extra: string]: unknown;
}
export type Matrix = Record<string, { targets: Target[] }>;

export function readMatrix(path = join(root, "release.toml")): Matrix {
  return (Bun.TOML.parse(readFileSync(path, "utf8")) as { products: Matrix }).products;
}

/** Bucket name of the releases bucket: apps/edge/edge.toml [buckets].RELEASES, the one place that names it. */
export function releasesBucket(path = join(root, "apps/edge/edge.toml")): string {
  const bucket = (Bun.TOML.parse(readFileSync(path, "utf8")) as { buckets?: Record<string, string> }).buckets?.RELEASES;
  if (!bucket) throw new Error(`${path}: [buckets].RELEASES is missing`);
  return bucket;
}

const escapeRe = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
/** The regular expression of an asset pattern: {version} and {tag} match what the release says, everything else is literal. */
export function patternRegex(pattern: string, vars: { version: string; tag: string }): RegExp {
  const body = pattern
    .split(/(\{version\}|\{tag\})/)
    .map((part) => (part === "{version}" ? escapeRe(vars.version) : part === "{tag}" ? escapeRe(vars.tag) : escapeRe(part)))
    .join("");
  return new RegExp(`^${body}$`);
}

const isIgnored = (name: string) => name === "SHA256SUMS" || name === "release.json" || name.endsWith(".sha256");

export interface Plan {
  manifest: ReleaseManifest;
  /** Local files by asset name. */
  files: Map<string, string>;
}

export interface PlanInput {
  product: string;
  version: string;
  channel: string;
  tag: string;
  commit: string;
  dir: string;
  date?: string;
  matrix?: Matrix;
}

/** Hashes and classifies the top-level files of `dir` against the matrix of the product; a file no target names is an error. */
export async function planRelease(input: PlanInput): Promise<Plan> {
  const { product, version, channel, tag, commit, dir } = input;
  if (!isChannel(channel)) throw new Error(`channel must be one of lts, stable, preview, canary, nightly, dev (got ${channel})`);
  const matrix = input.matrix ?? readMatrix();
  const targets = matrix[product]?.targets;
  if (!targets) throw new Error(`release.toml has no product ${product}`);
  const matchers = targets.filter((t) => t.ship !== false).map((t) => ({ t, re: patternRegex(t.asset, { version, tag }) }));
  const names = readdirSync(dir)
    .filter((n) => statSync(join(dir, n)).isFile() && !isIgnored(n))
    .sort();
  const assets: ReleaseAsset[] = [];
  const files = new Map<string, string>();
  const unmatched: string[] = [];
  for (const name of names) {
    const hit = matchers.find((m) => m.re.test(name));
    if (!hit) {
      unmatched.push(name);
      continue;
    }
    const path = join(dir, name);
    const hasher = new Bun.CryptoHasher("sha256");
    for await (const chunk of Bun.file(path).stream()) hasher.update(chunk);
    const { os, arch, abi, kind } = hit.t;
    assets.push({ name, os, arch, kind, size: statSync(path).size, sha256: hasher.digest("hex"), ...(abi ? { abi } : {}) });
    files.set(name, path);
  }
  if (unmatched.length) throw new Error(`${product}: no target of release.toml names ${unmatched.join(", ")}`);
  const manifest: ReleaseManifest = {
    schema: SCHEMA_RELEASE,
    product,
    version,
    channel,
    tag,
    commit,
    date: input.date ?? new Date().toISOString(),
    assets,
  };
  const issues = validateRelease(manifest);
  if (issues.length) throw new Error(`invalid release manifest: ${issues.join("; ")}`);
  return { manifest, files };
}

export interface Store {
  head(key: string): Promise<{ size: number } | null>;
  get(key: string): Promise<Uint8Array | null>;
  put(key: string, body: Uint8Array | string | { path: string }, type: string): Promise<void>;
}

export function fsStore(dir: string): Store {
  const at = (key: string) => join(dir, ...key.split("/"));
  return {
    async head(key) {
      const p = at(key);
      return existsSync(p) ? { size: statSync(p).size } : null;
    },
    async get(key) {
      const p = at(key);
      return existsSync(p) ? new Uint8Array(readFileSync(p)) : null;
    },
    async put(key, body) {
      const p = at(key);
      mkdirSync(dirname(p), { recursive: true });
      if (typeof body === "object" && "path" in body) await Bun.write(p, Bun.file(body.path));
      else writeFileSync(p, body);
    },
  };
}

export function r2Store(client: S3Client): Store {
  return {
    async head(key) {
      const f = client.file(key);
      return (await f.exists()) ? { size: (await f.stat()).size } : null;
    },
    async get(key) {
      const f = client.file(key);
      return (await f.exists()) ? new Uint8Array(await f.arrayBuffer()) : null;
    },
    async put(key, body, type) {
      await client.write(key, typeof body === "object" && "path" in body ? Bun.file(body.path) : body, { type });
    },
  };
}

export function r2StoreFromEnv(bucket: string, env: Record<string, string | undefined> = process.env): Store {
  const missing = ["CLOUDFLARE_ACCOUNT_ID", "R2_ACCESS_KEY_ID", "R2_SECRET_ACCESS_KEY"].filter((k) => !env[k]);
  if (missing.length) throw new Error(`R2 credentials missing in the environment: ${missing.join(", ")} (or pass --out <dir>)`);
  return r2Store(
    new S3Client({
      accessKeyId: env.R2_ACCESS_KEY_ID!,
      secretAccessKey: env.R2_SECRET_ACCESS_KEY!,
      bucket,
      endpoint: `https://${env.CLOUDFLARE_ACCOUNT_ID}.r2.cloudflarestorage.com`,
    }),
  );
}

const TYPES: Record<string, string> = {
  wasm: "application/wasm",
  json: "application/json; charset=utf-8",
  gz: "application/gzip",
  zip: "application/zip",
  exe: "application/vnd.microsoft.portable-executable",
  dmg: "application/x-apple-diskimage",
};
const typeOf = (name: string) => TYPES[name.split(".").pop() ?? ""] ?? "application/octet-stream";

export interface PublishResult {
  written: string[];
  unchanged: string[];
  manifest: ReleaseManifest;
  index: IndexManifest;
}

const same = (a: Uint8Array, b: Uint8Array) => a.byteLength === b.byteLength && a.every((v, i) => v === b[i]);

export async function publishRelease(plan: Plan, store: Store, opts: { force?: boolean; now?: string } = {}): Promise<PublishResult> {
  const { manifest, files } = plan;
  const { product, version, channel } = manifest;
  const written: string[] = [];
  const unchanged: string[] = [];
  const enc = new TextEncoder();

  const immutable = async (key: string, body: Uint8Array | string | { path: string }, type: string, size: number, bytes?: Uint8Array) => {
    const existing = await store.head(key);
    if (existing && !opts.force) {
      const equal = bytes ? same((await store.get(key)) ?? new Uint8Array(), bytes) : existing.size === size;
      if (!equal) throw new Error(`${key} already exists with different content; releases are immutable (use --force to replace)`);
      unchanged.push(key);
      return;
    }
    await store.put(key, body, type);
    written.push(key);
  };

  for (const a of manifest.assets) {
    await immutable(releaseKeys.asset(product, version, a.name), { path: files.get(a.name)! }, typeOf(a.name), a.size);
  }
  const sums = enc.encode(sha256sums(manifest));
  await immutable(releaseKeys.sums(product, version), sums, "text/plain; charset=utf-8", sums.byteLength, sums);
  const releaseJson = enc.encode(JSON.stringify(manifest, null, 2) + "\n");
  await immutable(releaseKeys.release(product, version), releaseJson, TYPES.json, releaseJson.byteLength, releaseJson);

  const pointer: ChannelPointer = { schema: SCHEMA_RELEASE, product, channel, version, tag: manifest.tag, date: manifest.date };
  await store.put(releaseKeys.channel(product, channel), JSON.stringify(pointer, null, 2) + "\n", TYPES.json);
  written.push(releaseKeys.channel(product, channel));

  const current = await store.get(releaseKeys.index());
  const known: ChannelPointer[] = [];
  if (current) {
    const parsed = JSON.parse(new TextDecoder().decode(current)) as IndexManifest;
    if (parsed.schema !== SCHEMA_INDEX) throw new Error(`${releaseKeys.index()}: unexpected schema ${parsed.schema}`);
    for (const channels of Object.values(parsed.products)) known.push(...(Object.values(channels) as ChannelPointer[]));
  }
  const index = foldIndex([...known, pointer], opts.now ?? manifest.date);
  await store.put(releaseKeys.index(), JSON.stringify(index, null, 2) + "\n", TYPES.json);
  written.push(releaseKeys.index());
  return { written, unchanged, manifest, index };
}

/** The build targets of a product as the `include` list of a GitHub Actions matrix. */
export function actionsMatrix(product: string, matrix = readMatrix()): { include: Target[] } {
  const targets = matrix[product]?.targets;
  if (!targets) throw new Error(`release.toml has no product ${product}`);
  return { include: targets.filter((t) => t.ship !== false) };
}

if (import.meta.main) {
  const [cmd, ...rest] = Bun.argv.slice(2);
  const opt = (flag: string) => {
    const i = rest.indexOf(flag);
    return i >= 0 ? rest[i + 1] : undefined;
  };
  const need = (flag: string) => {
    const v = opt(flag);
    if (!v) throw new Error(`${flag} is required`);
    return v;
  };
  try {
    if (cmd === "matrix") {
      console.log(JSON.stringify(actionsMatrix(need("--product"))));
    } else if (cmd === "publish") {
      const plan = await planRelease({
        product: need("--product"),
        version: need("--version"),
        channel: need("--channel"),
        tag: need("--tag"),
        commit: need("--commit"),
        dir: resolve(need("--dir")),
        date: opt("--date"),
      });
      const out = opt("--out");
      const store = out ? fsStore(resolve(out)) : r2StoreFromEnv(releasesBucket());
      const result = await publishRelease(plan, store, { force: rest.includes("--force") });
      console.log(JSON.stringify({ target: out ? resolve(out) : releasesBucket(), written: result.written, unchanged: result.unchanged }, null, 2));
    } else {
      console.error("usage: bun tools/release-publish.ts publish|matrix ... (see the header of this file)");
      process.exit(2);
    }
  } catch (e) {
    console.error(`release-publish: ${(e as Error).message}`);
    process.exit(1);
  }
}

