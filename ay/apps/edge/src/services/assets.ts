// Assets of the Worker. Typed content-addressed objects (v1/<kind>/<sha256>.<ext>, the agent-skills index) are served by the
// ay-cf-gateway app of apps/cf-gateway, reused as is; every other key of the assets bucket is served by name under /r2-assets
// (the aphrody-worker path) with the same cache policy as before: v1/ immutable, the rest one hour.
import gateway from "../../../cf-gateway/src/index.ts";
import { NOT_FOUND, serveObject } from "../r2.ts";
import type { Service } from "../types.ts";

const PLAIN = "/r2-assets";

const assets: Service = async (r) => {
  if (r.path === PLAIN || r.path.startsWith(`${PLAIN}/`)) {
    const key = decodeURIComponent(r.path.slice(PLAIN.length)).replace(/^\/+/, "");
    if (key === "" || key.includes("..") || key.includes("\0") || key.includes("\\")) return NOT_FOUND();
    return serveObject(r.env.ASSETS, key, r.request, { cache: key.startsWith("v1/") ? undefined : "public, max-age=3600", cors: true });
  }
  return gateway.fetch(r.request, { ASSETS: r.env.ASSETS, VIDEO: r.env.VIDEO }, r.ctx);
};

export default assets;
