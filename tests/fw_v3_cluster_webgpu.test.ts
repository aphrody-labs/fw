import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";

const ROOT = resolve(import.meta.dir, "..");
const EXT = process.platform === "win32" ? ".exe" : "";
const NATIVE_BIN = join(ROOT, "target", "release", `fw${EXT}`);
const WASM_BIN = join(ROOT, "target", "wasm32-unknown-unknown", "release", "fw_wasm.wasm");

describe("fw binary, cluster router, ggml, wasm32 module and PNG render", () => {
  test("cluster declaration and architecture docs exist", () => {
    expect(existsSync(join(ROOT, "Cargo.toml"))).toBe(true);
    expect(existsSync(join(ROOT, "cluster", "build.toml"))).toBe(true);
    expect(existsSync(join(ROOT, "docs", "ARCHITECTURE.md"))).toBe(true);
    expect(existsSync(join(ROOT, "docs", "GGML_COMPATIBILITY.md"))).toBe(true);
    expect(existsSync(join(ROOT, "docs", "HARDWARE_MATRIX.md"))).toBe(true);
  });

  test("fw_wasm.wasm executes live in WebAssembly.instantiate (Cloudflare + Browser WebGPU/FPU)", async () => {
    expect(existsSync(WASM_BIN)).toBe(true);
    const bytes = readFileSync(WASM_BIN);
    const { instance } = await WebAssembly.instantiate(bytes, {});
    const exports = instance.exports as {
      fw_wasm_abi_version: () => number;
      fw_wasm_decision_noul: (logit: number) => number;
      fw_wasm_q8_dot4: (
        a0: number,
        a1: number,
        a2: number,
        a3: number,
        b0: number,
        b1: number,
        b2: number,
        b3: number,
        scale: number,
      ) => number;
    };
    expect(exports.fw_wasm_abi_version()).toBe(3);
    expect(Math.abs(exports.fw_wasm_decision_noul(0.0) - 0.5)).toBeLessThan(1e-5);
    expect(Math.abs(exports.fw_wasm_q8_dot4(1, 2, 3, 4, 4, 3, 2, 1, 0.5) - 10.0)).toBeLessThan(1e-5);
  });

  test("fw zero-dep Fat-LTO binary runs status, ggml-rpc, cluster KV-route, and Blitz PNG render", () => {
    expect(existsSync(NATIVE_BIN)).toBe(true);

    const statusProc = Bun.spawnSync([NATIVE_BIN, "status"], { cwd: ROOT, stdout: "pipe", stderr: "pipe" });
    expect(statusProc.exitCode).toBe(0);
    const status = JSON.parse(new TextDecoder().decode(statusProc.stdout));
    expect(status.ok).toBe(true);
    expect(status.zeroSystemDeps).toBe(true);
    // The router is read from cluster/build.toml plus the measured local machine; nothing is hardcoded.
    const declared = Object.keys((Bun.TOML.parse(readFileSync(join(ROOT, "cluster", "build.toml"), "utf8")) as { hosts: object }).hosts);
    const localCuda = status.hardware.has_cuda === true;
    expect(status.cluster.gpuHosts).toBe(localCuda ? 1 : 0);
    expect(status.cluster.vpsProxyLbStorageCpuHosts).toBe(declared.length + (localCuda ? 0 : 1));
    expect(status.cluster.router).toContain("consistent-hash");

    const ggmlProc = Bun.spawnSync([NATIVE_BIN, "ggml"], { cwd: ROOT, stdout: "pipe", stderr: "pipe" });
    expect(ggmlProc.exitCode).toBe(0);
    const ggml = JSON.parse(new TextDecoder().decode(ggmlProc.stdout));
    expect(ggml.ok).toBe(true);
    expect(ggml.gguf.version).toBe(3);
    expect(ggml.rpcLoopback.totalBytes).toBe(12 * 1024 * 1024 * 1024);

    const routeProc = Bun.spawnSync([NATIVE_BIN, "cluster", "route", "Fw hybrid cluster test"], {
      cwd: ROOT,
      stdout: "pipe",
      stderr: "pipe",
    });
    expect(routeProc.exitCode).toBe(0);
    const route = JSON.parse(new TextDecoder().decode(routeProc.stdout));
    expect(route.first.kv_cache_hit).toBe(false);
    expect(route.secondKvHit.kv_cache_hit).toBe(true);
    expect(route.first.disaggregated).toBe(true);

    const pngOut = join(ROOT, "target", "blitz-proof.png");
    const uiProc = Bun.spawnSync([NATIVE_BIN, "ui", "--out", pngOut], { cwd: ROOT, stdout: "pipe", stderr: "pipe" });
    expect(uiProc.exitCode).toBe(0);
    const ui = JSON.parse(new TextDecoder().decode(uiProc.stdout));
    expect(ui.ok).toBe(true);
    expect(ui.png_bytes).toBeGreaterThan(500);
    const pngBytes = readFileSync(pngOut);
    expect(Array.from(pngBytes.subarray(0, 8))).toEqual([137, 80, 78, 71, 13, 10, 26, 10]);
  });
});
