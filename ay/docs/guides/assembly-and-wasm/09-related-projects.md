# 9. Related projects

This chapter compares our corelib (chapter 8) with existing WebAssembly-based kernels, runtimes and software-isolation or capability systems. Every project below was checked live on 2026-11-16 against its GitHub repository, README, paper page or documentation page; candidates that could not be verified were dropped (listed at the end). Star counts and push dates are snapshots from `gh api repos/OWNER/REPO` on 2026-11-16 and move daily. "Isolation" describes what the project's own README, paper or docs say, not an independent audit.

## Summary table

| Project | Isolation mechanism | Language | Status (last push, archived, license) | Link |
|---|---|---|---|---|
| **Our corelib** (this guide) | Wasm linear-memory sandbox plus a per-task capability table; no MMU | C23, assembly, WAT, Bun host | in this repository, `corelib/` | `corelib/src/` |
| nebulet | Wasm "usermode" in ring 0, single address space; compiled by Cranelift | Rust | 2023-02-28, archived, MIT, 2356 stars | https://github.com/nebulet/nebulet |
| kwast-os/kwast | Wasm as userspace, optional hardware protection domains | Rust | 2021-01-11, not archived, MIT, 177 stars | https://github.com/kwast-os/kwast |
| wasmerio/kernel-wasm | Wasm runtime inside the Linux kernel, software fault isolation | Rust | 2020-02-28, not archived, GPL-2.0, 756 stars | https://github.com/wasmerio/kernel-wasm |
| cisco-open/camblet-driver | wasm3 running in a Linux kernel module (OPA policies) | C | 2024-08-12, archived, license not detected, 82 stars | https://github.com/cisco-open/camblet-driver |
| WALI (Wasm-Thin-Kernel-Interfaces/WALI) | Thin Wasm interface exposing Linux syscalls; security deferred to higher layers | C | 2026-10-30, not archived, MIT, 224 stars | https://github.com/Wasm-Thin-Kernel-Interfaces/WALI |
| WAMR (wasm-micro-runtime/wasm-micro-runtime) | Wasm VM: interpreters and AOT, MCU to server | C | 2026-11-16, not archived, Apache-2.0, 6217 stars | https://github.com/wasm-micro-runtime/wasm-micro-runtime |
| wasmCloud/wasmCloud | Deny-by-default Wasm components, capabilities as interfaces | Rust | 2026-11-16, not archived, Apache-2.0, 2485 stars | https://github.com/wasmCloud/wasmCloud |
| spinframework/spin | Wasm components on Wasmtime | Rust | 2026-11-16, not archived, Apache-2.0, 6578 stars | https://github.com/spinframework/spin |
| lunatic-solutions/lunatic | Erlang-style Wasm processes, per-process permissions, channels | Rust | 2025-03-29, not archived, Apache-2.0, 4893 stars | https://github.com/lunatic-solutions/lunatic |
| bytecodealliance/wasmtime | Embeddable Wasm engine (Engine, Store, Linker, Module) | Rust | 2026-11-16, not archived, Apache-2.0, 18863 stars | https://github.com/bytecodealliance/wasmtime |
| cloudflare/workerd | Per-Worker isolation with capability bindings; README says not a hardened sandbox | C++ | 2026-11-16, not archived, Apache-2.0, 9204 stars | https://github.com/cloudflare/workerd |
| Singularity (Microsoft Research) | Software-isolated processes, message-only communication | Sing#/C# (per papers) | Research papers, 2006; no repository checked | see Sources |
| tock/tock | Rust kernel plus memory protection units for apps | Rust | 2026-11-16, not archived, license not detected (LICENSE-MIT and LICENSE-APACHE files), 6498 stars | https://github.com/tock/tock |
| redox-os/redox | Microkernel architecture (build-system mirror repo) | Rust | 2026-11-16, not archived, MIT, 16729 stars | https://github.com/redox-os/redox |
| seL4/seL4 | Microkernel, capability-based access control, formal proof | C | 2026-11-13, not archived, license not detected (README header: GPL-2.0-only), 5885 stars | https://github.com/seL4/seL4 |
| Fuchsia components | Sandboxed components, capabilities routed by a framework | docs | Documentation page (fuchsia.dev) | https://fuchsia.dev/fuchsia-src/concepts/components/v2/introduction |
| copy/v86 | x86 emulator in the browser, JIT to Wasm | JavaScript | 2026-10-30, not archived, BSD-2-Clause, 23694 stars | https://github.com/copy/v86 |
| leaningtech/webvm | x86-to-Wasm JIT (CheerpX) running Linux in the browser | Svelte | 2026-11-16, not archived, Apache-2.0, 17471 stars | https://github.com/leaningtech/webvm |

