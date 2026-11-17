#!/usr/bin/env bun
/** `bun apps/chromium-dash/src/main.ts`: serve the dashboards. PORT (default 8787) and AY_CHROMIUM_RELEASES_DB configure it. */
import { createClients } from "@aphrody/ay-chromium-releases";
import { createApp } from "./app.ts";

const app = createApp({ clients: createClients() });
const server = Bun.serve({ port: Number(process.env.PORT ?? 8787), hostname: process.env.HOST ?? "127.0.0.1", fetch: app.fetch, idleTimeout: 60 });
console.log(`chromium-dash listening on ${server.url}`);
