// Stores: what a module reads. A store is a binding of the Worker, a key prefix inside it and an allow-list of the keys it may serve, all rendered
// from edge.toml [stores.<NAME>] into vars.EDGE_STORES. The Worker replaces env.<NAME> by the guarded view before any module runs, so a module that
// serves "any key" (assets, bucket, releases) can only reach allowed keys: a private key answers as a missing one, whatever path asks for it.
// Moving a store to another bucket or prefix is an edge.toml change only.
import type { EdgeEnv } from "./types.ts";

export interface StoreSpec {
  /** Binding of the underlying bucket (edge.toml [buckets]). */
  binding: string;
  /** Prepended to every key before it reaches the bucket: "" or "dir/". */
  prefix?: string;
  /** Keys served, relative to the prefix: an entry ending in "/" allows that prefix, any other entry is one exact key. Absent: the whole store. */
  allow?: string[];
}

export const storesOf = (env: EdgeEnv): Record<string, StoreSpec> => (env.EDGE_STORES ?? {}) as Record<string, StoreSpec>;

export function isAllowed(allow: readonly string[] | undefined, key: string): boolean {
  if (!allow) return true;
  return allow.some((a) => (a.endsWith("/") ? key.startsWith(a) : key === a));
}

type Readable = Pick<R2Bucket, "get" | "head">;

/** Read-only view of a bucket: keys outside the allow-list are absent, the others live under the prefix. */
export function guard(bucket: R2Bucket, spec: StoreSpec): R2Bucket {
  const prefix = spec.prefix ?? "";
  const view: Readable = {
    get: ((key: string, options?: R2GetOptions) => (isAllowed(spec.allow, key) ? bucket.get(prefix + key, options) : Promise.resolve(null))) as R2Bucket["get"],
    head: (key: string) => (isAllowed(spec.allow, key) ? bucket.head(prefix + key) : Promise.resolve(null)),
  };
  return view as R2Bucket;
}

const views = new WeakMap<object, EdgeEnv>();

/** env with every declared store replaced by its guarded view (once per env). */
export function withStores(env: EdgeEnv): EdgeEnv {
  const specs = env.EDGE_STORES as object | undefined;
  if (!specs) return env;
  let out = views.get(env);
  if (!out) {
    out = { ...env };
    for (const [name, spec] of Object.entries(specs as Record<string, StoreSpec>)) {
      const bucket = env[spec.binding] as R2Bucket | undefined;
      if (bucket) out[name] = guard(bucket, spec);
    }
    views.set(env, out);
  }
  return out;
}
