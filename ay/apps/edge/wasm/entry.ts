// Entry of the wasm service alone, for `wrangler dev` with target/edge-wasm/wrangler.jsonc (tools/edge-wasm.ts renders it):
// the whole Worker is the service mounted at "/", exactly as apps/edge mounts it at its routes.
import wasm from "../src/services/wasm.ts";
import type { EdgeEnv } from "../src/types.ts";

export default {
  fetch(request: Request, env: EdgeEnv, ctx: ExecutionContext): Promise<Response> | Response {
    const url = new URL(request.url);
    return wasm({ request, url, path: url.pathname, env, ctx });
  },
} satisfies ExportedHandler<EdgeEnv>;
