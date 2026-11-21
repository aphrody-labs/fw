# Queue: gpu-unlock into fw (`fw gpu`) and compute tuning

## Goal
1. Fork `zagitta/gpu-unlock` into aphrody-labs and wrap it as `fw gpu` (status, detect, unlock, bench): GPL-2.0 C module stays in a submodule (fw is MIT), PCI-ID and register-recipe table ported to Rust as data, Linux-only apply path (DKMS build, load, FP16/BF16 TFLOPS before/after), Windows reports "Linux only" and touches nothing.
2. Tune the compute for fw, measured per machine (PC RTX 4070 sm_89 CUDA build, FA, graphs, `fw vram` split; fr1 haswell, prd znver4: target-cpu, mimalloc, LTO/PGO, `fw tune cachy` profiles), and tune fw for it (per-host build profiles, runtime kernel detection AVX2/AVX-512/CUDA, `fw doctor` output), with before/after numbers.
3. Docs and skill section under `plugins/aphrody/skills/nvidia` (or a new `gpu` skill) with the support matrix and commands.
Do NOT load kernel modules or write GPU registers on the Windows PC; no Windows system, power plan or registry changes.

## Done (verified on origin)
- Fork: https://github.com/aphrody-labs/gpu-unlock (fork of zagitta/gpu-unlock, default branch main; submodule pinned at 7eb51bc, VERSION 2.0.0).
- fw commit b05bceb (origin/main): `.gitmodules` (`third_party/gpu-unlock`, url `../gpu-unlock.git`) and the gitlink.
- No other code written or committed. No GPU, driver, registry or power change was made.

## Findings so far (commands output seen in session)
- PC inventory (PowerShell CIM, nvidia-smi):
  - CPU i7-13700F, 16 cores / 24 threads, 2.1 GHz base (MaxClockSpeed field); family 6 model 0xB7 expected, so LLVM `raptorlake`, no AVX-512.
  - RAM 2 x 16 GiB Samsung M323R2GA3BB0-CQKOL, WMI Speed and ConfiguredClockSpeed 5800; total 34164056064 bytes. Board ASUS ROG STRIX G35CA.
  - Drives: disk 0 Micron_2400_MTFDKBA1T0QFM NVMe SSD 1024209543168 B (953.9 GB), GPT, healthy; disk 1 "Msft Virtual Disk" 107374182400 B, file-backed VHD (Dev Drive).
  - Volumes: C: OS NTFS 952.6 GB (268.9 free); D: DevDrive ReFS 99.9 GB (49.1 free, the VHD).
  - GPU RTX 4070, 12282 MiB, driver 621.14 (WMI 32.0.16.2114), compute cap 8.9, PCI device id 0x2786 (bus 00000000:01:00.0, subsystem 0x88D51043), power limit 200 W, max SM 3105 MHz, mem 10501 MHz, PCIe gen4 x16.
- gpu-unlock verdict for this PC: device 0x2786 is in `recipes.c` as `AD104(0x2786)` (chip 0x194, family ad10x, Ada = experimental per README). The unlock is Linux only and needs a pre-driver kernel module plus extracted NVIDIA 610.43.03 firmware; this PC runs Windows 11, so it does not apply here (WSL2 cannot load it either). It would apply only if this PC boots native Linux.
- fr1 (ssh): Ubuntu kernel 7.3.0-10-generic, 12 vCPU Haswell (no TSX) VM, 45 GB, `/srv/xbuild` 805G (751G free), cargo 1.100.0, llvm-profdata, clang, mold, lld, sccache present. Load average was 69-80 when sampled (about 60 users), so timing there is noisy; `/srv/xbuild/fw` is a clean checkout at 15804181, `/srv/xbuild/fw-target/release/fw` exists.
- prd (ssh): kernel 7.4.0-rc3-1-cachyos-rc, AMD EPYC 9354P, 8 vCPU, 31 GB, load ~0.6-1.7, cargo/rustc in /usr/bin, taskset and numactl present, no perf.
- fw current state: `fw infer --gguf` only reads the header and runs a Q8_0 dot sample (no token generation); `fw infer <prompt>` posts to an OpenAI-compatible endpoint; `fw tune bench` and `fw bench silicon` are CPU-only; `fw-llama-sys` loads libllama dynamically, so CUDA is a property of the libllama / llama-server binary, not of fw build flags. `fw-ggml` forbids unsafe and `dot_q8_0` is an auto-vectorised scalar loop; `.cargo/config.toml` pins `target-cpu=x86-64-v3`; release profile is fat LTO, cgu 1, strip, panic abort.
- PC llama stack: `C:\Users\aphro\.local\bin\llama-server.exe` is build 12317 (e0949d09b) Vulkan-only (`--list-devices` shows Vulkan0 RTX 4070); no `llama-bench`. nvcc 13.5 (and 13.4) and VS BuildTools 18 present, cmake and ninja present. Test model: ollama blob `~/.ollama/models/blobs/sha256-6dd7ac24fe13a238898e16e253ebc7a72fa27fb83f2658c2f6c241df3af5937d` (3.5 GB, gemma4 e2b).
- Plan note: `aphrody-labs/llama.cpp` fork exists (master, a300a9c shallow-cloned).

