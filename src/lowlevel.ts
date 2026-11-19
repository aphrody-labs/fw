import { existsSync, mkdirSync, readFileSync, statSync } from "node:fs";
import { join, resolve } from "node:path";

export interface LowLevelTargetRecipe {
  lang: string;
  arch: string;
  compiler: string;
  ltoAotFlags: string;
  sourceExample: string;
}

export const LOW_LEVEL_LTO_AOT_MATRIX: LowLevelTargetRecipe[] = [
  {
    lang: "rust",
    arch: "x86_64 | aarch64 | riscv64 | wasm32-wasip2",
    compiler: "rustc / cargo",
    ltoAotFlags: "opt-level=3, lto='fat', codegen-units=1, panic='abort', strip='symbols'",
    sourceExample: "native/fairwind-agy/Cargo.toml",
  },
  {
    lang: "c",
    arch: "x86_64 | aarch64 | riscv64 | wasm32 | bpf",
    compiler: "clang",
    ltoAotFlags: "-O3 -flto=full -fuse-ld=lld -fomit-frame-pointer",
    sourceExample: "core/c/fairwind_c_smoke.c",
  },
  {
    lang: "cpp",
    arch: "x86_64 | aarch64 | riscv64 | wasm32",
    compiler: "clang++",
    ltoAotFlags: "-std=c++23 -O3 -flto=full -fno-exceptions -fno-rtti",
    sourceExample: "core/cpp/fairwind_cpp_smoke.cpp",
  },
  {
    lang: "assembly",
    arch: "x86_64 (Win64 & SysV) | aarch64 | riscv64",
    compiler: "clang / llvm-mc",
    ltoAotFlags: "-O3 -c (links directly into C/C++/Rust Fat-LTO binaries)",
    sourceExample: "core/asm/fairwind_kernel.S",
  },
  {
    lang: "webassembly",
    arch: "wasm32 | wasm64 | wasm32-wasip2",
    compiler: "clang --target=wasm32 / wasm-opt",
    ltoAotFlags: "--target=wasm32 -O3 -nostdlib -Wl,--no-entry -Wl,--export-all -Wl,--strip-all",
    sourceExample: "core/wasm/fairwind_wasm.c",
  },
  {
    lang: "cuda",
    arch: "nvptx64 (sm_80 / sm_89 / sm_90)",
    compiler: "clang --target=nvptx64-nvidia-cuda / nvcc",
    ltoAotFlags: "--target=nvptx64-nvidia-cuda -O3 -S (or nvcc -O3 -dlto -arch=sm_89)",
    sourceExample: "core/cuda/fairwind_kernel.cu",
  },
  {
    lang: "webgpu",
    arch: "SPIR-V / DXIL / MSL / WGSL",
    compiler: "naga / tint / wgpu",
    ltoAotFlags: "WGSL compute shaders with workgroupBarrier & fma",
    sourceExample: "core/webgpu/fairwind_compute.wgsl",
  },
  {
    lang: "typescript",
    arch: "x86_64 | aarch64 (Windows / Linux / macOS)",
    compiler: "bun build --compile --aot --trim",
    ltoAotFlags: "--compile --aot --trim --no-compile-function-source",
    sourceExample: "src/fairwind_cli.ts",
  },
  {
    lang: "go",
    arch: "amd64 | arm64 | riscv64 | wasm",
    compiler: "go build",
    ltoAotFlags: "-trimpath -ldflags='-s -w' (-buildmode=c-shared for FFI)",
    sourceExample: "core/go/fairwind-go/main.go",
  },
  {
    lang: "java",
    arch: "jvm / GraalVM native-image",
    compiler: "javac / java",
    ltoAotFlags: "javac --release 25 (or native-image -O3 --gc=serial)",
    sourceExample: "core/java/FairwindBridge.java",
  },
  {
    lang: "kotlin",
    arch: "jvm / kotlin-native (x64/arm64)",
    compiler: "kotlinc / konanc",
    ltoAotFlags: "-opt (Kotlin/Native LLVM AOT)",
    sourceExample: "core/kotlin/FairwindSdk.kt",
  },
  {
    lang: "python",
    arch: "x86_64 | aarch64 (PyTorch / PEFT / Heretic)",
    compiler: "uv / torch.compile",
    ltoAotFlags: "uv run + torch.compile(mode='max-autotune')",
    sourceExample: "core/py/fairwind_ml/pipeline.py",
  },
];

