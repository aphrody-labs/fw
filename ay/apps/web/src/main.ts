#!/usr/bin/env bun
import { createClients } from "@aphrody/ay-chromium-releases";
import { createApp as createDashApp } from "../../chromium-dash/src/app.ts";
import { createWebApp } from "./app.ts";
import { resolveRepoRoot } from "./catalog.ts";

const root = resolveRepoRoot();
const dash = createDashApp({ clients: createClients(), root, product: "Aphrody Chromium Dash" });
const web = createWebApp({
  root,
  dashFetch: dash.fetch,
  searchOrigin: process.env.SEARCH_ORIGIN_URL,
});

const server = Bun.serve({
  port: Number(process.env.PORT ?? 8788),
  hostname: process.env.HOST ?? "127.0.0.1",
  fetch: web.fetch,
  idleTimeout: 60,
});

console.log(`ay-web listening on ${server.url}`);
