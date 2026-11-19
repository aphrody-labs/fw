import { afterEach, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type IndexManifest, type ReleaseManifest, pickAsset, releaseKeys, validateRelease } from "../apps/edge/src/manifest.ts";
import { actionsMatrix, compileArgv, fsStore, patternRegex, planRelease, publishRelease, readMatrix, releasesBucket } from "./release-publish.ts";

const dirs: string[] = [];
const tmp = () => {
  const d = mkdtempSync(join(tmpdir(), "release-publish-"));
  dirs.push(d);
  return d;
};
afterEach(() => {
  for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
});

function artifacts(files: Record<string, string>): string {
  const dir = tmp();
  for (const [name, body] of Object.entries(files)) writeFileSync(join(dir, name), body);
  return dir;
}
const sha = (s: string) => new Bun.CryptoHasher("sha256").update(s).digest("hex");
const readJson = <T>(root: string, key: string): T => JSON.parse(readFileSync(join(root, ...key.split("/")), "utf8")) as T;

describe("release.toml", () => {
  test("every product of the ecosystem that ships binaries is described, with unique asset patterns and known vocabulary", () => {
    const matrix = readMatrix();
    const ay = Bun.TOML.parse(readFileSync(join(import.meta.dir, "..", "ay.toml"), "utf8")) as { ecosystem: { stack: string[] } };
    for (const key of ay.ecosystem.stack) expect(matrix[key]).toBeDefined();
    const arches = ["x86_64", "aarch64", "riscv64", "wasm32", "wasm64", "any"];
    const oses = ["linux", "windows", "macos", "android", "freestanding", "wasi", "browser", "any"];
    for (const [product, { targets }] of Object.entries(matrix)) {
      const names = targets.map((t) => t.asset);
      expect(new Set(names).size, product).toBe(names.length);
      for (const t of targets) {
        expect(arches, `${product} ${t.asset}`).toContain(t.arch);
        expect(oses, `${product} ${t.asset}`).toContain(t.os);
      }
    }
  });

  test("the releases bucket is named by edge.toml", () => {
    expect(releasesBucket()).toBeTruthy();
  });

  test("the ay build matrix lists every shipped target with its builder", () => {
    const { include } = actionsMatrix("ay");
    expect(include.map((t) => t.asset)).toContain("ay-corelib-component.wasm");
    expect(include.every((t) => (t as { build?: string }).build)).toBe(true);
    expect(actionsMatrix("nie").include.map((t) => t.asset).some((a) => a.endsWith(".wasm"))).toBe(false);
  });

  test("every bun-compile target builds with minify and bytecode for its own bun target", () => {
    const targets = actionsMatrix("ay").include.filter((t) => t.build === "bun-compile");
    expect(targets.map((t) => t.bun_target).sort()).toEqual(["bun-darwin-arm64", "bun-linux-arm64", "bun-linux-x64", "bun-windows-x64"]);
    for (const t of targets) {
      const argv = compileArgv("ay", t.asset, "out/x");
      expect(argv).toEqual(["build", "--compile", "--minify", "--bytecode", `--target=${t.bun_target}`, "apps/ay/src/cli.ts", "--outfile", "out/x"]);
    }
    expect(() => compileArgv("ay", "ay-corelib-base.wasm", "x")).toThrow("not bun-compile");
  });

  test("{version} patterns match the release version only", () => {
    const re = patternRegex("nie-v{version}-x86_64-pc-windows-msvc.zip", { version: "1.2.3", tag: "v1.2.3" });
    expect(re.test("nie-v1.2.3-x86_64-pc-windows-msvc.zip")).toBe(true);
    expect(re.test("nie-v1x2x3-x86_64-pc-windows-msvc.zip")).toBe(false);
  });
});

