# 8. The corelib and its capability ABI

`corelib/` is the SDK's freestanding C23 corelib: no libc dependency (`corelib/src/libc.c` carries the four `mem*` functions the compiler may call), no heap, no global state outside two tables. It is the reference implementation of the syscall and capability ABI that the Aphrody OS (`aphrody-dev/aphrody`, pure Rust) consumes through the C ABI; this repository (`ay`) owns the ABI, its C header and its conformance runs. Nothing here claims to be the OS itself. The toolchain rules are in [ADR-0008](../../adr/0008-sdk-languages-toolchain.md): LLVM first, the C ABI as the stable boundary, WebAssembly as imports and exports plus a WIT world.

## Design in one page

- **Tasks and a scheduler.** A fixed table of tasks, each with a capability table and four message registers. `ay_core_schedule` picks the next ready task round-robin. A task runs one `step` and reports done, yield or fault. There is no preemption in the corelib: WebAssembly has no way to suspend a running function (the stack-switching proposal is still phase 3, chapter 6), so tasks are cooperative state machines that return to the scheduler.
- **A frame allocator.** 64 frames tracked by a bitmap with per-frame ownership. Allocation is "lowest clear bit" (`tzcnt` on x86-64, `i64.ctz` on Wasm); only the owner may free a frame.
- **An IPC bus.** Bounded queues of four-word messages, addressed by endpoint number. Sending and receiving need the matching right on a capability.
- **Capabilities.** A task holds handles, indices into its own table. A handle names an object (the console or an endpoint) and carries rights (`AY_RIGHT_SEND`, `AY_RIGHT_RECV`, `AY_RIGHT_DERIVE`). `AY_SYS_CAP_DERIVE` creates a new handle with `rights & mask`, so rights only ever shrink. A task cannot name an object it was not given.
- **One ABI.** Every value is a 32-bit word and there is one entry point, `ay_syscall(nr, a0, a1, a2) -> i32`, so the same signature is a single Wasm import and a register convention on native targets. Negative results are errors.

<!-- listing file:corelib/include/ay_abi.h#15-41 -->
```c
#define AY_SYS_YIELD        1   /* () -> 0 */
#define AY_SYS_LOG          2   /* (console cap, packed bytes, len 1..4) -> len; bytes little-endian */
#define AY_SYS_MSG_SET      3   /* (register 0..3, value) -> 0 */
#define AY_SYS_MSG_GET      4   /* (register 0..3) -> value */
#define AY_SYS_SEND         5   /* (endpoint cap) -> 0; sends the four message registers */
#define AY_SYS_RECV         6   /* (endpoint cap) -> 0; fills the four message registers */
#define AY_SYS_CAP_DERIVE   7   /* (cap, rights mask) -> new handle with rights & mask; needs AY_RIGHT_DERIVE */
#define AY_SYS_CAP_DROP     8   /* (cap) -> 0 */
#define AY_SYS_FRAME_ALLOC  9   /* () -> frame number */
#define AY_SYS_FRAME_FREE   10  /* (frame) -> 0; only the owner may free */

/* Negative results. */
#define AY_EPERM    (-1)  /* capability lacks the right, or the object is not yours */
#define AY_EBADCAP  (-2)  /* no such handle, or the wrong kind of object */
#define AY_EINVAL   (-3)  /* argument out of range */
#define AY_EAGAIN   (-4)  /* queue full or empty */
#define AY_ENOMEM   (-5)  /* table or frame pool exhausted */
#define AY_ENOSYS   (-6)  /* unknown syscall number */

/* Capability rights. */
#define AY_RIGHT_SEND    1u
#define AY_RIGHT_RECV    2u
#define AY_RIGHT_DERIVE  4u

/* Sizes and version. */
#define AY_ABI_VERSION  1
#define AY_MSG_WORDS    4
```
<!-- /listing -->

`include/ay_abi.h` is the source of truth for the numbers; the C sources include it, and the four runners below fail when a syscall's behaviour drifts from what the transcript expects.

## The dispatcher

