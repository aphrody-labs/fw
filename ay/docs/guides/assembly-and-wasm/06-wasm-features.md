# 6. WebAssembly features: SIMD, threads, GC, exceptions, tail calls, memories

Chapter 5 covered the core language. WebAssembly grows by **proposals** that move through phases; a finished proposal is merged into the core specification. This chapter states what is standard today, then takes one feature at a time with a minimal hand-written example from `corelib/wat/examples/`, validated with `wasm-tools validate --features all` and run by `corelib/host/wat-examples.test.ts` on the engines named in each section. Everything below was read or run on 2026-11-17.

## Standards status

| Item | Status | Source |
|---|---|---|
| Wasm 1.0 | W3C **Recommendation**, 2019-12-05 | [api.w3.org/specifications/wasm-core-1/versions/latest](https://api.w3.org/specifications/wasm-core-1/versions/latest): `"status": "Recommendation"`, `"date": "2019-12-05"` |
| `wasm-core-2` | W3C **Candidate Recommendation Draft**, dated 2026-11-16, not a Recommendation. The document at this shortlink says "release 3.0" and "version 3.0 (2026-11-16)": the specification text published under the `wasm-core-2` name is the 3.0 text. | [www.w3.org/TR/wasm-core-2/](https://www.w3.org/TR/wasm-core-2/), [api.w3.org/specifications/wasm-core-2/versions/latest](https://api.w3.org/specifications/wasm-core-2/versions/latest) (`"status": "Candidate Recommendation Draft"`, `"uri": "https://www.w3.org/TR/2026/CRD-wasm-core-2-20261116/"`) |
| What webassembly.org lists | "WebAssembly Specification (Wasm 3.0)" as the current core specification; older versions: "W3C Candidate Recommendation Draft, 16 June 2025 (Wasm 2.0)" and "W3C Recommendation, 5 December 2019 (Wasm 1.0)" | [webassembly.org/specs](https://webassembly.org/specs/) |
| Finished proposals | 1.0: MVP, import/export of mutable globals. 2.0: non-trapping float-to-int, sign-extension, multi-value, JS BigInt to i64 integration, reference types, bulk memory, fixed-width SIMD. 3.0: tail call, extended constant expressions, typed function references, GC, multiple memories, relaxed SIMD, custom annotation syntax in the text format, branch hinting, exception handling, JS string builtins, memory64. | [WebAssembly/proposals finished-proposals.md](https://github.com/WebAssembly/proposals/blob/main/finished-proposals.md) |
| Not finished | threads: phase 4; wide arithmetic: phase 4; JS Promise Integration: phase 5; stack switching: phase 3; custom page sizes: phase 3; component model: phase 1 | `features.json` of [WebAssembly/website](https://github.com/WebAssembly/website/blob/main/features.json) (last commit 2026-11-10), rendered at [webassembly.org/features](https://webassembly.org/features/) |

Phases are the proposal process of the WebAssembly CG: phase 4 is "standardize the feature", phase 5 is "the feature is standardized" (and for the finished proposals above, merged). A feature in phase 3 or 4 may still change. The consequence for tool users: *engines ship features before they are merged* (JSPI is in phase 5 and ships in Chrome 137, Firefox 153, Safari 27), and *a proposal may be finished and still missing from an engine* (memory64 is in 3.0 and not in the Bun used below).

## Engine support

Two sources, kept apart. **Observed** columns were run here with the example files below. **Cited** columns are the release numbers of `features.json` (data behind [webassembly.org/features](https://webassembly.org/features/)); they were not run here. Chrome/V8 is cited only.

| Feature | Bun 1.4.5-canary (JavaScriptCore), observed | wasmtime 50.0.2, observed | Chrome / Firefox / Safari / Node.js, cited | wasmtime, cited |
|---|---|---|---|---|
| Fixed-width SIMD | runs | runs, default | 91 / 89 / 16.4 / 16.4 | 0.33 |
| Relaxed SIMD | runs | runs, default | 114 / 145 / flag / 21.0 | 15 |
| Threads and atomics | runs, incl. 4 workers on one shared memory | runs with `-W shared-memory` (default: "shared memory support is disabled for this engine") | 74 / 79 / 14.1 / 16.4 | 15 |
| GC | runs | runs, default | 119 / 120 / 18.2 / 22.0 | 47 |
| Exception handling (`try_table`) | runs | runs, default | 137 / 131 / 18.4 / 24.15 | 47 |
| Tail calls | runs | runs, default | 112 / 121 / 18.2 / 20.0 | 22 |
| Multiple memories | runs | runs, default | 120 / 125 / not listed / 22.0 | 15 |
| Memory64 | **rejects**: `Memory64 is not enabled` | runs, default | 133 / 134 / flag / 24.0 | 30 |
| Custom page sizes | **rejects**: `resizable limits flag are not valid 08` | runs with `-W custom-page-sizes`; default: `the custom page sizes proposal must be enabled to customize a memory's page size` | not listed in any of the four | flag |
| JSPI | runs: `WebAssembly.Suspending` and `WebAssembly.promising` exist | no JS API in the CLI, not run | 137 / 153 / 27 / 26.0 | no |
| Stack switching | **rejects**: `1th Type is non-Func, non-Struct, and non-Array -35` | runs with `-W stack-switching` on Linux x86-64 (`fr1`); on Windows: `the wasm_stack_switching feature is not supported on this compiler configuration` | flag in Chrome, not listed for the others | not listed |

The "cited" cells quote `features.json`; "flag" means the entry exists only behind a flag. Bun 1.4.5-canary (`1.4.5-canary.1+9098897a1`) is the runtime of this repository; its JavaScriptCore is a fork, so its status can differ from Safari's. The rejections are recorded as test expectations: `wat-examples.test.ts` asserts the exact error and runs the real behavior assertions the moment a newer build accepts the module.

wasmtime was installed from the release asset `wasmtime-v50.0.2-x86_64-linux.tar.xz` of [bytecodealliance/wasmtime v50.0.2](https://github.com/bytecodealliance/wasmtime/releases/tag/v50.0.2) (published 2026-11-16) under `/srv/xbuild/wasmtime` on `fr1`, and the Windows zip of the same release for the test; `wat-examples.test.ts` runs a wasmtime block when `wasmtime` is on `PATH` or `AY_WASMTIME` is set.

## The clang side: CPUs and features

The corelib is freestanding C23 built with clang and wasm-ld (ADR-0008; `tools/corelib.ts`). clang chooses WebAssembly features through the CPU (`-mcpu`) and per-feature switches (`-msimd128`, `-matomics`, `-mbulk-memory`, `-mtail-call`, `-mexception-handling`, `-mmultimemory`, `-mgc`, ...). The corelib builds with `-mcpu=mvp`; the guide's variants add `-msimd128` (`simd`) and `-matomics -mbulk-memory` (`atomics`). Observed with clang 23.1.5 and LLD 23.1.5: `clang --target=wasm32 -mcpu=help` lists the CPUs `bleeding-edge`, `generic`, `lime1` and `mvp`, and the `target_features` custom section that the linker writes (read with a small decoder of the section) is:

| `-mcpu` | `target_features` of the linked module |
|---|---|
| `mvp` | no `target_features` section at all |
| `generic` (the default) | `+mutable-globals +nontrapping-fptoint +bulk-memory +sign-ext +reference-types +multivalue +bulk-memory-opt +call-indirect-overlong` |
| `lime1` | `+mutable-globals +nontrapping-fptoint +sign-ext +multivalue +extended-const +bulk-memory-opt +call-indirect-overlong` |
| `bleeding-edge` | `+atomics +mutable-globals +nontrapping-fptoint +simd128 +bulk-memory +sign-ext +exception-handling +tail-call +reference-types +multivalue +gc +relaxed-simd +extended-const +multimemory +fp16 +bulk-memory-opt +call-indirect-overlong +wide-arithmetic +relaxed-atomics`; clang prints `'+compact-imports' is not a recognized feature for this target (ignoring feature)` |
| `mvp -msimd128` (the `simd` variant of the corelib) | `+mutable-globals +simd128 +sign-ext` |
| `mvp -matomics -mbulk-memory` (the `atomics` variant) | `+atomics +mutable-globals +bulk-memory +sign-ext +bulk-memory-opt` |
| `generic --target=wasm64` | the `generic` set plus `+memory64` |

The default CPU is not the 1.0 language, and the code differs visibly. On a probe with `__builtin_memmove` and `(int32_t)f`: `generic` emits one `memory.copy` and `i32.trunc_sat_f32_s`; `-mcpu=mvp` emits neither, it needs a `memmove` function (linking a freestanding build fails with `wasm-ld: error: ... undefined symbol: memmove`, which is why `corelib/src/libc.c` defines `memmove`, `memcpy`, `memset`, `memcmp`) and turns the float conversion into `f32.abs`, a compare against 2^31 and `i32.trunc_f32_s` inside a branch.

`-mcpu=mvp` is not a guarantee of Wasm 1.0 output, though. The corelib's base module passes `wasm-tools validate --features mvp` (observed), so engines since 2019 run it, but the probe function `int32_t sx(int32_t x) { return (int8_t)x; }` compiled with `-mcpu=mvp -O2` contains `i32.extend8_s`, a sign-extension instruction, and `wasm-tools validate --features mvp` rejects it with `sign extension operations support is not enabled`; `-mno-sign-ext` did not change that, and the same file at `-O0` was valid. Validate the module against the feature set you claim; do not infer it from the flags.

A feature in the *module* is a feature the *runtime* must have: a build is only as portable as its `target_features` section, and an `mvp` build has none, so the section is also no proof of portability.

## SIMD: `v128`

`v128` is one 128-bit value that instructions view as lanes: `i8x16`, `i16x8`, `i32x4`, `i64x2`, `f32x4`, `f64x2`. Operations are lane-wise (`i32x4.add`), lane access (`i32x4.extract_lane 0`, `replace_lane`), shuffles (`i8x16.shuffle` with 16 constant lane indices), compares that give all-ones or all-zeros lanes, and `bitmask`. Loads and stores are `v128.load`/`v128.store` with the usual `align` and `offset`. Fixed-width SIMD was finished in Wasm 2.0 (finished-proposals). It is **deterministic**: the same input gives the same bits on every engine.

The same loop in five places. C:

<!-- listing c:corelib/src/ipc.c#ay_sum -->
```c
/// Wrapping sum of words: the loop a compiler turns into SIMD when the target has vector registers.
AY_API uint32_t ay_sum(const uint32_t *words, size_t len) {
  uint32_t acc = 0;
  for (size_t i = 0; i < len; i++) acc += words[i];
  return acc;
}
```
<!-- /listing -->

Default Wasm (`-mcpu=mvp`, no `-msimd128`): LLVM unrolled by four, with scalar `i32.add`s:

<!-- listing wat:ay_sum -->
```wat
(func $ay_sum (export "ay_sum") (type 3) (param i32 i32) (result i32)
  (local i32 i32 i32 i32)
  block ;; label = @1
    local.get 1
    br_if 0 (;@1;)
    i32.const 0
    return
  end
  local.get 1
  i32.const 3
  i32.and
  local.set 2
  i32.const 0
  local.set 3
  i32.const 0
  local.set 4
  block ;; label = @1
    block ;; label = @2
      local.get 1
      i32.const 4
      i32.lt_u
      br_if 0 (;@2;)
      local.get 1
      i32.const -4
      i32.and
      local.set 5
      i32.const 0
      local.set 3
      local.get 0
      local.set 1
      i32.const 0
      local.set 4
      loop ;; label = @3
        local.get 1
        i32.const 12
        i32.add
        i32.load
        local.get 1
        i32.const 8
        i32.add
        i32.load
        local.get 1
        i32.const 4
        i32.add
        i32.load
        local.get 1
        i32.load
        local.get 4
        i32.add
        i32.add
        i32.add
        i32.add
        local.set 4
        local.get 1
        i32.const 16
        i32.add
        local.set 1
        local.get 5
        local.get 3
        i32.const 4
        i32.add
        local.tee 3
        i32.ne
        br_if 0 (;@3;)
      end
      local.get 2
      i32.eqz
      br_if 1 (;@1;)
    end
    local.get 0
    local.get 3
    i32.const 2
    i32.shl
    i32.add
    local.set 1
    loop ;; label = @2
      local.get 1
      i32.load
      local.get 4
      i32.add
      local.set 4
      local.get 1
      i32.const 4
      i32.add
      local.set 1
      local.get 2
      i32.const -1
      i32.add
      local.tee 2
      br_if 0 (;@2;)
    end
  end
  local.get 4
)
```
<!-- /listing -->

With `-msimd128`, **clang does vectorize it** (observed, clang 23.1.5, `-O2`): a `v128.load` and `i32x4.add` loop accumulating four lanes, then a shuffle-and-add reduction and a scalar epilogue for the remaining words:

<!-- listing wat:ay_sum:simd -->
```wat
(func $ay_sum (export "ay_sum") (type 3) (param i32 i32) (result i32)
  (local i32 i32 v128 i32)
  block ;; label = @1
    local.get 1
    br_if 0 (;@1;)
    i32.const 0
    return
  end
  i32.const 0
  local.set 2
  i32.const 0
  local.set 3
  block ;; label = @1
    block ;; label = @2
      local.get 1
      i32.const 4
      i32.lt_u
      br_if 0 (;@2;)
      v128.const i32x4 0x00000000 0x00000000 0x00000000 0x00000000
      local.set 4
      local.get 0
      local.set 5
      local.get 1
      i32.const -4
      i32.and
      local.tee 2
      local.set 3
      loop ;; label = @3
        local.get 5
        v128.load align=4
        local.get 4
        i32x4.add
        local.set 4
        local.get 5
        i32.const 16
        i32.add
        local.set 5
        local.get 3
        i32.const -4
        i32.add
        local.tee 3
        br_if 0 (;@3;)
      end
      local.get 4
      local.get 4
      local.get 4
      i8x16.shuffle 8 9 10 11 12 13 14 15 0 1 2 3 0 1 2 3
      i32x4.add
      local.tee 4
      local.get 4
      local.get 4
      i8x16.shuffle 4 5 6 7 0 1 2 3 0 1 2 3 0 1 2 3
      i32x4.add
      i32x4.extract_lane 0
      local.set 3
      local.get 1
      local.get 2
      i32.eq
      br_if 1 (;@1;)
    end
    local.get 1
    local.get 2
    i32.sub
    local.set 1
    local.get 0
    local.get 2
    i32.const 2
    i32.shl
    i32.add
    local.set 5
    loop ;; label = @2
      local.get 5
      i32.load
      local.get 3
      i32.add
      local.set 3
      local.get 5
      i32.const 4
      i32.add
      local.set 5
      local.get 1
      i32.const -1
      i32.add
      local.tee 1
      br_if 0 (;@2;)
    end
  end
  local.get 3
)
```
<!-- /listing -->

On x86-64 it does not: the guide's flags for that target (`tools/corelib.ts`: `--target=x86_64-unknown-none-elf -mgeneral-regs-only -mno-red-zone`) forbid vector registers, so the loop stays a scalar unroll of eight adds (the `push rbp` prologue is clang's frame pointer):

<!-- listing asm:ay_sum:x86_64 -->
```asm
ay_sum:
	push	rbp
	mov	rbp, rsp
	test	rsi, rsi
	je	.LBB1_1
	mov	ecx, esi
	and	ecx, 7
	cmp	rsi, 8
	jae	.LBB1_8
	xor	edx, edx
	xor	eax, eax
	jmp	.LBB1_5
.LBB1_1:
	xor	eax, eax
	pop	rbp
	ret
.LBB1_8:
	and	rsi, -8
	xor	edx, edx
	xor	eax, eax
.LBB1_9:
	add	eax, dword ptr [rdi + 4*rdx]
	add	eax, dword ptr [rdi + 4*rdx + 4]
	add	eax, dword ptr [rdi + 4*rdx + 8]
	add	eax, dword ptr [rdi + 4*rdx + 12]
	add	eax, dword ptr [rdi + 4*rdx + 16]
	add	eax, dword ptr [rdi + 4*rdx + 20]
	add	eax, dword ptr [rdi + 4*rdx + 24]
	add	eax, dword ptr [rdi + 4*rdx + 28]
	add	rdx, 8
	cmp	rsi, rdx
	jne	.LBB1_9
	test	rcx, rcx
	je	.LBB1_7
.LBB1_5:
	lea	rdx, [rdi + 4*rdx]
	xor	esi, esi
.LBB1_6:
	add	eax, dword ptr [rdx + 4*rsi]
	inc	rsi
	cmp	rcx, rsi
	jne	.LBB1_6
.LBB1_7:
	pop	rbp
	ret
```
<!-- /listing -->

AArch64 (`--target=aarch64-none-elf`) has NEON in its baseline and the vectorizer uses it with no flag: `ldp q4, q5` loads, four accumulators updated with `add v0.4s, ...`, and an `addv s0, v0.4s` reduction:

<!-- listing asm:ay_sum:aarch64 -->
```asm
ay_sum:
	cbz	x1, .LBB1_3
	cmp	x1, #4
	b.hs	.LBB1_4
	mov	x9, xzr
	mov	w8, wzr
	b	.LBB1_13
.LBB1_3:
	mov	w8, wzr
	mov	w0, w8
	ret
.LBB1_4:
	cmp	x1, #16
	b.hs	.LBB1_6
	mov	x9, xzr
	mov	w8, wzr
	b	.LBB1_10
.LBB1_6:
	movi	v0.2d, #0000000000000000
	movi	v1.2d, #0000000000000000
	and	x10, x1, #0xc
	movi	v2.2d, #0000000000000000
	movi	v3.2d, #0000000000000000
	and	x9, x1, #0xfffffffffffffff0
	add	x8, x0, #32
	and	x11, x1, #0xfffffffffffffff0
.LBB1_7:
	ldp	q4, q5, [x8, #-32]
	subs	x11, x11, #16
	ldp	q6, q7, [x8], #64
	add	v0.4s, v4.4s, v0.4s
	add	v1.4s, v5.4s, v1.4s
	add	v2.4s, v6.4s, v2.4s
	add	v3.4s, v7.4s, v3.4s
	b.ne	.LBB1_7
	add	v0.4s, v1.4s, v0.4s
	cmp	x1, x9
	add	v0.4s, v2.4s, v0.4s
	add	v0.4s, v3.4s, v0.4s
	addv	s0, v0.4s
	fmov	w8, s0
	b.eq	.LBB1_15
	cbz	x10, .LBB1_13
.LBB1_10:
	movi	v0.2d, #0000000000000000
	mov	x10, x9
	and	x9, x1, #0xfffffffffffffffc
	mov	v0.s[0], w8
	add	x8, x0, x10, lsl #2
	sub	x10, x10, x9
.LBB1_11:
	ldr	q1, [x8], #16
	adds	x10, x10, #4
	add	v0.4s, v1.4s, v0.4s
	b.ne	.LBB1_11
	addv	s0, v0.4s
	cmp	x1, x9
	fmov	w8, s0
	b.eq	.LBB1_15
.LBB1_13:
	add	x10, x0, x9, lsl #2
	sub	x9, x1, x9
.LBB1_14:
	ldr	w11, [x10], #4
	subs	x9, x9, #1
	add	w8, w11, w8
	b.ne	.LBB1_14
.LBB1_15:
	mov	w0, w8
	ret
```
<!-- /listing -->

Written by hand, the same loop and a lane compare (`any_equal` returns the `i32x4.bitmask` of `lane == x`; the test checks `any_equal(30) == 0b0100`):

<!-- listing file:corelib/wat/examples/simd.wat -->
```wat
;; Fixed-width SIMD: one v128 holds four i32 lanes. Same job as ay_sum: wrapping sum of n words.
(module
  (memory (export "memory") 1)

  ;; sum4(ptr): add the four lanes loaded at ptr into one scalar.
  (func (export "sum4") (param $ptr i32) (result i32)
    (local $v v128)
    (local.set $v (v128.load (local.get $ptr)))
    (i32.add
      (i32.add (i32x4.extract_lane 0 (local.get $v)) (i32x4.extract_lane 1 (local.get $v)))
      (i32.add (i32x4.extract_lane 2 (local.get $v)) (i32x4.extract_lane 3 (local.get $v)))))

  ;; sum(ptr, n): n words, 4 per iteration in a vector accumulator, n a multiple of 4.
  (func (export "sum") (param $ptr i32) (param $n i32) (result i32)
    (local $acc v128)
    (local $end i32)
    (local.set $end (i32.add (local.get $ptr) (i32.shl (local.get $n) (i32.const 2))))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $ptr) (local.get $end)))
        (local.set $acc (i32x4.add (local.get $acc) (v128.load (local.get $ptr))))
        (local.set $ptr (i32.add (local.get $ptr) (i32.const 16)))
        (br $next)))
    (i32.add
      (i32.add (i32x4.extract_lane 0 (local.get $acc)) (i32x4.extract_lane 1 (local.get $acc)))
      (i32.add (i32x4.extract_lane 2 (local.get $acc)) (i32x4.extract_lane 3 (local.get $acc)))))

  ;; A v128 constant and a lane-wise compare: bitmask packs one bit per lane.
  (func (export "any_equal") (param $x i32) (result i32)
    (i32x4.bitmask (i32x4.eq (i32x4.splat (local.get $x)) (v128.const i32x4 10 20 30 40)))))
```
<!-- /listing -->

### Relaxed SIMD

Relaxed SIMD (finished in 3.0) adds instructions that are *allowed to differ* between machines, so each can map to the best native instruction: `f32x4.relaxed_madd` (fused or not), `relaxed_laneselect`, `relaxed_min/max`, `i16x8.relaxed_q15mulr_s`, `i16x8.relaxed_dot_i8x16_i7x16_s`, `relaxed_trunc`. The proposal states the lane select rule: "If each lane-sized mask in `m` has all bits set or all bits unset, these instructions behave the same as `v128.bitselect`. Otherwise, the result is implementation defined" ([relaxed-simd Overview.md](https://github.com/WebAssembly/relaxed-simd/blob/main/proposals/relaxed-simd/Overview.md), read 2026-11-17). The core spec calls these instructions "non-deterministic and implementation-dependent". The test therefore asserts `madd` on exact integers (`2*10+1 = 21`), a clean lane select, and only the in-range lanes of `trunc`; on Bun on x86-64 a NaN lane and 1e10 both gave `0x80000000`.

<!-- listing file:corelib/wat/examples/relaxed_simd.wat -->
```wat
;; Relaxed SIMD: operations whose result is implementation-defined for some inputs.
;; The module is valid wherever the feature exists, but results may differ between engines and CPUs.
;; Each function reads vectors at ptr, ptr+16, ptr+32 and stores its result at ptr+48.
(module
  (memory (export "memory") 1)

  ;; Fused multiply-add a*b+c: rounded once (fused) or twice (separate multiply and add), at the engine's choice.
  (func (export "madd") (param $p i32)
    (v128.store offset=48 (local.get $p)
      (f32x4.relaxed_madd
        (v128.load (local.get $p))
        (v128.load offset=16 (local.get $p))
        (v128.load offset=32 (local.get $p)))))

  ;; Bitwise select: bit-exact when every mask byte (at ptr+32) is all ones or all zeros,
  ;; implementation-defined for any other mask byte.
  (func (export "laneselect") (param $p i32)
    (v128.store offset=48 (local.get $p)
      (i8x16.relaxed_laneselect
        (v128.load (local.get $p))
        (v128.load offset=16 (local.get $p))
        (v128.load offset=32 (local.get $p)))))

  ;; Float to int: in-range lanes are exact; NaN and out-of-range lanes are implementation-defined.
  (func (export "trunc") (param $p i32)
    (v128.store offset=48 (local.get $p)
      (i32x4.relaxed_trunc_f32x4_s (v128.load (local.get $p))))))
```
<!-- /listing -->

Rule of thumb: use deterministic SIMD unless a result that differs on another CPU is acceptable (ML kernels, graphics), and never hash or serialize a relaxed result.

## Threads and atomics

The threads proposal (phase 4, not merged into 3.0) adds a **shared** memory (`(memory 1 1 shared)`; a maximum is required), atomic loads, stores and read-modify-writes, compare-and-swap, `atomic.fence`, and two blocking primitives: `memory.atomic.wait32/64` (park the thread if the word still equals `expected`; result 0 woken, 1 value differed, 2 timed out) and `memory.atomic.notify` (wake up to `count` waiters, returns how many). Instances on different threads share the memory object; the threads themselves come from the host (Web Workers, `pthread`, an embedder). Wasm defines no way to start a thread. Atomic accesses must be naturally aligned or they trap (chapter 5).

The corelib's spin lock `ay_try_acquire` (`corelib/src/sync.c`) is one C11 compare-exchange, `atomic_compare_exchange_strong_explicit(flag, &expected, 1, memory_order_acquire, memory_order_relaxed)`. Built for `-mcpu=mvp` without `-matomics`, clang lowers it to a plain load, select and store, which is **not atomic**:

<!-- listing wat:ay_try_acquire -->
```wat
(func $ay_try_acquire (export "ay_try_acquire") (type 4) (param i32) (result i32)
  (local i32)
  local.get 0
  local.get 0
  i32.load
  local.tee 1
  i32.const 1
  local.get 1
  select
  i32.store
  local.get 1
  i32.eqz
)
```
<!-- /listing -->

With `-matomics -mbulk-memory` the same C becomes the Wasm instruction `i32.atomic.rmw.cmpxchg`:

<!-- listing wat:ay_try_acquire:atomics -->
```wat
(func $ay_try_acquire (export "ay_try_acquire") (type 4) (param i32) (result i32)
  local.get 0
  i32.const 0
  i32.const 1
  i32.atomic.rmw.cmpxchg
  i32.eqz
)
```
<!-- /listing -->

And on the CPUs, x86-64 `lock cmpxchg`, and AArch64 a load-acquire-exclusive and store-exclusive loop (`ldaxr`/`stxr`) that retries when the exclusive monitor was lost:

<!-- listing asm:ay_try_acquire:x86_64 -->
```asm
ay_try_acquire:
	push	rbp
	mov	rbp, rsp
	mov	edx, 1
	xor	ecx, ecx
	xor	eax, eax
	lock		cmpxchg	dword ptr [rdi], edx
	sete	cl
	mov	eax, ecx
	pop	rbp
	ret
```
<!-- /listing -->

<!-- listing asm:ay_try_acquire:aarch64 -->
```asm
ay_try_acquire:
.LBB0_1:
	ldaxr	w8, [x0]
	cbnz	w8, .LBB0_4
	mov	w8, #1
	stxr	w9, w8, [x0]
	cbnz	w9, .LBB0_1
	mov	w0, w8
	ret
.LBB0_4:
	mov	w8, wzr
	clrex
	mov	w0, w8
	ret
```
<!-- /listing -->

Link-time observations for the `atomics` variant (clang 23.1.5; neither build printed a warning): `-matomics -mbulk-memory` links as is, and the memory stays **unshared** (`(memory 2)`), which is valid for atomic instructions; `-matomics` alone also links. Asking for a shared memory with `-Wl,--shared-memory` needs bulk memory: without `-mbulk-memory` wasm-ld stops with `error: 'bulk-memory' feature must be used in order to use shared memory`; with it the memory becomes `(memory 2 2 shared)` with no `--max-memory` flag, the module gains a second mutable `i32` global initialised to 0, and Bun reports `memory.buffer instanceof SharedArrayBuffer` as true. In a threaded build the memory would be shared this way.

The same function, three machine models: Wasm exposes one portable atomic instruction and the engine chooses the CPU sequence. The hand-written module has the lock, an atomic add, `wait`, `notify` and `fence`, and takes a shared memory as an **import** so that several instances, one per thread, can use it:

<!-- listing file:corelib/wat/examples/threads.wat -->
```wat
;; Threads proposal: shared memory plus atomic read-modify-write, wait and notify.
;; The spin lock is ay_try_acquire in corelib/src/sync.c; the futex-style pair parks a thread instead of spinning.
(module
  (import "env" "memory" (memory 1 1 shared))

  ;; ay_try_acquire: flag 0 -> 1 with one atomic compare-and-swap; 1 when this caller got the lock.
  (func (export "try_acquire") (param $flag i32) (result i32)
    (i32.eqz (i32.atomic.rmw.cmpxchg (local.get $flag) (i32.const 0) (i32.const 1))))

  (func (export "release") (param $flag i32)
    (i32.atomic.store (local.get $flag) (i32.const 0)))

  (func (export "add") (param $addr i32) (param $n i32) (result i32)
    (i32.atomic.rmw.add (local.get $addr) (local.get $n)))

  ;; wait(addr, expected, timeout_ns): 0 woken, 1 value differed, 2 timed out.
  (func (export "wait") (param $addr i32) (param $expected i32) (param $timeout i64) (result i32)
    (memory.atomic.wait32 (local.get $addr) (local.get $expected) (local.get $timeout)))

  ;; notify(addr, count): number of waiters woken.
  (func (export "notify") (param $addr i32) (param $count i32) (result i32)
    (memory.atomic.notify (local.get $addr) (local.get $count)))

  (func (export "fence") (atomic.fence)))
```
<!-- /listing -->

Observed on Bun: `try_acquire` twice gives 1 then 0; `add(8, 5)` returns the old values 0 then 5; `wait(16, 0, 1ms)` returns 2 (timeout) and `wait(16, 1, 0)` returns 1 (value differs). The multi-thread test creates a shared `WebAssembly.Memory`, starts four Workers that each instantiate the module over it, and checks the result: 4 x 20,000 atomic adds give exactly 80,000, and 4 x 5,000 increments of a plain counter guarded by `try_acquire`/`release` give exactly 20,000; one worker parked in `wait` returns 0 once the main thread's `notify` wakes it. On wasmtime the same instructions ran from a self-contained variant of the file under `-W shared-memory`; without it, instantiation fails with "shared memory support is disabled for this engine".

## GC: structs, arrays, `i31`

WasmGC (finished in 3.0, with typed function references) adds engine-managed heap objects, so languages with a GC (Java, Kotlin, Dart, OCaml) do not ship their own collector in linear memory. Types are declared in the type section: `struct` with typed, optionally mutable fields, `array`, and function types, inside **recursive groups** (`(rec ...)`) so types can refer to each other. `(sub $parent ...)` declares a subtype; `final` forbids further subtyping. References are `(ref null? ht)` over the heap-type lattice (`any > eq > i31, struct, array`; `func`; `extern`; `exn`; and the bottom types `none`, `nofunc`, `noextern`, `noexn`, from the spec's grammar). Instructions: `struct.new/get/set`, `array.new/get/set/len`, `ref.i31`/`i31.get_s/u` (a 31-bit integer stored inside the reference, no allocation), `ref.test` (a boolean type test), `ref.cast` (a checked downcast that traps on failure), `br_on_cast`.

<!-- listing file:corelib/wat/examples/gc.wat -->
```wat
;; WasmGC: managed structs and arrays, i31 scalars and subtyping, with no linear memory involved.
(module
  (rec
    (type $shape (sub (struct (field $sides i32))))
    (type $rect (sub $shape (struct (field $sides i32) (field $w i32) (field $h i32)))))
  (type $words (array (mut i32)))

  (func $rect (param $w i32) (param $h i32) (result (ref $rect))
    (struct.new $rect (i32.const 4) (local.get $w) (local.get $h)))

  ;; Upcast is free; the downcast is checked at run time and traps when the value is not a $rect.
  (func (export "area") (param $w i32) (param $h i32) (result i32)
    (local $s (ref $shape))
    (local.set $s (call $rect (local.get $w) (local.get $h)))
    (i32.mul
      (struct.get $rect $w (ref.cast (ref $rect) (local.get $s)))
      (struct.get $rect $h (ref.cast (ref $rect) (local.get $s)))))

  (func (export "sides_of_plain") (result i32)
    (struct.get $shape $sides (struct.new $shape (i32.const 3))))

  (func (export "bad_cast") (result i32)
    (struct.get $rect $w (ref.cast (ref $rect) (struct.new $shape (i32.const 3)))))

  ;; An array of i32 allocated and indexed with bounds checks (out of range traps).
  (func (export "array_sum") (param $n i32) (result i32)
    (local $a (ref $words))
    (local $i i32)
    (local $acc i32)
    (local.set $a (array.new $words (i32.const 7) (local.get $n)))
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (local.set $acc (i32.add (local.get $acc) (array.get $words (local.get $a) (local.get $i))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (local.get $acc))

  ;; i31ref: a 31-bit integer stored in the reference itself, no allocation.
  (func (export "i31_roundtrip") (param $x i32) (result i32)
    (i31.get_s (ref.i31 (local.get $x))))

  (func (export "is_i31") (param $x i32) (result i32)
    (ref.test (ref i31) (ref.i31 (local.get $x)))))
```
<!-- /listing -->

Observed on Bun and wasmtime: `area(3, 4) = 12` after an upcast to `$shape` and a checked downcast back; `array_sum(5) = 35`; `i31_roundtrip(-5) = -5`; `i31_roundtrip(0x7fffffff) = -1` because only 31 bits survive; `bad_cast` traps (`ref.cast failed to cast reference to target heap type` on Bun, `wasm trap: cast failure` on wasmtime). Relation to the corelib: the corelib needs no GC; its frame bitmap and capability table are fixed arrays in linear memory by design. GC matters for the *apps*: a managed-language app can target GC types instead of bringing a heap into its 64 KiB pages. C compiles to linear memory: the corelib module has no GC types. `clang -mgc` is accepted and only adds `+gc` (and `+reference-types`) to the `target_features` section (observed); GC types come from hand-written WAT or from a managed-language compiler.

## Exception handling

The final design (finished in 3.0) has **tags**, `throw`, `throw_ref` and a structured `try_table` whose clauses are `catch $tag $label`, `catch_ref $tag $label`, `catch_all $label` and `catch_all_ref $label` (spec section 6, text format). A tag declares the payload types (`(tag $overflow (param i32))`). `throw $tag` unwinds the stack to the innermost `try_table` with a matching clause, and branches to its label with the payload on the stack (and an `exnref` for the `_ref` forms, which `throw_ref` can raise again). The `exnref` value type and `try_table` are the reason this is a different feature from the older proposal: the legacy form used `try`, `catch`, `delegate` and `rethrow` and is listed as "Legacy Exception Handling" (phase "inactive") in `features.json`; engines list it separately from "Exception Handling with exnref". The example uses only the final form; `wasm-tools validate --features all` accepts it.

<!-- listing file:corelib/wat/examples/exceptions.wat -->
```wat
;; Exception handling, final form: a tag declares the payload, throw raises, try_table catches.
;; This is the form standardised in Wasm 3.0; the older `try`/`catch`/`delegate` encoding is a different feature.
(module
  (tag $overflow (param i32))

  (func $checked_add (param $a i32) (param $b i32) (result i32)
    (local $sum i64)
    (local.set $sum (i64.add (i64.extend_i32_s (local.get $a)) (i64.extend_i32_s (local.get $b))))
    (if (i64.ne (local.get $sum) (i64.extend_i32_s (i32.wrap_i64 (local.get $sum))))
      (then (throw $overflow (local.get $a))))
    (i32.wrap_i64 (local.get $sum)))

  ;; Returns the sum, or -1 when the add overflowed; the catch branch receives the tag's payload.
  (func (export "add_or_minus_one") (param $a i32) (param $b i32) (result i32)
    (block $caught (result i32)
      (try_table (result i32) (catch $overflow $caught)
        (call $checked_add (local.get $a) (local.get $b)))
      (return))
    (drop)
    (i32.const -1))

  ;; An exception nobody catches leaves the instance as a WebAssembly.Exception.
  (func (export "uncaught") (throw $overflow (i32.const 99))))
```
<!-- /listing -->

Observed: `add_or_minus_one(1, 2) = 3`; the overflowing add is caught and returns -1 on both engines; the uncaught throw leaves Bun as a `WebAssembly.Exception`. **Traps are not exceptions**: `traps.wat` shows that `try_table (catch_all ...)` around `unreachable` still traps (chapter 5), so no Wasm code can swallow an out-of-bounds access. The corelib itself needs none: its syscalls return negative codes (`EPERM`, `EBADCAP`, ...) and its module contains no `unreachable`.

From C, observed with clang 23.1.5 on `void boom(void *p) { __builtin_wasm_throw(0, p); }`: without `-mexception-handling` the compile fails with `'__builtin_wasm_throw' needs target feature exception-handling`; with it the assembly is `throw __cpp_exception`, and the link fails with `wasm-ld: error: ... undefined symbol: __cpp_exception` because the tag is normally defined by the C++ unwinding runtime. A three-line assembly file (`.tagtype __cpp_exception i32`, `.globl __cpp_exception`, `__cpp_exception:`) defines it, and the result is a valid module (`wasm-tools validate --features all`) with `(tag (param i32))` and `throw 0`, `target_features` `+mutable-globals +sign-ext +exception-handling +reference-types +multivalue +call-indirect-overlong`. C has no `catch`, so no `try_table` came out of clang here; `-mllvm -wasm-use-legacy-eh=false` stopped the backend with `-exception-model=wasm only allowed with at least one of -wasm-enable-eh or -wasm-enable-sjlj`. Catching needs C++ with `-fwasm-exceptions` and its runtime, which this freestanding build does not link; that was not run.

## Tail calls

`return_call`, `return_call_indirect` and `return_call_ref` (finished in 3.0) are calls that reuse the caller's frame: the stack does not grow, so a loop written as recursion, or a state machine that calls its next state, runs in constant stack. The result type of the callee must match the caller's. Without them a call-heavy interpreter loop or functional-language code overflows the stack in engines that limit it. Observed on Bun and wasmtime: `sum_to(10_000_000)` recurses ten million times and returns 50000005000000; a mutual recursion `is_even`/`is_odd` through `return_call_indirect` over a table works the same way.

<!-- listing file:corelib/wat/examples/tail_call.wat -->
```wat
;; Tail calls: return_call reuses the caller's frame, so a loop written as recursion runs in constant stack.
(module
  (func $go (param $n i32) (param $acc i64) (result i64)
    (if (result i64) (i32.eqz (local.get $n))
      (then (local.get $acc))
      (else (return_call $go (i32.sub (local.get $n) (i32.const 1)) (i64.add (local.get $acc) (i64.extend_i32_u (local.get $n)))))))

  (func (export "sum_to") (param $n i32) (result i64)
    (return_call $go (local.get $n) (i64.const 0)))

  ;; return_call_indirect: the dispatch loop of a cooperative scheduler needs no growing stack either.
  (type $step (func (param i32) (result i32)))
  (table $steps funcref (elem $even $odd))
  (func $even (type $step) (if (result i32) (i32.eqz (local.get 0)) (then (i32.const 1)) (else (return_call_indirect $steps (type $step) (i32.sub (local.get 0) (i32.const 1)) (i32.const 1)))))
  (func $odd (type $step) (if (result i32) (i32.eqz (local.get 0)) (then (i32.const 0)) (else (return_call_indirect $steps (type $step) (i32.sub (local.get 0) (i32.const 1)) (i32.const 0)))))
  (func (export "is_even") (param i32) (result i32) (return_call $even (local.get 0))))
```
<!-- /listing -->

The corelib could use this for its scheduler dispatch, but a different property decides its design: every app's `step()` returns to the scheduler, so there is no deep call chain to flatten. From C, observed with clang 23.1.5: with `-mtail-call` a call in tail position becomes `return_call` (two mutually recursive functions compiled at `-O0` gave `return_call $even` and `return_call $odd`; without the flag the same file gave `call $even` and `call $odd`), and the module records `+tail-call`. A self-recursive tail call needs no flag at `-O2`: LLVM turns it into a loop and the module contains no call at all. `__attribute__((musttail))` and `[[clang::musttail]]` on `return` statements printed `warning: unknown attribute ... ignored` on this clang in C23 mode, so the guarantee of a tail call was not available from the attribute. The corelib does not enable `-mtail-call`.

## Multiple memories

A module may declare several memories (finished in 3.0). Every load, store and bulk instruction names its memory index (default 0, so old code is unchanged), each memory has its own size, limits and bounds, `memory.copy $dst $src` moves bytes **between** memories, and `memory.grow $m` grows only one. The binary format marks a non-zero memory index in the memarg flags.

The corelib maps onto this directly. Today the corelib and each app are separate instances with separate memories, so no syscall can carry a pointer and `LOG` packs four bytes into one argument (chapter 5). With multiple memories, one module could hold a *kernel* memory and an *app* memory and copy across the boundary under its own bounds check:

<!-- listing file:corelib/wat/examples/multi_memory.wat -->
```wat
;; Multiple memories: memory 0 is the "kernel" (corelib) memory, memory 1 an app's private memory.
;; Every load, store and bulk operation names its memory; memory.copy moves bytes across the boundary.
(module
  (memory $kernel (export "kernel") 1)
  (memory $app (export "app") 1)
  (data $greeting (memory $app) (i32.const 0) "hello from ay app\n")

  ;; The kernel copies a message out of the app's memory into its own, after a bounds check of its choosing.
  (func (export "copy_in") (param $src i32) (param $dst i32) (param $len i32)
    (memory.copy $kernel $app (local.get $dst) (local.get $src) (local.get $len)))

  (func (export "kernel_byte") (param $addr i32) (result i32)
    (i32.load8_u $kernel (local.get $addr)))

  (func (export "poke_app") (param $addr i32) (param $v i32)
    (i32.store8 $app (local.get $addr) (local.get $v)))

  (func (export "grow_app") (param $pages i32) (result i32)
    (memory.grow $app (local.get $pages)))

  (func (export "sizes") (result i32 i32)
    (memory.size $kernel) (memory.size $app)))
```
<!-- /listing -->

Observed on Bun and wasmtime: `sizes()` returns `[1, 1]`; `copy_in(0, 100, 18)` copies "hello from ay app\n" from the app memory into the kernel memory at byte 100; `copy_in(65535, 0, 4)` traps because the *source* range leaves the 1-page app memory even though the destination is fine; `grow_app(1)` returns 1 and `sizes()` becomes `[1, 2]`: the kernel memory is untouched. `clang -mmultimemory` is accepted and only adds `+multimemory` to `target_features`; the linked module still has one memory (observed), and C has no syntax to name a second one, so a second memory comes from hand-written WAT.

## Memory64

A memory declared `(memory i64 1 4)` has an `i64` **address type**: addresses, sizes, `memory.size`, `memory.grow` and bulk instructions use `i64`; data values are unchanged. It lifts the 4 GiB limit of 32-bit addressing (`i32` memories cap at 65,536 pages of 64 KiB) at the price of wider pointers (the generated function below takes two `i64`s where a 32-bit target takes two `i32`s). Finished in 3.0.

<!-- listing file:corelib/wat/examples/memory64.wat -->
```wat
;; Memory64: the memory index type is i64, so addresses, sizes and memory.grow take and return i64.
;; Values (i32/i64 data) are unchanged; only address operands widen.
(module
  (memory (export "memory") i64 1 4)
  (data (i64.const 0) "hello from ay app\n")

  (func (export "load8") (param $addr i64) (result i32)
    (i32.load8_u (local.get $addr)))

  (func (export "store32") (param $addr i64) (param $v i32)
    (i32.store (local.get $addr) (local.get $v)))

  (func (export "size") (result i64) (memory.size))
  (func (export "grow") (param $pages i64) (result i64) (memory.grow (local.get $pages)))

  ;; An address above 4 GiB is expressible but out of bounds for a 1-page memory: the load traps.
  (func (export "far") (result i32) (i32.load8_u (i64.const 0x1_0000_0000))))
```
<!-- /listing -->

Observed on wasmtime 50.0.2 (default flags): `size` = 1; `load8(1)` = 101 (the `e` of "hello"); `far` (an `i64` address of 4 GiB in a 1-page memory) traps with `out of bounds memory access`. On Bun 1.4.5-canary the module is rejected at compile time: `CompileError: WebAssembly.Module doesn't parse at byte 46: Memory64 is not enabled`. From JavaScript an `i64` address is a `BigInt` (`load8(1n)`).

**C**: clang builds the whole corelib for `--target=wasm64` with no extra flag (observed, clang 23.1.5): `clang --target=wasm64 -mcpu=mvp -std=c23 -O2 -ffreestanding -nostdlib -Wl,--no-entry -Wl,--export-dynamic -o c64.wasm corelib/src/{abi,apps,cap,demo,engine,ipc,libc,mem,out,sync,wasm}.c` compiled and linked silently (the flags and sources are those of `buildWasm`, with `-Wl,-z,stack-size` and `--initial-memory` left out). `wasm-ld` picked the 64-bit mode from the object files. The result, from `wasm-tools print`:

```text
  (import "ay_host" "console_put" (func (;0;) (type 3) (param i32)))
  (table (;0;) i64 2 2 funcref)
  (memory (;0;) i64 2)
  (global (;0;) (mut i64) i64.const 65536)
  ...
  (func (;10;) (type 4) (param i64 i64) (result i32))   ;; ay_sum(const uint32_t *words, size_t len)
```

`size_t` and pointers are 64 bits, so `ay_sum` takes two `i64`, the table and the memory have the `i64` index type, and the stack pointer global is an `i64`; the `console_put` import keeps its `i32` byte parameter. The `target_features` section is `+mutable-globals +sign-ext +memory64`. `wasm-tools validate` (default features) accepts the module. wasmtime 50.0.2 on `fr1` ran it once the `ay_host` import was supplied with `--preload ay_host=stub.wat` (a one-function module): `ay_abi_version` returned 1, `ay_first_free(7)` returned 3, `ay_rights_allow(5, 4)` returned 1. Bun rejects the module: `CompileError: WebAssembly.Module doesn't parse at byte 152: Memory64 is not enabled`. The `wasm64` module is not part of the guide's generated listings: `tools/listings.ts` builds the 32-bit variants only.

## Custom page sizes

The page size of a memory is 64 KiB in the core spec. The custom-page-sizes proposal (phase 3) lets a memory type declare its page size; "there are currently exactly two valid page sizes: 1 byte and 64 KiB" ([custom-page-sizes Overview.md](https://github.com/WebAssembly/custom-page-sizes/blob/main/proposals/custom-page-sizes/Overview.md), read 2026-11-17). A memory of 1-byte pages has an exact size, which matters for embedded targets where a 64 KiB minimum is the whole RAM, and for Wasm-in-a-kernel designs like the corelib, whose 2 pages (128 KiB) are mostly one stack page and one static page (chapter 5).

<!-- listing file:corelib/wat/examples/custom_page_sizes.wat -->
```wat
;; Custom page sizes: a memory may declare a page of 1 byte instead of 64 KiB, so its size is exact.
;; This is the form for tiny embedded targets: a 1 KiB scratch memory without wasting a 64 KiB page.
(module
  (memory (export "memory") 1024 (pagesize 1))
  (data (i32.const 0) "ay")

  (func (export "size") (result i32) (memory.size))
  (func (export "byte") (param $addr i32) (result i32) (i32.load8_u (local.get $addr))))
```
<!-- /listing -->

Observed on wasmtime 50.0.2 with `-W custom-page-sizes`: `size` = 1024 (bytes) and `byte(0)` = 97 (`a` of "ay"). Without the flag wasmtime refuses the module at parse time: `the custom page sizes proposal must be enabled to customize a memory's page size (at offset 0x1c)`. Bun rejects it: `resizable limits flag are not valid 08` (the limits flag byte `0x08` carries the page-size bit). `wasm-tools validate --features all` accepts it.

## JS Promise Integration and stack switching

Wasm cannot suspend. Two proposals attack that from two sides.

**JSPI** (phase 5) is a JavaScript API, not new instructions: the embedder wraps an async JavaScript import in `new WebAssembly.Suspending(fn)` and an export in `WebAssembly.promising(fn)`. A call from Wasm into a `Suspending` import then *suspends the whole Wasm stack* until the promise settles, and the promising export returns a promise. The Wasm code is ordinary synchronous code, which lets a C program that blocks on I/O run unchanged on the web. The module contains no mention of promises:

<!-- listing file:corelib/wat/examples/jspi.wat -->
```wat
;; JS Promise Integration: plain synchronous Wasm that calls an import which, on the JS side, is async.
;; The module contains no mention of promises; the embedder wraps the import in WebAssembly.Suspending
;; and the export in WebAssembly.promising, so the call suspends the whole Wasm stack until the promise settles.
(module
  (import "ay" "fetch_word" (func $fetch_word (param i32) (result i32)))

  ;; Sum of fetch_word(0..n): reads like a blocking loop.
  (func (export "total") (param $n i32) (result i32)
    (local $i i32)
    (local $acc i32)
    (block $done
      (loop $next
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (local.set $acc (i32.add (local.get $acc) (call $fetch_word (local.get $i))))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    (local.get $acc)))
```
<!-- /listing -->

Observed on Bun: `promising(total)(4)` returns a `Promise` that resolves to 60 (`fetch_word(i) = 10*i`, awaited with `Bun.sleep(1)` each time); calling the same export without `promising` throws `SuspendError: Suspending function called outside of a promising() context`.

**Stack switching** (phase 3) adds typed continuations to the instruction set itself: `cont.new` makes a continuation from a function, `resume` runs it, `suspend` returns control to the nearest handler for a tag, and the handler gets the payload and a continuation to resume later. Coroutines, generators, green threads and effect handlers are all this one mechanism. It is exactly what the corelib's `step()` convention does by hand (chapter 5): the state in `hello.wat`'s `$state` global *is* the continuation. With the proposal, an app could keep its stack across a yield:

<!-- listing file:corelib/wat/examples/stack_switching.wat -->
```wat
;; Stack switching (typed continuations): a task suspends with `suspend` and its caller resumes it later.
;; A cooperative `step()` state machine, as in the corelib's apps (corelib/src/apps.c), is this written by hand; with the
;; proposal the app would keep its own stack across a yield instead.
(module
  (type $step (func (result i32)))
  (type $task (cont $step))
  (tag $yield (param i32))

  (func $body (type $step)
    (suspend $yield (i32.const 1))
    (suspend $yield (i32.const 2))
    (i32.const 7))
  (elem declare func $body)

  ;; Run the task to completion and return the number of times it yielded.
  (func (export "yields") (result i32)
    (local $k (ref null $task))
    (local $n i32)
    (local.set $k (cont.new $task (ref.func $body)))
    (loop $again
      (block $on_yield (result i32 (ref $task))
        (drop (resume $task (on $yield $on_yield) (local.get $k)))
        (return (local.get $n)))
      (local.set $k)
      (drop)
      (local.set $n (i32.add (local.get $n) (i32.const 1)))
      (br $again))
    (unreachable)))
```
<!-- /listing -->

Observed: `wasm-tools validate --features all` accepts the module; wasmtime 50.0.2 on `fr1` (Linux x86-64) with `-W stack-switching,function-references,exceptions,gc` returns 2 (the task suspended twice); Bun rejects it; wasmtime on Windows reports that stack switching is not supported on that compiler configuration. Until it is standard everywhere, the cooperative `step()` state machine, which runs on any engine, remains the corelib's design.

## Feature-checking a module

`wasm-tools validate --features all file.wasm` checks against every proposal wasmparser knows; `wasm-tools validate file.wasm` checks against the default set, which its help describes as "all WebAssembly proposals that are at phase 4 or after"; `--features wasm1`/`mvp`, `wasm2`, `wasm3` select a spec version. Run on `stack_switching.wat` the default set says `cannot define continuation types when stack switching is disabled`. In JavaScript, test an engine by compiling a minimal module (`WebAssembly.validate(bytes)`), the way the `wasm-feature-detect` library does.

## Sources

- <https://www.w3.org/TR/wasm-core-2/>, <https://api.w3.org/specifications/wasm-core-2/versions/latest>, <https://api.w3.org/specifications/wasm-core-1/versions/latest>, <https://webassembly.org/specs/> (all read 2026-11-17).
- <https://github.com/WebAssembly/proposals/blob/main/finished-proposals.md>, <https://github.com/WebAssembly/website/blob/main/features.json>, <https://webassembly.org/features/> (2026-11-17).
- Proposal overviews: [relaxed-simd](https://github.com/WebAssembly/relaxed-simd/blob/main/proposals/relaxed-simd/Overview.md), [custom-page-sizes](https://github.com/WebAssembly/custom-page-sizes/blob/main/proposals/custom-page-sizes/Overview.md).
- Toolchain: `docs/adr/0008-sdk-languages-toolchain.md` (LLVM first: clang, wasm-ld); `tools/corelib.ts` (the flags); clang 23.1.5 and LLD 23.1.5 on Windows x64, run on 2026-11-17.
- Runs: Bun 1.4.5-canary.1+9098897a1 on Windows x64, wasmtime 50.0.2 on `fr1` (Linux x86-64) and on Windows, `wasm-tools 1.263.0`; tests `corelib/host/wat-examples.test.ts`.