describe("publish", () => {
  const input = { product: "fw", version: "4.2.0", channel: "stable", tag: "v4.2.0", commit: "a".repeat(40), date: "2026-11-19T10:00:00.000Z" };
  const fwFiles = {
    "fw-x86_64-linux-musl": "elf-bytes",
    "fw-x86_64-windows.exe": "pe-bytes",
    "fw-aarch64-macos": "macho-bytes",
    "fw_wasm.wasm": "\0asm\x01\0\0\0",
    "SHA256SUMS": "ignored",
    "fw-x86_64-linux-musl.sha256": "ignored",
  };

  test("dry run into a directory writes the manifest layout and a valid release", async () => {
    const out = tmp();
    const plan = await planRelease({ ...input, dir: artifacts(fwFiles) });
    const result = await publishRelease(plan, fsStore(out), { now: input.date });

    expect(validateRelease(plan.manifest)).toEqual([]);
    expect(plan.manifest.assets.map((a) => a.name)).toEqual(["fw-aarch64-macos", "fw-x86_64-linux-musl", "fw-x86_64-windows.exe", "fw_wasm.wasm"]);

    const release = readJson<ReleaseManifest>(out, releaseKeys.release("fw", "4.2.0"));
    expect(release).toEqual(plan.manifest);
    const linux = release.assets.find((a) => a.name === "fw-x86_64-linux-musl")!;
    expect(linux).toMatchObject({ os: "linux", arch: "x86_64", abi: "musl", kind: "binary", size: 9, sha256: sha("elf-bytes") });
    expect(readFileSync(join(out, "fw", "4.2.0", "fw-x86_64-linux-musl"), "utf8")).toBe("elf-bytes");
    expect(readFileSync(join(out, "fw", "4.2.0", "SHA256SUMS"), "utf8")).toContain(`${sha("macho-bytes")}  fw-aarch64-macos\n`);
    expect(readJson<{ version: string; channel: string }>(out, releaseKeys.channel("fw", "stable"))).toMatchObject({ version: "4.2.0", channel: "stable" });
    expect(pickAsset(release, "browser", "wasm32")?.name).toBe("fw_wasm.wasm");
    expect(pickAsset(release, "macos", "aarch64")?.name).toBe("fw-aarch64-macos");
    expect(result.written).toContain(releaseKeys.index());
  });

  test("a second product folds into the index, a newer version replaces the pointer of its channel", async () => {
    const out = tmp();
    await publishRelease(await planRelease({ ...input, dir: artifacts(fwFiles) }), fsStore(out));
    const ay = { product: "ay", version: "0.4.0", channel: "stable", tag: "v0.4.0", commit: "b".repeat(40), date: "2026-11-19T11:00:00.000Z" };
    await publishRelease(await planRelease({ ...ay, dir: artifacts({ "ay-linux-x86_64": "x", "ay-corelib-base.wasm": "\0asm\x01\0\0\0" }) }), fsStore(out));
    await publishRelease(
      await planRelease({ ...input, version: "4.3.0", tag: "v4.3.0", date: "2026-11-19T12:00:00.000Z", dir: artifacts({ "fw-aarch64-macos": "newer" }) }),
      fsStore(out),
    );
    const index = readJson<IndexManifest>(out, releaseKeys.index());
    expect(Object.keys(index.products).sort()).toEqual(["ay", "fw"]);
    expect(index.products.fw?.stable?.version).toBe("4.3.0");
    expect(index.products.ay?.stable?.version).toBe("0.4.0");
  });

  test("immutable objects: identical re-run is a no-op, different bytes fail, --force replaces", async () => {
    const out = tmp();
    const store = fsStore(out);
    await publishRelease(await planRelease({ ...input, dir: artifacts(fwFiles) }), store);
    const again = await publishRelease(await planRelease({ ...input, dir: artifacts(fwFiles) }), store);
    expect(again.unchanged).toContain(releaseKeys.release("fw", "4.2.0"));
    expect(again.written).not.toContain(releaseKeys.release("fw", "4.2.0"));

    const changed = await planRelease({ ...input, dir: artifacts({ ...fwFiles, "fw-aarch64-macos": "other-bytes-longer" }) });
    await expect(publishRelease(changed, store)).rejects.toThrow(/immutable/);
    await publishRelease(changed, store, { force: true });
    expect(readFileSync(join(out, "fw", "4.2.0", "fw-aarch64-macos"), "utf8")).toBe("other-bytes-longer");
  });

  test("a file no target names, an unknown product and a bad channel are refused", async () => {
    await expect(planRelease({ ...input, dir: artifacts({ ...fwFiles, "mystery.bin": "x" }) })).rejects.toThrow(/mystery\.bin/);
    await expect(planRelease({ ...input, product: "nope", dir: artifacts(fwFiles) })).rejects.toThrow(/no product nope/);
    await expect(planRelease({ ...input, channel: "weekly", dir: artifacts(fwFiles) })).rejects.toThrow(/channel/);
  });

  test("versioned patterns (nie) classify and a subdirectory such as manifests/ is ignored", async () => {
    const dir = artifacts({
      "nie-v1.0.0-x86_64-unknown-linux-musl.tar.gz": "tar",
      "nie-v1.0.0-x86_64-pc-windows-msvc.zip": "zip",
    });
    mkdirSync(join(dir, "manifests"));
    writeFileSync(join(dir, "manifests", "linux-x86_64.json"), "{}");
    const plan = await planRelease({ product: "nie", version: "1.0.0", channel: "preview", tag: "v1.0.0", commit: "c".repeat(40), dir });
    expect(plan.manifest.assets.map((a) => [a.os, a.arch, a.abi, a.kind])).toEqual([
      ["windows", "x86_64", "msvc", "archive"],
      ["linux", "x86_64", "musl", "archive"],
    ]);
  });
});