<!-- listing c:corelib/src/engine.c#ay_core_syscall -->
```c
/// The single entry point of the syscall ABI, on behalf of the current task.
int32_t ay_core_syscall(ay_core *core, const ay_console *con, uint32_t nr, uint32_t a0, uint32_t a1, uint32_t a2) {
  uint8_t id = (uint8_t)core->current;
  ay_task *task = &core->tasks[core->current];
  ay_cap cap;
  int32_t err;
  switch (nr) {
  case AY_SYS_VERSION:
    return AY_ABI_VERSION;
  case AY_SYS_YIELD:
    return 0;
  case AY_SYS_LOG:
    if ((err = ay_cap_get(&task->caps, a0, AY_RIGHT_SEND, &cap)) < 0) return err;
    if (cap.object != AY_OBJ_CONSOLE) return AY_EBADCAP;
    if (a2 < 1 || a2 > 4) return AY_EINVAL;
    for (uint32_t i = 0; i < a2; i++) con->put(con->ctx, (uint8_t)(a1 >> (8 * i)));
    return (int32_t)a2;
  case AY_SYS_MSG_SET:
    if (a0 >= AY_MSG_WORDS) return AY_EINVAL;
    task->regs.w[a0] = a1;
    return 0;
  case AY_SYS_MSG_GET:
    if (a0 >= AY_MSG_WORDS) return AY_EINVAL;
    return (int32_t)task->regs.w[a0];
  case AY_SYS_SEND:
    if ((err = ay_cap_get(&task->caps, a0, AY_RIGHT_SEND, &cap)) < 0) return err;
    if (cap.object != AY_OBJ_ENDPOINT) return AY_EBADCAP;
    return ay_queue_send(core->queues, cap.endpoint, task->regs);
  case AY_SYS_RECV:
    if ((err = ay_cap_get(&task->caps, a0, AY_RIGHT_RECV, &cap)) < 0) return err;
    if (cap.object != AY_OBJ_ENDPOINT) return AY_EBADCAP;
    return ay_queue_recv(core->queues, cap.endpoint, &task->regs);
  case AY_SYS_CAP_DERIVE:
    return ay_cap_derive(&task->caps, a0, a1);
  case AY_SYS_CAP_DROP:
    return ay_cap_drop(&task->caps, a0);
  case AY_SYS_FRAME_ALLOC:
    return ay_frames_alloc(&core->frames, id);
  case AY_SYS_FRAME_FREE:
    return ay_frames_free(&core->frames, id, a0);
  default:
    return AY_ENOSYS;
  }
}
```
<!-- /listing -->

<!-- listing c:corelib/src/cap.c#ay_cap_derive -->
```c
/// New handle to the same object with `rights & mask`; needs the DERIVE right, so rights only shrink.
int32_t ay_cap_derive(ay_cap_table *table, uint32_t handle, uint32_t mask) {
  ay_cap cap;
  int32_t err = ay_cap_get(table, handle, AY_RIGHT_DERIVE, &cap);
  if (err < 0) return err;
  cap.rights &= mask;
  return ay_cap_install(table, cap);
}
```
<!-- /listing -->

## The same app twice

The boot app prints a greeting four bytes per `LOG` call, sends the words 1, 2, 3, 4 to its own endpoint, receives them back, sums them and prints `ipc ok`. It exists twice: in C (what the bare-metal and native images run) and in hand-written WAT (what the WebAssembly corelib runs). Both are the same state machine.

<!-- listing c:corelib/src/apps.c#ay_hello_step -->
```c
ay_step ay_hello_step(ay_hello *app, const ay_sys *sys) {
  switch (app->state) {
  case 0:
    PRINT(sys, "hello from ay app\n");
    app->state = 1;
    return AY_STEP_YIELD;
  case 1:
    for (uint32_t i = 0; i < 4; i++) sys->call(sys->ctx, AY_SYS_MSG_SET, i, i + 1, 0);
    if (sys->call(sys->ctx, AY_SYS_SEND, ENDPOINT, 0, 0) < 0) return AY_STEP_FAULT;
    app->state = 2;
    return AY_STEP_YIELD;
  default: {
    if (sys->call(sys->ctx, AY_SYS_RECV, ENDPOINT, 0, 0) < 0) return AY_STEP_FAULT;
    int32_t sum = 0;
    for (uint32_t i = 0; i < 4; i++) sum += sys->call(sys->ctx, AY_SYS_MSG_GET, i, 0, 0);
    if (sum == 10) PRINT(sys, "ipc ok\n");
    else PRINT(sys, "ipc bad\n");
    return AY_STEP_DONE;
  }
  }
}
```
<!-- /listing -->

