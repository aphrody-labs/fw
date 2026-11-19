import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";

const ROOT = resolve(import.meta.dir, "..");
const EXT = process.platform === "win32" ? ".exe" : "";
const NATIVE_BIN = join(ROOT, "target", "release", `fw${EXT}`);

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

  test("reproducible root flake.nix and nix/ 3-VPS pure-Rust userland definitions are complete", () => {
    const rootFlake = readFileSync(join(ROOT, "flake.nix"), "utf8");
    expect(rootFlake).toContain("rust-userland");
    expect(rootFlake).toContain("uutils-coreutils-noprefix");
    expect(rootFlake).toContain("xddxdd/nix-cachyos-kernel/release");
    expect(rootFlake).toContain("nixosConfigurations");
    expect(rootFlake).toContain("fr1 = mkVps ./nix/hosts/fr1.nix");
    expect(rootFlake).toContain("fr2 = mkVps ./nix/hosts/fr2.nix");
    expect(rootFlake).toContain("prd = mkVps ./nix/hosts/prd.nix");

    const common = readFileSync(join(ROOT, "nix", "modules", "vps-common.nix"), "utf8");
    expect(common).toContain("security.sudo-rs.enable = true");
    expect(common).toContain("services.ntpd-rs.enable = true");
    expect(common).toContain("vm.watermark_boost_factor");
    expect(common).toContain("vm.watermark_scale_factor");
    expect(common).toContain('ATTR{queue/scheduler}="none"');
    expect(common).toContain("fairwind-native serve");

    const fr1 = readFileSync(join(ROOT, "nix", "hosts", "fr1.nix"), "utf8");
    expect(fr1).toContain("linuxPackages-cachyos-latest-lto-x86_64-v3");

    const prd = readFileSync(join(ROOT, "nix", "hosts", "prd.nix"), "utf8");
    expect(prd).toContain("linuxPackages-cachyos-latest-lto-zen4");
    expect(prd).toContain("fairwind-build-worker");
    expect(prd).toContain('MemoryHigh = "12G"');

    const bootstrap = readFileSync(join(ROOT, "nix", "bootstrap-vps.sh"), "utf8");
    expect(bootstrap).toContain("rust-userland");
    expect(bootstrap).toContain("auto-optimise-store = true");

    const wslHost = readFileSync(join(ROOT, "nix", "hosts", "wsl-fairwind.nix"), "utf8");
    expect(wslHost).toContain("useWindowsDriver = true");
    expect(wslHost).toContain('MESA_D3D12_DEFAULT_ADAPTER_NAME = "NVIDIA"');
    expect(wslHost).toContain("virtiofsd");
    expect(wslHost).toContain("scx.rustscheds");
    expect(wslHost).not.toContain("\n    wslu\n");

    const wslKernelCfg = readFileSync(join(ROOT, "os", "fairwind-wsl", "kernel", "fairwind-wsl-6.18.config"), "utf8");
    expect(wslKernelCfg).toContain("# CONFIG_9P_FS is not set");
    expect(wslKernelCfg).toContain("CONFIG_VIRTIO_FS=y");
    expect(wslKernelCfg).toContain("CONFIG_VIRTIO_VSOCKETS=y");
    expect(wslKernelCfg).toContain("CONFIG_DXGKRNL=y");
    expect(wslKernelCfg).toContain("CONFIG_SCHED_BORE=y");
    expect(wslKernelCfg).toContain("CONFIG_SCHED_CLASS_EXT=y");
    expect(wslKernelCfg).toContain("CONFIG_LTO_CLANG_THIN=y");
    expect(wslKernelCfg).toContain("CONFIG_XFS_FS=y");

    const rustStack = readFileSync(join(ROOT, "os", "fairwind-wsl", "wslg", "rust-compositor-stack.toml"), "utf8");
    expect(rustStack).toContain("microsoft/openvmm");
    expect(rustStack).toContain("Devolutions/IronRDP");
    expect(rustStack).toContain("fw-wayland-wgpu");
    expect(rustStack).toContain("scx_lavd");

    const wslgBuild = readFileSync(join(ROOT, "os", "fairwind-wsl", "wslg", "build-wslg.sh"), "utf8");
    expect(wslgBuild).toContain("1137 1333 1046 1426 1436");
  });
});