export interface LowLevelVerifyStep {
  name: string;
  ok: boolean;
  skipped?: boolean;
  output?: string;
  bytes?: number;
  error?: string;
}

export async function verifyLowLevelToolchains(root: string) {
  const outDir = resolve(root, "target", "lowlevel");
  mkdirSync(outDir, { recursive: true });
  const steps: LowLevelVerifyStep[] = [];
  const isWin = process.platform === "win32";

  const hasClang = Boolean(Bun.which("clang"));
  const hasClangxx = Boolean(Bun.which("clang++"));
  const hasGo = Boolean(Bun.which("go"));
  const hasJavac = Boolean(Bun.which("javac")) && Boolean(Bun.which("java"));

  // 1. C + x86-64 Assembly native build & execution
  if (hasClang) {
    const outExe = join(outDir, isWin ? "fairwind_c_asm.exe" : "fairwind_c_asm");
    const compile = Bun.spawnSync(
      ["clang", "-O3", "core/c/fairwind_c_smoke.c", "core/asm/fairwind_kernel.S", "-o", outExe],
      { cwd: root, stdout: "pipe", stderr: "pipe" },
    );
    if (compile.exitCode === 0) {
      const run = Bun.spawnSync([outExe], { cwd: root, stdout: "pipe", stderr: "pipe" });
      steps.push({
        name: "c+x86_64-asm",
        ok: run.exitCode === 0,
        bytes: statSync(outExe).size,
        output: run.stdout.toString().trim(),
        error: run.exitCode !== 0 ? run.stderr.toString().trim() : undefined,
      });
    } else {
      steps.push({ name: "c+x86_64-asm", ok: false, error: compile.stderr.toString().trim() });
    }

    // 2. WebAssembly (wasm32) compile + LIVE execution inside Bun WebAssembly runtime
    const wasmOut = join(outDir, "fairwind_kernel.wasm");
    const wasmCompile = Bun.spawnSync(
      [
        "clang",
        "--target=wasm32",
        "-O3",
        "-nostdlib",
        "-Wl,--no-entry",
        "-Wl,--export-all",
        "-Wl,--strip-all",
        "core/wasm/fairwind_wasm.c",
        "-o",
        wasmOut,
      ],
      { cwd: root, stdout: "pipe", stderr: "pipe" },
    );
    if (wasmCompile.exitCode === 0 && existsSync(wasmOut)) {
      try {
        const wasmBytes = readFileSync(wasmOut);
        const mod = await WebAssembly.instantiate(wasmBytes);
        const exp = mod.instance.exports as {
          fairwind_wasm_add: (a: number, b: number) => number;
          fairwind_wasm_dot4: (
            a0: number,
            a1: number,
            a2: number,
            a3: number,
            b0: number,
            b1: number,
            b2: number,
            b3: number,
          ) => number;
        };
        const sum = exp.fairwind_wasm_add(19, 23);
        const dot = exp.fairwind_wasm_dot4(1, 2, 3, 4, 5, 6, 7, 8);
        steps.push({
          name: "webassembly-wasm32",
          ok: sum === 42 && Math.abs(dot - 70) < 0.001,
          bytes: wasmBytes.byteLength,
          output: JSON.stringify({ sum, dot }),
        });
      } catch (err) {
        steps.push({ name: "webassembly-wasm32", ok: false, error: String(err) });
      }
    } else {
      steps.push({
        name: "webassembly-wasm32",
        ok: false,
        error: wasmCompile.stderr.toString().trim() || "wasm-ld unavailable",
      });
    }

    // 3. CUDA / NVPTX64 PTX generation via LLVM Clang
    const ptxOut = join(outDir, "fairwind_nvptx.ptx");
    const ptxCompile = Bun.spawnSync(
      ["clang", "--target=nvptx64-nvidia-cuda", "-S", "-O3", "core/cuda/fairwind_nvptx.c", "-o", ptxOut],
      { cwd: root, stdout: "pipe", stderr: "pipe" },
    );
    if (ptxCompile.exitCode === 0 && existsSync(ptxOut)) {
      const ptx = readFileSync(ptxOut, "utf8");
      const validPtx = ptx.includes(".target sm_") && ptx.includes("fairwind_nvptx_saxpy");
      steps.push({
        name: "cuda-nvptx64-ptx",
        ok: validPtx,
        bytes: statSync(ptxOut).size,
        output: ptx.split(/\r?\n/).slice(0, 6).join(" | "),
      });
    } else {
      steps.push({ name: "cuda-nvptx64-ptx", ok: false, error: ptxCompile.stderr.toString().trim() });
    }
  } else {
    steps.push({ name: "clang-lowlevel", ok: true, skipped: true, output: "clang not installed" });
  }

  // 4. C++ native build & execution
  if (hasClangxx) {
    const cppExe = join(outDir, isWin ? "fairwind_cpp.exe" : "fairwind_cpp");
    const c = Bun.spawnSync(["clang++", "-std=c++20", "-O3", "core/cpp/fairwind_cpp_smoke.cpp", "-o", cppExe], {
      cwd: root,
      stdout: "pipe",
      stderr: "pipe",
    });
    if (c.exitCode === 0) {
      const r = Bun.spawnSync([cppExe], { cwd: root, stdout: "pipe", stderr: "pipe" });
      steps.push({
        name: "cpp-native",
        ok: r.exitCode === 0,
        bytes: statSync(cppExe).size,
        output: r.stdout.toString().trim(),
      });
    } else {
      steps.push({ name: "cpp-native", ok: false, error: c.stderr.toString().trim() });
    }
  }

  // 5. WebGPU WGSL static verification
  const wgslPath = resolve(root, "core", "webgpu", "fairwind_compute.wgsl");
  if (existsSync(wgslPath)) {
    const wgsl = readFileSync(wgslPath, "utf8");
    const valid =
      wgsl.includes("@compute") &&
      wgsl.includes("@workgroup_size(256)") &&
      wgsl.includes("workgroupBarrier()") &&
      wgsl.includes("fn fairwind_rmsnorm");
    steps.push({ name: "webgpu-wgsl", ok: valid, bytes: Buffer.byteLength(wgsl), output: "fairwind_saxpy + fairwind_rmsnorm" });
  }

  // 6. Go native test
  if (hasGo) {
    const goRes = Bun.spawnSync(["go", "test", "./..."], {
      cwd: resolve(root, "core", "go", "fairwind-go"),
      stdout: "pipe",
      stderr: "pipe",
    });
    steps.push({
      name: "go-test",
      ok: goRes.exitCode === 0,
      output: goRes.stdout.toString().trim(),
      error: goRes.exitCode !== 0 ? goRes.stderr.toString().trim() : undefined,
    });
  }

  // 7. Java compile & run
  if (hasJavac) {
    const javaOut = join(outDir, "java");
    mkdirSync(javaOut, { recursive: true });
    const jc = Bun.spawnSync(["javac", "-d", javaOut, "core/java/FairwindBridge.java"], {
      cwd: root,
      stdout: "pipe",
      stderr: "pipe",
    });
    if (jc.exitCode === 0) {
      const jr = Bun.spawnSync(["java", "-cp", javaOut, "FairwindBridge", root], {
        cwd: root,
        stdout: "pipe",
        stderr: "pipe",
      });
      steps.push({
        name: "java-ghidra-bridge",
        ok: jr.exitCode === 0,
        output: jr.stdout.toString().trim(),
      });
    } else {
      steps.push({ name: "java-ghidra-bridge", ok: false, error: jc.stderr.toString().trim() });
    }
  }

  return {
    ok: steps.every((s) => s.ok),
    outDir,
    steps,
    matrixCount: LOW_LEVEL_LTO_AOT_MATRIX.length,
  };
}
