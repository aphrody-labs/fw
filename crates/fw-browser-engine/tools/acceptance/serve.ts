// The live dashboard of the acceptance tool: one Bun.serve process reading the store. HTML for people, JSON for tools,
// Server-Sent Events so a page or a benchmark that lands in the store is on screen within a second (Redis wakes the
// stream at once when available, a one-second tick does otherwise). `--schedule` also runs the cron of acceptance.toml,
// which is what keeps the page following the engines' evolution without anyone starting anything.
import { timingSafeEqual } from "node:crypto";
import { join, resolve, sep } from "node:path";
import { engines, launch, manifest, probe, report, root } from "../acceptance.ts";
import { probeNet, type NetProbe } from "./net.ts";
import type { Store } from "./store.ts";

const page = await Bun.file(join(import.meta.dir, "dashboard.html")).text();

export async function serve(store: Store, opt: { port: number; schedule: boolean }) {
  const key = process.env.ACCEPTANCE_KEY;
  const hub = new Set<() => void>();
  await store.watch(() => hub.forEach((wake) => wake()));

  // ---- the published host as a client sees it
  let net: NetProbe | null = await store.cached<NetProbe>("net");
  const refreshNet = async (verify: boolean) => {
    const next = await probeNet(manifest.serve.host);
    const blink = verify && next.h3.advertised ? engines(["blink"])[0] : null;
    if (blink) {
      // The engine reports the protocol it negotiated; forcing QUIC on the origin skips the first-visit Alt-Svc round trip.
      const running = await launch(blink, {
        extra: [`--origin-to-force-quic-on=${manifest.serve.host}:443`, "--enable-quic"],
      }).catch(() => null);
      const seen = running
        ? await probe(running, `https://${manifest.serve.host}/`, null, false).finally(() => running.stop())
        : null;
      if (seen && !next.h3.verified)
        Object.assign(next.h3, {
          verified: seen.protocol === "h3",
          via: `${blink.id} CDP response.protocol`,
          error: seen.protocol === "h3" ? undefined : `negotiated ${seen.protocol ?? seen.error}`,
        });
    }
    const changed =
      JSON.stringify({ ...net, at: 0, tls: { ...net?.tls, ms: 0 }, http: { ...net?.http, ms: 0 } }) !==
      JSON.stringify({ ...next, at: 0, tls: { ...next.tls, ms: 0 }, http: { ...next.http, ms: 0 } });
    net = next;
    await store.cache("net", next, 3600);
    if (changed) store.emit("net", { host: next.host, h3: next.h3, tls: next.tls.protocol, status: next.http.status });
  };
  void refreshNet(true).catch(() => {});
  setInterval(() => void refreshNet(Math.floor(Date.now() / 60000) % 30 === 0).catch(() => {}), 5 * 60000);

  const state = () => {
    const rep = report(store);
    const runs = store.runs(20).map(({ id, started, finished, ok, host }) => ({ id, started, finished, ok, host }));
    return {
      generated: new Date().toISOString(),
      host: manifest.serve.host,
      store: { sqlite: true, redis: store.redis },
      last_event: store.lastEvent(),
      run: rep,
      runs,
      bench: store.bench(300),
      upstreams: store.upstreams(40),
      net,
    };
  };

  // ---- cron
  const busy = new Set<string>();
  if (opt.schedule)
    for (const [command, expression] of Object.entries(manifest.schedule)) {
      Bun.cron(expression, async () => {
        if (busy.has(command)) return void store.emit("schedule", { command, skipped: "still running" });
        busy.add(command);
        store.emit("schedule", { command, state: "started", next: Bun.cron.parse(expression)?.toISOString() });
        const code = await Bun.spawn([process.execPath, join(root, "tools/acceptance.ts"), ...command.split(/\s+/)], {
          cwd: root,
          stdin: "ignore",
          stdout: "ignore",
          stderr: "ignore",
        }).exited;
        busy.delete(command);
        store.emit("schedule", { command, state: "finished", code });
      });
      console.error(`schedule ${expression}  ${command}`);
    }

  const secret = key ? Buffer.from(key) : null;
  const allowed = (req: Request) => {
    if (!secret) return true;
    const given = Buffer.from(req.headers.get("x-acceptance-key") ?? "");
    return given.length === secret.length && timingSafeEqual(given, secret);
  };
  const out = resolve(root, "out/acceptance") + sep;
  const encoder = new TextEncoder();

  const server = Bun.serve({
    port: opt.port,
    hostname: process.env.ACCEPTANCE_BIND ?? "127.0.0.1", // the tunnel and the Worker come from outside; nothing else should
    idleTimeout: 120,
    routes: {
      "/healthz": () => new Response("ok"),
      "/": (req) =>
        allowed(req)
          ? new Response(page, { headers: { "content-type": "text/html; charset=utf-8", "cache-control": "no-cache" } })
          : new Response("forbidden", { status: 403 }),
      "/api/state": (req) => (allowed(req) ? Response.json(state()) : new Response("forbidden", { status: 403 })),
      "/api/events": (req) => {
        if (!allowed(req)) return new Response("forbidden", { status: 403 });
        const url = new URL(req.url);
        let id = Number(req.headers.get("last-event-id") ?? url.searchParams.get("after") ?? store.lastEvent());
        let wake: (() => void) | undefined;
        let beat: Timer | undefined;
        const body = new ReadableStream({
          start(controller) {
            const flush = () => {
              try {
                for (const e of store.eventsAfter(id)) {
                  controller.enqueue(encoder.encode(`id: ${e.id}\nevent: ${e.kind}\ndata: ${JSON.stringify(e)}\n\n`));
                  id = e.id;
                }
              } catch {} // the stream was closed between two events
            };
            wake = flush;
            hub.add(flush);
            controller.enqueue(encoder.encode("retry: 2000\n\n"));
            flush();
            beat = setInterval(() => controller.enqueue(encoder.encode(": keep-alive\n\n")), 15000);
          },
          cancel() {
            if (wake) hub.delete(wake);
            clearInterval(beat);
          },
        });
        return new Response(body, {
          headers: {
            "content-type": "text/event-stream",
            "cache-control": "no-cache, no-transform",
            "x-accel-buffering": "no",
          },
        });
      },
      // /shots/<run>/<engine>/<file>.png, from the output directory the run recorded; nothing outside out/acceptance is served.
      "/shots/:run/:engine/:file": async (req) => {
        if (!allowed(req)) return new Response("forbidden", { status: 403 });
        const { run, engine, file } = req.params;
        const meta = store.runs(200).find((r) => r.id === run)?.meta;
        const path = meta?.out ? resolve(meta.out, engine, file) : null;
        if (!path || !path.startsWith(out) || !file.endsWith(".png")) return new Response("not found", { status: 404 });
        const png = Bun.file(path);
        return (await png.exists())
          ? new Response(png, {
              headers: { "content-type": "image/png", "cache-control": "public, max-age=31536000, immutable" },
            })
          : new Response("not found", { status: 404 });
      },
    },
    fetch: () => new Response("not found", { status: 404 }),
  });
  console.error(
    `dashboard http://${server.hostname}:${server.port}  store sqlite${store.redis ? ` + ${store.redis}` : ""}  published as https://${manifest.serve.host}`,
  );
  store.emit("serve", { port: server.port, schedule: opt.schedule });
  await new Promise(() => {}); // runs until killed
}
