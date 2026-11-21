// The release contract shared by the publisher (tools/release-publish.ts, CI) and the edge Worker (downloads, manifest).
// R2 layout of the releases bucket (edge.toml [buckets.releases]), one prefix per product of ay.toml [ecosystem].stack:
//   <product>/<version>/<asset>           immutable, cached forever
//   <product>/<version>/SHA256SUMS        `<sha256>  <asset>` lines
//   <product>/<version>/release.json      ReleaseManifest of this version, immutable
//   <product>/channels/<channel>.json     ChannelPointer, mutable (revalidated on every request)
//   manifest.json                         IndexManifest of every product and channel, mutable

export const SCHEMA_RELEASE = "ay.release.v1";
export const SCHEMA_INDEX = "ay.releases.v1";

// release.toml [channels]: order and aliases (tools/release-plan.test.ts keeps the two equal).
export const CHANNELS = ["canary", "dev", "beta", "stable", "extended", "lts"] as const;
export type Channel = (typeof CHANNELS)[number];
/** Channel names from before the release train and the upstream names, so old URLs and pointers keep resolving. */
export const CHANNEL_ALIASES: Readonly<Record<string, Channel>> = { nightly: "canary", preview: "beta", longterm: "lts", mainline: "dev", "linux-next": "canary" };

export type AssetKind = "binary" | "archive" | "installer" | "wasm" | "library" | "bundle" | "checksum";

export interface ReleaseAsset {
  name: string;
  /** ay.toml [sdk].os: linux, windows, macos, android, wasi, browser, freestanding; "any" for portable files. */
  os: string;
  /** ay.toml [sdk].arch: x86_64, aarch64, riscv64, wasm32, wasm64; "any" for portable files. */
  arch: string;
  kind: AssetKind;
  size: number;
  sha256: string;
  /** Rust/clang triple or free-form libc flavour when it distinguishes two assets of one os and arch (musl, gnu, msvc). */
  abi?: string;
}

export interface ReleaseManifest {
  schema: typeof SCHEMA_RELEASE;
  product: string;
  version: string;
  channel: Channel;
  tag: string;
  commit: string;
  date: string;
  assets: ReleaseAsset[];
}

export interface ChannelPointer {
  schema: typeof SCHEMA_RELEASE;
  product: string;
  channel: Channel;
  version: string;
  tag: string;
  date: string;
}

export interface IndexManifest {
  schema: typeof SCHEMA_INDEX;
  generated: string;
  products: Record<string, Partial<Record<Channel, ChannelPointer>>>;
}

const SEGMENT = /^[A-Za-z0-9][A-Za-z0-9._+-]*$/;
const SHA256 = /^[0-9a-f]{64}$/;

export const isChannel = (v: string): v is Channel => (CHANNELS as readonly string[]).includes(v);
export const toChannel = (v: string): Channel | null => (isChannel(v) ? v : (CHANNEL_ALIASES[v] ?? null));
export const isSegment = (v: string): boolean => SEGMENT.test(v) && !v.includes("..");

export const releaseKeys = {
  asset: (product: string, version: string, name: string) => `${product}/${version}/${name}`,
  sums: (product: string, version: string) => `${product}/${version}/SHA256SUMS`,
  release: (product: string, version: string) => `${product}/${version}/release.json`,
  channel: (product: string, channel: Channel) => `${product}/channels/${channel}.json`,
  index: () => "manifest.json",
};

/** Issues of a manifest read from the outside; an empty list means it can be published. */
export function validateRelease(m: unknown): string[] {
  const issues: string[] = [];
  const r = m as Partial<ReleaseManifest> | null;
  if (!r || typeof r !== "object") return ["manifest is not an object"];
  if (r.schema !== SCHEMA_RELEASE) issues.push(`schema must be ${SCHEMA_RELEASE}`);
  for (const k of ["product", "version", "tag", "commit", "date"] as const) if (typeof r[k] !== "string" || !r[k]) issues.push(`${k} is required`);
  if (typeof r.product === "string" && !isSegment(r.product)) issues.push("product is not a path segment");
  if (typeof r.version === "string" && !isSegment(r.version)) issues.push("version is not a path segment");
  if (typeof r.channel !== "string" || !isChannel(r.channel)) issues.push(`channel must be one of ${CHANNELS.join(", ")}`);
  if (!Array.isArray(r.assets) || r.assets.length === 0) issues.push("assets must be a non-empty array");
  const seen = new Set<string>();
  for (const a of r.assets ?? []) {
    if (!a || typeof a.name !== "string" || !isSegment(a.name)) issues.push(`asset name invalid: ${JSON.stringify(a?.name)}`);
    else if (seen.has(a.name)) issues.push(`asset duplicated: ${a.name}`);
    else seen.add(a.name);
    if (typeof a?.sha256 !== "string" || !SHA256.test(a.sha256)) issues.push(`asset ${a?.name}: sha256 must be 64 lowercase hex`);
    if (!Number.isSafeInteger(a?.size) || (a?.size ?? -1) < 0) issues.push(`asset ${a?.name}: size must be a non-negative integer`);
    if (typeof a?.os !== "string" || typeof a?.arch !== "string") issues.push(`asset ${a?.name}: os and arch are required`);
  }
  return issues;
}

/** The asset for an os and arch: exact first, then portable over os (wasm32 runs anywhere), over arch, then fully portable; `kind` and `abi` narrow it, the first listed wins. */
export function pickAsset(m: ReleaseManifest, os: string, arch: string, opts: { kind?: AssetKind; abi?: string } = {}): ReleaseAsset | undefined {
  const fit = (a: ReleaseAsset) => a.kind !== "checksum" && (!opts.kind || a.kind === opts.kind) && (!opts.abi || a.abi === opts.abi);
  const at = (o: string, r: string) => m.assets.find((a) => fit(a) && a.os === o && a.arch === r);
  return at(os, arch) ?? at("any", arch) ?? at(os, "any") ?? at("any", "any");
}

export function sha256sums(m: ReleaseManifest): string {
  return m.assets.map((a) => `${a.sha256}  ${a.name}`).join("\n") + "\n";
}

/** Latest pointer per channel folded into the index; later pointers of the same product and channel replace earlier ones. */
export function foldIndex(pointers: ChannelPointer[], generated: string): IndexManifest {
  const products: IndexManifest["products"] = {};
  for (const p of pointers) (products[p.product] ??= {})[p.channel] = p;
  return { schema: SCHEMA_INDEX, generated, products };
}