<!-- listing file:corelib/wat/hello.wat -->
```wat
;; The boot app, hand-written. Same state machine as ay_hello_step in corelib/src/apps.c.
;; ABI: ay.syscall(nr, a0, a1, a2) -> i32. Capability handle 0 is the console, 1 an endpoint.
(module
  (import "ay" "syscall" (func $sys (param i32 i32 i32 i32) (result i32)))

  (memory (export "memory") 1)
  (data (i32.const 0)  "hello from ay app\n")   ;; 18 bytes
  (data (i32.const 32) "ipc ok\n")               ;; 7 bytes
  (data (i32.const 48) "ipc bad\n")              ;; 8 bytes

  (global $state (mut i32) (i32.const 0))

  ;; print(ptr, len): one LOG syscall (nr 2) per little-endian 4-byte word.
  (func $print (param $ptr i32) (param $len i32)
    (local $n i32)
    (block $done
      (loop $next
        (br_if $done (i32.eqz (local.get $len)))
        (local.set $n (select (local.get $len) (i32.const 4) (i32.lt_u (local.get $len) (i32.const 4))))
        (drop (call $sys (i32.const 2) (i32.const 0) (i32.load align=1 (local.get $ptr)) (local.get $n)))
        (local.set $ptr (i32.add (local.get $ptr) (i32.const 4)))
        (local.set $len (i32.sub (local.get $len) (local.get $n)))
        (br $next))))

  ;; step() -> 0 done, 1 yield, 2 fault
  (func (export "step") (result i32)
    (local $i i32)
    (local $sum i32)
    (if (i32.eqz (global.get $state))
      (then
        (call $print (i32.const 0) (i32.const 18))
        (global.set $state (i32.const 1))
        (return (i32.const 1))))
    (if (i32.eq (global.get $state) (i32.const 1))
      (then
        (loop $set                                   ;; MSG_SET(i, i + 1) for i in 0..4
          (drop (call $sys (i32.const 3) (local.get $i) (i32.add (local.get $i) (i32.const 1)) (i32.const 0)))
          (br_if $set (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 4))))
        (if (i32.lt_s (call $sys (i32.const 5) (i32.const 1) (i32.const 0) (i32.const 0)) (i32.const 0))   ;; SEND
          (then (return (i32.const 2))))
        (global.set $state (i32.const 2))
        (return (i32.const 1))))
    (if (i32.lt_s (call $sys (i32.const 6) (i32.const 1) (i32.const 0) (i32.const 0)) (i32.const 0))       ;; RECV
      (then (return (i32.const 2))))
    (loop $get                                       ;; sum of MSG_GET(i)
      (local.set $sum (i32.add (local.get $sum) (call $sys (i32.const 4) (local.get $i) (i32.const 0) (i32.const 0))))
      (br_if $get (i32.lt_u (local.tee $i (i32.add (local.get $i) (i32.const 1))) (i32.const 4))))
    (if (i32.eq (local.get $sum) (i32.const 10))
      (then (call $print (i32.const 32) (i32.const 7)))
      (else (call $print (i32.const 48) (i32.const 8))))
    (i32.const 0)))
```
<!-- /listing -->

An app module imports exactly one function, `ay.syscall`. It has no other way out of its sandbox, and every syscall is checked against the capabilities the corelib gave that task. The conformance test asserts both properties: the module's import list is `["ay.syscall"]`, and an app that calls `LOG` through a handle it never received gets `-2` (`AY_EBADCAP`).

## The WebAssembly host

The host is the only code outside Wasm: instantiate the corelib module, give it `ay_host.console_put` (its serial port), instantiate each app with `ay.syscall` bound to the corelib's `ay_syscall` export, and loop over the scheduler.

