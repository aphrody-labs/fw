// Bindings of the unified edge Worker. Names are unique across services. A store binding (edge.toml [stores.<NAME>]) is replaced by its
// guarded view before a module runs (see withStores in store.ts).
export interface EdgeEnv {
  /** Releases bucket (edge.toml [buckets.releases]): <product>/<version>/..., manifest.json, legacy channels/, scripts/. */
  RELEASES: R2Bucket;
  /** Content-addressed assets bucket: v1/<kind>/<sha256>.<ext> and plain keys. */
  ASSETS: R2Bucket;
  /** Video bucket behind the loop/stream/segment kinds of the gateway. */
  VIDEO: R2Bucket;
  /** Windows builds of ay-tools under ay-tools/. */
  CHROMIUM: R2Bucket;
  /** Service bindings to the scripts that still own state or code in another repository. */
  SVC_APHRODY?: Fetcher;
  SVC_AUTO?: Fetcher;
  SVC_SHENRON_CDN?: Fetcher;
  SVC_FW?: Fetcher;
  /** Where the wasm packages of apps/edge/wasm are imported (CompiledWasm modules, see services/wasm.ts). */
  [key: string]: unknown;
}

/** One routed request: the service sees the path with its mount prefix removed. */
export interface Routed {
  request: Request;
  url: URL;
  /** Path below the mount, always starting with "/". */
  path: string;
  /** Origin plus the preview mount ("" on a routed host): the prefix of every absolute link a service writes. */
  origin: string;
  env: EdgeEnv;
  ctx: ExecutionContext;
}

export type Service = (r: Routed) => Promise<Response> | Response;
