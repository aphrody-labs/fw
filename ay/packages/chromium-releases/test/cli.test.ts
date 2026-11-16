import { describe, expect, test } from "bun:test";
import { join } from "node:path";

const cli = join(import.meta.dir, "..", "src", "cli.ts");
async function run(...args: string[]) {
  const p = Bun.spawn([process.execPath, cli, ...args], { stdout: "pipe", stderr: "pipe", env: { ...process.env, AY_CHROMIUM_RELEASES_DB: ":memory:" } });
  const [out, err, code] = await Promise.all([new Response(p.stdout).text(), new Response(p.stderr).text(), p.exited]);
  return { out, err, code };
}

describe("cli", () => {
  test("endpoints prints both services with their endpoint tables", async () => {
    const { out, code } = await run("endpoints");
    const j = JSON.parse(out);
    expect(code).toBe(0);
    expect(j.chromiumdash.map((e: { path: string }) => e.path)).toContain("/fetch_releases");
    expect(j.chromestatus.map((e: { path: string }) => e.path)).toContain("/features/{id}");
    expect(j.channels).toContain("Extended");
  });

  test("no command prints the usage", async () => {
    const { out, code } = await run();
    expect(code).toBe(0);
    expect(out).toContain("usage: chromium-releases");
  });

  test("an unknown command fails with the usage on stderr", async () => {
    const { err, code } = await run("nope");
    expect(code).toBe(2);
    expect(err).toContain("usage: chromium-releases");
  });

  test("a missing argument is an error, not a crash trace", async () => {
    const { err, code } = await run("commit", "--no-cache");
    expect(code).toBe(1);
    expect(err).toContain("missing <hash>");
  });
});