<!-- listing file:corelib/host/host.ts#33-48 -->
```ts
/** Boot the corelib, run every app to completion under its scheduler and return the serial transcript. */
export function run(coreWasm: Uint8Array, appWasms: Uint8Array[]): string {
  const bytes: number[] = [];
  const corelib = instantiateCore(coreWasm, b => bytes.push(b));
  if (corelib.ay_boot() !== 0) throw new Error("corelib self-test failed");
  const apps = new Map<number, AppExports>();
  for (const wasm of appWasms) {
    const id = corelib.ay_spawn();
    if (id < 0) throw new Error(`spawn failed: ${id}`);
    apps.set(id, instantiateApp(wasm, corelib));
  }
  for (let id = corelib.ay_sched_next(); id >= 0; id = corelib.ay_sched_next()) {
    corelib.ay_sched_report(id, apps.get(id)!.step());
  }
  return new TextDecoder().decode(new Uint8Array(bytes));
}
```
<!-- /listing -->

The module is built by `tools/corelib.ts` with `clang --target=wasm32 -mcpu=mvp -std=c23 -O2 -ffreestanding -nostdlib` and `wasm-ld`; the platform layer is `corelib/src/wasm.c`. Built with `-DAY_COMPONENT` the same C functions carry the canonical-ABI names of `corelib/wit/ay-corelib.wit` (chapter 7).

## The bare-metal image

`corelib/arch/x86_64` boots through the PVH protocol (`-kernel` in QEMU, also Xen, Firecracker and Cloud Hypervisor): `boot.s` goes from 32-bit protected mode to long mode (chapter 2) and calls `kmain` in `kmain.c`, which runs the same `ay_run_reference` every other runner runs. The serial port is a 16550 UART on COM1; the image reports pass or fail to QEMU through the `isa-debug-exit` device, which turns the written value `0x10` into process status 33.

The native image runs the C reference app, not the WAT module: it contains no Wasm interpreter. Running the Wasm app natively would need an embedded engine and is not part of the corelib.

## One transcript, four runs

The expected output is a committed file, and every runner must reproduce it byte for byte:

<!-- listing file:corelib/tests/golden/boot.txt -->
```text
ay-corelib abi 1
sched: order 0 1 0 1
mem: task 0 frames 0 1 2, freed 1 (0), realloc 1
mem: task 1 free of foreign frame 0 -> -1, 61 frames free
cap: derive console -1, derive endpoint as send-only 2, recv through it -1
ipc: task 1 send 0, checksum 0xe5bacf84
ipc: task 0 recv 0, checksum 0xe5bacf84, then empty -> -4
log: ok
selftest: ok
hello from ay app
ipc ok
core: task 0 exited
core: idle, no runnable task
```
<!-- /listing -->

| Run | Code | Proof |
|---|---|---|
| Native process | C reference app on the platform libc, built with clang and with clang-cl + lld-link | `bun test corelib/host/native.test.ts` |
| WebAssembly | corelib `.wasm` + hand-written `hello.wat` under Bun's JavaScriptCore | `bun test corelib/host/corelib.test.ts` |
| WebAssembly component | the same C built with `-DAY_COMPONENT`, composed with WAT parts, run on wasmtime | `bun tools/corelib-component.ts --remote` |
| Bare metal | `x86_64-unknown-none-elf` image in QEMU, serial to a file | `bun tools/corelib-qemu.ts --remote`, log `corelib/proof/qemu-boot.txt` |

The QEMU run executed on fr1 (QEMU 11.0.3, `-machine q35 -cpu qemu64`), exit status 33, 13 serial lines identical to the golden file; the same command runs on GitHub-hosted Ubuntu in the `corelib` CI job.

## Limits

- No MMU isolation in the corelib: on Wasm the engine isolates modules, on bare metal the corelib image is a single ring-0 program that demonstrates the ABI. Process isolation on real hardware belongs to the OS that consumes the ABI.
- Scheduling is cooperative and the tables are fixed-size (8 tasks, 8 capabilities per task, 4 endpoints of 8 messages, 64 frames); the numbers are enumerators in `corelib/src/core.h`.
- `LOG` carries at most four bytes per call by design: no pointers cross the ABI, so no module ever reads another module's memory. A pointer-carrying syscall would be built on the multiple-memories feature (chapter 6).
- The self-test and the app talk to the corelib through function pointers (`ay_console`, `ay_sys`), so the same code runs where the console is a UART, a host import or `stdout`.
