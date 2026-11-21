#!/usr/bin/env bun
// `docker compose` for compose.yaml with FW_VERSION (the image tag) taken from product.toml `version`.
//   bun tools/compose.ts up --build
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const product = Bun.TOML.parse(await Bun.file(join(root, "product.toml")).text()) as { version: string };
const proc = Bun.spawn(["docker", "compose", ...Bun.argv.slice(2)], {
  cwd: root,
  env: { ...process.env, FW_VERSION: product.version },
  stdio: ["inherit", "inherit", "inherit"],
});
process.exit(await proc.exited);