## Remaining steps
1. Merge blocker: `git merge origin/main` in `C:\src\fw` aborts because untracked files of other workers (plugins/aphrody/skills/ui-ux-pro-max/...) collide with origin; push from a throwaway clone (`git clone --depth 1`, apply, `git push origin HEAD:main`) or after the owners commit those files. Local main also holds a duplicate of b05bceb (identical content, merges cleanly).
2. Create crate `crates/fw-gpu` (lib + sibling bin `fw-gpu`, dispatched with `run_sibling("gpu", "fw-gpu", ...)` in `crates/fw-bin/src/main.rs`, `CommandMeta` in `cli_parser.rs`); modules: `recipes` (device-id table: 14 AD, 17 TU, 12 GA102, GA103, 29 GA104, 9 GA106 ids as in recipes.c, privileged and host register writes from registers.h, firmware names, status Stable/Experimental), `detect` (nvidia-smi `pci.device_id`, sysfs on Linux, WSL, Secure Boot, `/sys/module/gpu_unlock`, DKMS, cmdline), `unlock` (Linux only: `make -C third_party/gpu-unlock install`, add `gpu_unlock.targets=all` to cmdline, optional live reload; never on Windows), `bench` (embedded `cuda/tensor_bench.cu`, nvcc on demand, cuBLAS GemmEx FP16/BF16 with FP32 accumulate vs FP16 accumulate vs TF32 vs FP32; the FP32-acc/FP16-acc ratio of about 0.5 locked versus about 1.0 unlocked is an on-device verdict; on Windows build through vcvars64.bat from vswhere and put CUDA bin\x64 on PATH), `llm` bench (spawn llama-server, raw HTTP to /completion, read `timings`), `host` (cpuid brand/family/model to LLVM cpu: raptorlake, haswell, znver4; ISA tier).
3. Drift test: parse `third_party/gpu-unlock/kernel/gpu_unlock/recipes.c` and `registers.h` and compare with the Rust table; add `submodules: true` to the `actions/checkout` of the test job in `.github/workflows/ci.yml`.
4. `fw doctor` hardware section (CPU tier, llvm cpu, GPU, CUDA, kernel plan) and `fw tune profile` (per-host rustflags, LTO/PGO, scx, sysctl; write `configs/tune/<host>.toml` for pc, fr1, prd so the CachyOS fleet worker can apply them).
5. Measure: PC stock tensor TFLOPS; Vulkan llama-server (before) versus CUDA sm_89 build with flash attention and graphs on/off (after) in pp and tg tokens/s using the gemma4 blob, and `fw vram --vram-gb 12 --model-gb 3.26`. fr1: build fw release (haswell/v3 baseline versus `-C target-cpu=haswell`, fat LTO, PGO via llvm-profdata) in a private dir under `/srv/xbuild`, run `fw bench silicon` and `fw tune bench`. prd: run the cross-built znver4 binary for seconds only (production, keep it light). Report with the host load caveat for fr1.
6. Docs: `plugins/aphrody/skills/nvidia` section or new `gpu` skill (support matrix: Ampere stable, Ada and Turing experimental, Blackwell unsupported; commands), upstream entry in `upstreams.toml`, then `claude plugin update aphrody@fw`.

## Resume
```
cd C:/src/fw && git fetch origin && git log origin/main --oneline -3   # confirm b05bceb is present
cat docs/agents/queue/gpu-unlock-and-compute-tuning.md
nvidia-smi --query-gpu=name,pci.device_id,driver_version,compute_cap --format=csv
llama-server --list-devices
# CUDA llama.cpp (sm_89) build, in D:\build\llama.cpp (needs vcvars64 first):
#   cmake -S . -B build-cuda -G Ninja -DCMAKE_BUILD_TYPE=Release -DGGML_CUDA=ON -DCMAKE_CUDA_ARCHITECTURES=89 -DGGML_NATIVE=ON -DLLAMA_CURL=OFF
#   cmake --build build-cuda --target llama-server llama-bench -j 12
```

## Directories and files created (sizes measured)
- `D:\build\llama.cpp` 225 MB: shallow clone of aphrody-labs/llama.cpp. Safe to delete or reuse for the CUDA build; no build output exists (the CUDA configure never ran, no `build-cuda`).
- `D:\build\build-llama-cuda.bat` and `D:\build` itself: helper script that failed before configuring (it wrote no logs); safe to delete.
- `C:\src\tmp-fw-push`: throwaway clone used for the push, safe to delete.
- `C:\src\fw\third_party\gpu-unlock`: the submodule checkout, keep.
- No processes remain running; no GPU, driver or system setting was changed.
