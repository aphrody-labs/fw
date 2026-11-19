import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";

const ROOT = resolve(import.meta.dir, "..");
const EXT = process.platform === "win32" ? ".exe" : "";
const NATIVE_BIN = join(ROOT, "target", "release", `fairwind-native${EXT}`);

describe("Fairwind v4.0 — Asterinas OSDK + GPUI WebGPU/Wasm + Alpine uutils + NixOS + 23 unified stacks", () => {
  test("os/fairwind-os conforms to official Asterinas OSDK, NixOS flake.nix, and Alpine APKBUILD", () => {
    const libRs = readFileSync(join(ROOT, "os", "fairwind-os", "src", "lib.rs"), "utf8");
    expect(libRs).toContain("#![no_std]");
    expect(libRs).toContain("#![deny(unsafe_code)]");
    expect(libRs).toContain("#[ostd::main]");

    const osdkToml = readFileSync(join(ROOT, "os", "fairwind-os", "OSDK.toml"), "utf8");
    expect(osdkToml).toContain('project_type = "kernel"');
    expect(osdkToml).toContain('[scheme."microvm"]');
    expect(osdkToml).toContain('[scheme."virtio-gpu"]');

    const flakeNix = readFileSync(join(ROOT, "os", "fairwind-os", "flake.nix"), "utf8");
    expect(flakeNix).toContain("uutils-coreutils-noprefix");
    expect(flakeNix).toContain("nixosModules.fairwind");

    const apkbuild = readFileSync(join(ROOT, "os", "fairwind-os", "APKBUILD"), "utf8");
    expect(apkbuild).toContain("uutils-coreutils");
    expect(apkbuild).toContain("+crt-static");
  });

  test("fairwind-native executes ostd+GPUI, uutils multicall, pure-Rust NixOS store/NAR, and unified v4 stack", () => {
    expect(existsSync(NATIVE_BIN)).toBe(true);

    const ostd = JSON.parse(
      new TextDecoder().decode(Bun.spawnSync([NATIVE_BIN, "ostd"], { cwd: ROOT, stdout: "pipe" }).stdout),
    );
    expect(ostd.ok).toBe(true);
    expect(ostd.kernel.framekernel_safe_rust).toBe(true);
    expect(ostd.kernel.gpui_scene_quads).toBe(3);

    const uu = JSON.parse(
      new TextDecoder().decode(
        Bun.spawnSync([NATIVE_BIN, "uutils", "echo", "alpine-lighter", "pure-rust"], { cwd: ROOT, stdout: "pipe" }).stdout,
      ),
    );
    expect(uu.ok).toBe(true);
    expect(uu.result.stdout).toBe("alpine-lighter pure-rust\n");
    expect(uu.applets).toContain("sha256sum");

    const nix = JSON.parse(
      new TextDecoder().decode(Bun.spawnSync([NATIVE_BIN, "nix", "fairwind-os-4.0"], { cwd: ROOT, stdout: "pipe" }).stdout),
    );
    expect(nix.ok).toBe(true);
    expect(nix.entry.store_path.startsWith("/nix/store/")).toBe(true);
    expect(nix.entry.uutils_noprefix).toBe(true);

    const stack = JSON.parse(
      new TextDecoder().decode(Bun.spawnSync([NATIVE_BIN, "stack"], { cwd: ROOT, stdout: "pipe" }).stdout),
    );
    expect(stack.ok).toBe(true);
    expect(stack.catalog.version).toBe("4.0.0");
    expect(stack.burnRig.webgpu_native).toBe(true);
    expect(stack.voice.terminal_control_ready).toBe(true);

    const wl = JSON.parse(
      new TextDecoder().decode(Bun.spawnSync([NATIVE_BIN, "wayland"], { cwd: ROOT, stdout: "pipe" }).stdout),
    );
    expect(wl.ok).toBe(true);
    expect(wl.zeroLibwaylandCDeps).toBe(true);
    expect(wl.commitHeader.opcode).toBe(6);
    expect(wl.commitHeader.size).toBe(8);
    expect(wl.gpuiQuads).toBe(1);
  });
});