## Wasm-as-kernel-isolation

**Nebulet.** The README calls it "a microkernel that executes WebAssembly modules in ring 0 and a single address space to increase performance", with low context-switch overhead and "syscalls just being function calls". Cranelift compiles Wasm to native code before it runs. It began as a Google Summer of Code project in 2018 and the README says it is not ready to do anything yet. The repository is archived (https://github.com/nebulet/nebulet).

**Kwast.** "An operating system, written in Rust, running WebAssembly", microkernel design. The README says Wasm can run without hardware usermode or multiple address spaces, with several software-isolated processes in one hardware protection domain or in separate address spaces, and mentions optional hardware protection domains later. Its ABI starts from WASI. No push since 2021-01-11 (https://github.com/kwast-os/kwast).

**kernel-wasm.** Described as a "sandboxed kernel mode WebAssembly runtime" for Linux. The README lists WASI support (incomplete), an epoll networking extension, a modular host API provider and "software fault isolation", and argues that avoiding syscall context switches and `copy_{from,to}_user` improves performance. It cites an earlier project by the same author, Cervus (`cervus-v/cervus`, archived, GPL-2.0, last push 2018-06-03). The repository sits under the wasmerio organisation but has had no push since 2020-02-28 (https://github.com/wasmerio/kernel-wasm).

**camblet-driver.** A Linux kernel module of Cisco's Camblet system (TLS identity for TCP sockets). Its README says an early version ran OPA policies compiled to Wasm in the kernel via wasm3, with no floating point and no WASI in kernel space. Archived (https://github.com/cisco-open/camblet-driver).

**WALI.** The WebAssembly Linux Interface. Its site describes a "Linux syscall interface for Wasm" that treats Wasm as a software-defined ISA, based on the EuroSys 2025 paper "Empowering WebAssembly with Thin-Kernel Interfaces" (https://dl.acm.org/doi/10.1145/3689031.3717470, linked from the repository README; the ACM page returned HTTP 403 to automated fetches, so only the project site was read). The site says WALI is a base layer that defers security enforcement to higher-level APIs such as WASI. The baseline engine is built into WAMR (https://wasm-thin-kernel-interfaces.github.io/WALI/). The repository was found at `Wasm-Thin-Kernel-Interfaces/WALI`; `WebAssembly/WALI` does not exist.

Not found: `wasmerio/wasmer-os` returns HTTP 404 and no renamed repository turned up in search, so it is dropped.

## Wasm runtimes for embedded/RTOS and edge

**WAMR.** The repository moved to `wasm-micro-runtime/wasm-micro-runtime` (the README still lists the `bytecodealliance` location, which redirects). README: interpreters and an AOT compiler (`wamrc`), a core VM library of about 29.4K (AOT runtime) to 58.9K (fast interpreter) on Cortex-M4F, and RTOS ports including Zephyr, NuttX, RT-Thread, VxWorks and ESP-IDF/FreeRTOS (https://github.com/wasm-micro-runtime/wasm-micro-runtime).

**Wasmtime.** The Wasmtime book calls it "a standalone runtime for WebAssembly, WASI, and the Component Model" usable as a CLI or "a library embedded in a larger application" (https://docs.wasmtime.dev/). The `wasmtime` crate docs (version 50.0.2 when read) describe embedding: the host defines host functions, an `Engine` holds compilation state, a `Store` holds instances and memories, a `Linker` registers imports by name (https://docs.rs/wasmtime/latest/wasmtime/). Chapter 7 runs the corelib on this engine: the composed component executes on Wasmtime 50.0.2 and prints the golden boot transcript.

**wasmCloud.** A CNCF incubating project. The README says components are "deny-by-default": no file I/O, network or system calls unless a capability is granted, declared as interfaces in the component. Its `wash-runtime` crate "wraps Wasmtime with a plugin-based capability model" (https://github.com/wasmCloud/wasmCloud).

**Spin.** "A framework for building, deploying, and running fast, secure, and composable cloud microservices with WebAssembly", built on the component model and Wasmtime. `fermyon/spin` redirects to `spinframework/spin` (https://github.com/spinframework/spin).

**Lunatic.** "An Erlang-inspired runtime for WebAssembly". Its feature list includes process creation and cancellation, fine-grained process permissions, supervision and channel-based message passing. Last push 2025-03-29 (https://github.com/lunatic-solutions/lunatic).

**workerd.** The runtime behind Cloudflare Workers. Its README describes "capability bindings" instead of global namespaces and states "`workerd` is not a hardened sandbox": it tries to isolate each Worker but gives no defense in depth against implementation bugs, so hostile code must run inside a VM or similar (https://github.com/cloudflare/workerd).

**Component Model.** `WebAssembly/component-model` ("Repository for design and specification of the Component Model", pushed 2026-11-16 20:48Z, 1389 stars) holds the explainer, the WIT IDL, the binary format and the canonical ABI (https://github.com/WebAssembly/component-model). WASI itself lives in `WebAssembly/WASI` (https://github.com/WebAssembly/WASI).

## Software-isolated and capability systems

**Singularity.** "Deconstructing Process Isolation" (Aiken, Fahndrich, Hawblitzel, Hunt, Larus; ACM SIGPLAN Workshop on Memory Systems Performance and Correctness, 2006) says Singularity isolates software-isolated processes (SIPs) by language safety rules enforced by static type checking instead of page tables and privilege modes. The abstract reports that optional hardware isolation costs up to 25-33% on its benchmarks while "Software isolation has less than 5% overhead" (https://www.microsoft.com/en-us/research/publication/deconstructing-process-isolation/). "Language Support for Fast and Reliable Message-based Communication in Singularity OS" (EuroSys 2006) says message passing is the only communication mechanism between processes and that channel specifications catch mistakes at compile time (https://www.microsoft.com/en-us/research/publication/language-support-for-fast-and-reliable-message-based-communication-in-singularity-os/). Both pages answered `curl` with HTTP 403 and were read through a page fetcher.

**Tock.** README: an embedded OS for "multiple concurrent, mutually distrustful applications" on Cortex-M and RISC-V. It uses Rust to protect the kernel and drivers and "memory protection units to isolate applications from each other and the kernel" (https://github.com/tock/tock).

**Redox.** The repository is the build system; the README says Redox is an open-source OS written in Rust that "uses a microkernel architecture" (https://github.com/redox-os/redox). It is hardware-isolated, not a Wasm project.

**seL4.** The seL4 docs define a capability as "a unique, unforgeable token that constitutes a permission to access an entity or object in the system", note that several capabilities can point to the same object with different permissions, and show copying a capability between CSpace slots (https://docs.sel4.systems/Tutorials/capabilities.html). The repository README points to the website for details on its formal correctness proof (https://github.com/seL4/seL4). The reference manual and whitepaper PDFs resolve (HTTP 200) but could not be read through the page fetcher (the whitepaper URL returned a logo file), so nothing is claimed from them about rights reduction.

**Fuchsia.** The component framework page says each component is "a composable, sandboxed module that interacts with other components through capabilities", that "a capability combines access to a resource and a set of rights", and that a new program "does not have the ability to do anything -- not even to allocate memory" (https://fuchsia.dev/fuchsia-src/concepts/components/v2/introduction). The handles page says a handle references a kernel object, rights say which operations are allowed, and "Rights can be dropped" when a handle is duplicated (https://fuchsia.dev/fuchsia-src/concepts/kernel/handles).

## Browser and emulated systems

These run an unmodified x86 or Linux userland in an emulator, so they are not Wasm-native designs like ours.

**v86.** "x86 PC emulator and x86-to-wasm JIT, running in the browser". The README says machine code is translated to Wasm modules at runtime (https://github.com/copy/v86).

**WebVM.** Hosts https://webvm.io, "a Linux virtual machine that runs in your browser", "Linux ABI-compatible", powered by the CheerpX engine with an x86-to-Wasm JIT and a Linux syscall emulator (https://github.com/leaningtech/webvm).

## What we borrow, what differs

Our design is C, not Rust, by decision: ADR-0008 (`docs/adr/0008-sdk-languages-toolchain.md`, accepted 2026-11-17) accepts C and its derivatives, assembly (including WebAssembly text), Zig, Python, .NET and Bun for this repository, puts Rust in `aphrody-labs/aphrody`, and says "WebAssembly gets the same ABI as imports/exports and a WIT world"; `ay.toml` [sdk] lists `arch = ["x86_64", "aarch64", "riscv64", "wasm32", "wasm64"]` and `os` including `freestanding` and `wasi`. Chapter 7 is that sentence made concrete: the same C functions are exported once as `ay_*` for the plain module and once under the names of the WIT world `corelib`. Several projects above (Nebulet, Kwast, Tock, Redox, Lunatic, Wasmtime) are Rust; that is a difference in implementation language only, not in what is compared.

**Borrowed**
- **Capability handles with rights narrowing.** `CAP_DERIVE` needs the `DERIVE` right and creates a handle with `rights & mask` (`ay_cap_derive` in `corelib/src/cap.c`). Fuchsia duplicates handles with fewer rights; seL4 holds several capabilities to one object with different permissions. Differences: our handles are per-task slot indices into an 8-entry table, with three rights (`SEND`, `RECV`, `DERIVE`) and two object kinds (console, endpoint). There is no revocation tree, no transfer of handles over IPC and no `replace`.
- **One import as the only way out.** An app module imports exactly `ay.syscall(nr, a0, a1, a2) -> i32` (`corelib/wat/hello.wat`). WALI makes the same move at the opposite scale: a thin Wasm-to-kernel interface, with policy left to layers above. Our interface is eleven calls, not the Linux syscall table, and the capability check lives inside the corelib instead of in a higher layer.
- **Software isolation without an MMU.** Singularity's SIPs, Nebulet and Kwast rely on language or Wasm safety instead of hardware. Our Wasm build has the same trade: isolation is the Wasm linear-memory sandbox plus the capability table.
- **One code base, two targets.** The same freestanding C23 core (`corelib/src/*.c`, built with clang and lld) builds for bare-metal x86_64 (QEMU), for AArch64 and RISC-V assembly listings, and as a core Wasm module (`corelib/src/wasm.c`). ADR-0008 (`docs/adr/0008-sdk-languages-toolchain.md`) fixes the languages: C, assembly, WAT and Bun here, Rust only in `aphrody-labs/aphrody`. All ABI values are 32-bit words so the signature is one `i32` function on Wasm and a register convention natively (`corelib/include/ay_abi.h`). None of the READMEs and pages read for this chapter describes shipping both from one kernel source (not an exhaustive survey).
- **A host as platform driver.** Wasmtime's Engine/Store/Linker embedding, as used by Spin and wasmCloud, is the model for moving our host from Bun to wasmtime and the component model, which chapter 7 does for the boot flow. wasmCloud's "capability as a declared interface" is the component-model counterpart of our rights bits.
- **Message endpoints.** Our IPC is a bounded queue of four-word messages per endpoint, gated by `SEND` and `RECV` rights (`corelib/src/ipc.c`). Singularity makes message passing the only communication and types the channels at compile time; Lunatic offers channel-based message passing between Wasm processes. We have no typed channel contracts, no zero-copy exchange of buffers, and a full queue returns `AY_EAGAIN` instead of blocking.

**Differences and gaps (honest list)**
- **No MMU isolation for the Wasm build.** A bug in the host's Wasm engine or in the corelib module breaks isolation. Tock uses memory protection units for apps and seL4 relies on hardware plus a proof. workerd's own README warns that sandboxing without defense in depth is not enough for hostile code; the same applies to us.
- **No preemption of app code.** Wasm cannot suspend a running function, so tasks are cooperative `step()` state machines returning `Done`, `Yield` or `Fault` (chapter 8). A looping app stalls the host.
- **No verification.** seL4 has a formal correctness proof; we have unit tests and a conformance run that checks the import list and the `AY_EBADCAP` case.
- **Tiny scope.** Eight tasks, eight capabilities per task, four endpoints, 64 frames, no filesystem, no network; the corelib itself uses no WASI (only chapter 7's console component does). WAMR, Wasmtime and Spin are production runtimes; we are a reference ABI and corelib.
- **Frames are not memory protection.** `FRAME_ALLOC` tracks ownership in a bitmap; on Wasm a frame is carved out of linear memory, so an app's own linear memory is its isolation boundary, not the frame table.

## Sources

Accessed 2026-11-16 (the session ran past UTC midnight, so some page fetches may carry 2026-11-17). Repository facts come from `gh api repos/OWNER/REPO --jq '{full_name,stargazers_count,pushed_at,archived,license:.license.spdx_id,description,language}'` and `gh api repos/OWNER/REPO/readme`.

- https://github.com/nebulet/nebulet
- https://github.com/kwast-os/kwast
- https://github.com/wasmerio/kernel-wasm
- https://github.com/cervus-v/cervus
- https://github.com/cisco-open/camblet-driver
- https://github.com/Wasm-Thin-Kernel-Interfaces/WALI and https://wasm-thin-kernel-interfaces.github.io/WALI/
- https://dl.acm.org/doi/10.1145/3689031.3717470 (listed by the WALI README; not readable by automated fetch)
- https://github.com/wasm-micro-runtime/wasm-micro-runtime
- https://github.com/bytecodealliance/wasmtime, https://docs.wasmtime.dev/, https://docs.rs/wasmtime/latest/wasmtime/
- https://github.com/wasmCloud/wasmCloud
- https://github.com/spinframework/spin
- https://github.com/lunatic-solutions/lunatic
- https://github.com/cloudflare/workerd
- https://github.com/WebAssembly/component-model and https://github.com/WebAssembly/WASI
- https://www.microsoft.com/en-us/research/publication/deconstructing-process-isolation/
- https://www.microsoft.com/en-us/research/publication/language-support-for-fast-and-reliable-message-based-communication-in-singularity-os/
- https://github.com/tock/tock
- https://github.com/redox-os/redox
- https://github.com/seL4/seL4, https://docs.sel4.systems/Tutorials/capabilities.html, https://sel4.systems/Info/Docs/seL4-manual-latest.pdf (HTTP 200, unread)
- https://fuchsia.dev/fuchsia-src/concepts/components/v2/introduction and https://fuchsia.dev/fuchsia-src/concepts/kernel/handles
- https://github.com/copy/v86 and https://github.com/leaningtech/webvm
- Local: `corelib/src/{abi,cap,ipc,mem,engine,wasm}.c`, `corelib/include/ay_abi.h`, `corelib/wat/hello.wat`, `docs/adr/0008-sdk-languages-toolchain.md`, `ay.toml` [sdk]
