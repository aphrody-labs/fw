// Writes docs/auto/openapi.json directly from @aphrody/auto-server's buildOpenApiSpec.
import { buildOpenApiSpec } from "../apps/auto-server/src/router";

const target = "docs/auto/openapi.json";
await Bun.write(target, `${JSON.stringify(buildOpenApiSpec("https://auto.aphrody.com"), null, 2)}\n`);
const fmt = Bun.spawnSync([process.execPath, "fmt", target], { stdout: "inherit", stderr: "inherit" });
if (fmt.exitCode !== 0) process.exit(fmt.exitCode ?? 1);
console.log(`wrote ${target}`);
