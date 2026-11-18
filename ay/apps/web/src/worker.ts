import { createWebApp } from "./app.ts";
import { DEFAULT_PORTAL_SNAPSHOT } from "./catalog.ts";

export interface WorkerEnv {
  SEARCH_ORIGIN_URL?: string;
}

export default {
  async fetch(request: Request, env: WorkerEnv = {}): Promise<Response> {
    const app = createWebApp({
      snapshot: DEFAULT_PORTAL_SNAPSHOT,
      searchOrigin: env.SEARCH_ORIGIN_URL,
    });
    return app.fetch(request);
  },
};
