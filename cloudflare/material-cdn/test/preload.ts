// Runs the Worker under bun test: `.wasm` imports become compiled modules (as in Workers) and `caches.default` is an in-memory Cache.
import { plugin } from "bun";

plugin({
  name: "wasm-module",
  setup(build) {
    build.onLoad({ filter: /\.wasm$/ }, async (args) => ({
      exports: { default: new WebAssembly.Module(await Bun.file(args.path).arrayBuffer()) },
      loader: "object",
    }));
  },
});

const store = new Map<string, Response>();
(globalThis as unknown as { caches: unknown }).caches = {
  default: {
    async match(request: Request) {
      return store.get(new URL(request.url).pathname + new URL(request.url).search)?.clone();
    },
    async put(request: Request, response: Response) {
      store.set(new URL(request.url).pathname + new URL(request.url).search, response.clone());
    },
  },
};
