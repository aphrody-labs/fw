# 5. WebAssembly core: stack machine, modules, memory, tables, binary and text format

WebAssembly (Wasm) is a portable, typed, sandboxed instruction set. It is not tied to a browser: in this code base the same freestanding C23 corelib (`corelib/src`, built with clang and lld, ADR-0008) runs as a bare-metal x86-64 image and as `corelib.wasm`, and apps for it are hand-written Wasm modules. This chapter explains the core language through those modules. Every listing is generated from the code (`tools/listings.ts`); every behavior claimed below was run on Bun (JavaScriptCore), on wasmtime 50.0.2, or with `wasm-tools 1.263.0` on 2026-11-17, and the runnable ones are in `corelib/host/wat-examples.test.ts` and `corelib.test.ts`.

Specification used: [WebAssembly Core Specification](https://www.w3.org/TR/wasm-core-2/), whose page reads "release 3.0 ... version 3.0 (2026-11-16)" and carries the W3C status Candidate Recommendation Draft (read 2026-11-17). Chapter 6 gives the standards status in full.

## The files this chapter points at

| File | What it is |
|---|---|
| `corelib/wat/hello.wat` | The boot app, hand-written WAT. Imports one function, exports `step` and its memory. |
| `corelib/wat/syscall_shim.wat` | A hand-written shim: typed wrappers over the one import, plus a `funcref` table and `call_indirect`. |
| `corelib/src/*.c`, `tools/corelib.ts` | The corelib in C23, and the one place that knows the clang flags. `buildWasm()` runs `clang --target=wasm32 -mcpu=mvp -std=c23 -O2 -ffreestanding -nostdlib -Wl,--no-entry -Wl,-z,stack-size=65536 -Wl,--initial-memory=131072 -Wl,--export-dynamic`. The resulting `corelib.wasm` has (`wasm-tools objdump`/`print`, clang 23.1.5, 2026-11-17): 9 types, 1 import, 23 functions, 1 table of 2 entries, 1 memory of 2 pages, 1 mutable global, 14 exports, 3 data segments and only a `producers` custom section. |
| `corelib/host/host.ts` | The embedder: instantiates the modules and wires imports. |
| `corelib/wat/examples/*.wat` | Small examples written for this guide, each validated with `wasm-tools validate --features all` and run by the test. |

## A stack machine, checked by types

Wasm code is a sequence of instructions that push and pop values on an **operand stack**. There are no registers and no addresses in the instruction stream, so the corelib's rights check `have & need == need` is five instructions:

<!-- listing c:corelib/src/abi.c#ay_rights_allow -->
```c
/// Whether a capability holding `have` may perform an operation that needs `need`.
AY_API uint32_t ay_rights_allow(uint32_t have, uint32_t need) { return (have & need) == need; }
```
<!-- /listing -->

<!-- listing wat:ay_rights_allow -->
```wat
(func $ay_rights_allow (export "ay_rights_allow") (type 3) (param i32 i32) (result i32)
  local.get 0
  local.get 1
  i32.and
  local.get 1
  i32.eq
)
```
<!-- /listing -->

<!-- listing asm:ay_rights_allow:x86_64 -->
```asm
ay_rights_allow:
	push	rbp
	mov	rbp, rsp
	not	edi
	xor	eax, eax
	test	edi, esi
	sete	al
	pop	rbp
	ret
```
<!-- /listing -->

`local.get 0; local.get 1; i32.and` pushes `have`, pushes `need`, pops both and pushes `have & need`; `local.get 1; i32.eq` compares that with `need`. The x86-64 version (after clang's frame-pointer prologue) works the other way round: it has registers, so it complements `have` and tests it against `need` (`not edi; test edi, esi; sete al`). Wasm has no flags register and no `andn`; an engine's compiler decides what machine code the five instructions become.

The central rule is **validation**: before anything runs, an engine type-checks the module by abstractly executing every function with a stack of types instead of values. Each instruction has a type `[inputs] -> [outputs]`; it is valid only if the inputs are on the stack, and at the end of a function the stack must be exactly the declared results. `i32.and` is `[i32 i32] -> [i32]`, so `i32.and` on an `i64` operand is rejected before the module can be instantiated. The spec defines validity per instruction as typing rules (section 3, "Validation"), and an invalid module is never run: there is no partially valid state to defend against at run time. After `unreachable`, `br`, `return` and `throw` the stack becomes *polymorphic* (any types may be assumed), which is why dead code after them still validates.

Why a stack machine and not registers? The design rationale gives two reasons: the stack machine encodes smaller than registers or SSA, and **structured control flow** makes verification simple, "including decoding directly to a compiler's internal SSA form" ([WebAssembly/design Rationale.md](https://github.com/WebAssembly/design/blob/main/Rationale.md), read 2026-11-17). The same page says the restriction to structured control and structured stack use "greatly simplifies one-pass verification, avoiding a fixpoint computation like that of other stack machines such as the JVM (prior to stack maps)".

## Value types, function types, multi-value

| Type | Bits | Notes |
|---|---|---|
| `i32`, `i64` | 32, 64 | No signedness in the type: `i32.lt_s` and `i32.lt_u` are different instructions; two's complement wraps. |
| `f32`, `f64` | 32, 64 | IEEE 754-2019 arithmetic; NaN payloads are not fully specified (see determinism). |
| `v128` | 128 | One SIMD register: 16 x i8, 8 x i16, 4 x i32, 2 x i64, 4 x f32 or 2 x f64 lanes (chapter 6). |
| `funcref`, `externref` | opaque | Reference types: `funcref` is a function (null or callable), `externref` a host object. Cannot be stored in linear memory. |
| `(ref null? ht)` | opaque | Typed references, the base of GC (chapter 6). |

Memory-only formats `i8` and `i16` exist as storage types in loads, stores and GC fields, not as stack types. A **function type** is `[params] -> [results]`; since multi-value (Wasm 2.0) a function and a block may return several values. The type section of `hello.wat` below has three types, and `corelib/wat/examples/multi_memory.wat` exports `sizes` with the type `[] -> [i32 i32]`, two results with no memory or struct involved. clang enables multi-value in its default CPU (`generic`, `+multivalue` in the `target_features` section) and not with `-mcpu=mvp`, which the corelib uses (chapter 6).

## Module structure and sections

A module is a binary with a header and sections. The spec lists these ids (section 5.5, read 2026-11-17): 0 custom, 1 type, 2 import, 3 function, 4 table, 5 memory, 6 global, 7 export, 8 start, 9 element, 10 code, 11 data, 12 data count, 13 tag. "Section ids do not always correspond to the order of sections": the required order in the module is type, import, function, table, memory, **tag**, global, export, start, element, **data count**, code, data, with custom sections allowed between any two. Every section is optional.

| Section | Holds | In `hello.wat` / `syscall_shim.wat` |
|---|---|---|
| type | the function types, referenced by index | 3 types in hello; 4 in the shim |
| import | `(module, name, type)` triples: functions, tables, memories, globals, tags | `ay.syscall`, `(i32 i32 i32 i32) -> i32` |
| function | one type index per defined function; the bodies come later | 2 functions in hello |
| table | arrays of references | the shim's `funcref` table of 3 slots |
| memory | linear memories, with limits in pages | 1 page, exported as `memory` |
| tag | exception tags (exception handling) | none; see `examples/exceptions.wat` |
| global | typed cells, mutable or not, with a constant initializer | `$state`, a mutable i32 |
| export | names for functions, tables, memories, globals, tags | `step` and `memory`; the shim's five wrappers |
| start | a function to run at instantiation | none |
| element | initial contents of tables | the shim's three function references |
| data count | number of data segments (lets `memory.init` validate in one pass) | emitted by assemblers when `memory.init` is used |
| code | the function bodies: locals and instructions | the `$print` and `step` bodies |
| data | initial contents of memories | three strings at offsets 0, 32 and 48 |
| custom | opaque, ignored by semantics: `name` (debug names), `producers`, `target_features`, DWARF | `name` |

The compiled corelib adds a `producers` section and a `target_features` section; `wasm-tools objdump` shows both. The `target_features` section is a toolchain convention (it records what the object was built with), not part of the spec.

## The binary format

<!-- listing dump:corelib/wat/hello.wat:68 -->
```text
   0x0 | 00 61 73 6d | version 1 (Module)
       | 01 00 00 00
   0x8 | 01 12       | type section
   0xa | 03          | 3 count
--- rec group 0 (implicit) ---
   0xb | 60 04 7f 7f | [type 0] (func (param i32 i32 i32 i32) (result i32))
       | 7f 7f 01 7f
--- rec group 1 (implicit) ---
  0x13 | 60 02 7f 7f | [type 1] (func (param i32 i32))
       | 00         
--- rec group 2 (implicit) ---
  0x18 | 60 00 01 7f | [type 2] (func (result i32))
  0x1c | 02 0e       | import section
  0x1e | 01          | 1 count
  0x1f | 02 61 79 07 | import [func 0] Import { module: "ay", name: "syscall", ty: Func(0) }
       | 73 79 73 63
       | 61 6c 6c 00
       | 00         
  0x2c | 03 03       | func section
  0x2e | 02          | 2 count
  0x2f | 01          | [func 1] type 1
  0x30 | 02          | [func 2] type 2
  0x31 | 05 03       | memory section
  0x33 | 01          | 1 count
  0x34 | 00 01       | [memory 0] MemoryType { memory64: false, shared: false, initial: 1, maximum: None, page_size_log2: None }
  0x36 | 06 06       | global section
  0x38 | 01          | 1 count
  0x39 | 7f 01       | [global 0] GlobalType { content_type: I32, mutable: true, shared: false }
  0x3b | 41 00       | i32_const value:0
  0x3d | 0b          | end
  0x3e | 07 11       | export section
  0x40 | 02          | 2 count
  0x41 | 06 6d 65 6d | export Export { name: "memory", kind: Memory, index: 0 }
       | 6f 72 79 02
       | 00         
  0x4a | 04 73 74 65 | export Export { name: "step", kind: Func, index: 2 }
       | 70 00 02   
  0x51 | 0a db 01    | code section
  0x54 | 02          | 2 count
============== func 1 ====================
  0x55 | 39          | size of function
  0x56 | 01          | 1 local blocks
  0x57 | 01 7f       | 1 locals of type I32
  0x59 | 02 40       | block blockty:Empty
  0x5b | 03 40       | loop blockty:Empty
  0x5d | 20 01       | local_get local_index:1
  0x5f | 45          | i32_eqz
  0x60 | 0d 01       | br_if relative_depth:1
  0x62 | 20 01       | local_get local_index:1
  0x64 | 41 04       | i32_const value:4
  0x66 | 20 01       | local_get local_index:1
  0x68 | 41 04       | i32_const value:4
  0x6a | 49          | i32_lt_u
  0x6b | 1b          | select
  0x6c | 21 02       | local_set local_index:2
  0x6e | 41 02       | i32_const value:2
  0x70 | 41 00       | i32_const value:0
  0x72 | 20 00       | local_get local_index:0
  0x74 | 28 00 00    | i32_load memarg:MemArg { align: 0, max_align: 2, offset: 0, target: Memory(0), ordering: None }
  0x77 | 20 02       | local_get local_index:2
  0x79 | 10 00       | call function_index:0
  0x7b | 1a          | drop
  0x7c | 20 00       | local_get local_index:0
  0x7e | 41 04       | i32_const value:4
  0x80 | 6a          | i32_add
  0x81 | 21 00       | local_set local_index:0
  0x83 | 20 01       | local_get local_index:1
  0x85 | 20 02       | local_get local_index:2
```
<!-- /listing -->

This is `wasm-tools dump` of the assembled `hello.wat`: the left column is the byte offset, then the bytes, then the decoded meaning. How to read it:

- `00 61 73 6d 01 00 00 00`: the magic `\0asm` and version 1. Every Wasm module, whatever features it uses, starts so.
- A section is `id`, a **size** and the contents. `01 12` is the type section, 18 bytes long; `0a db 01` further down is the code section with the size `db 01`.
- Sizes and counts are **LEB128**: little-endian groups of 7 bits, the high bit of each byte meaning "more follows". `db 01` is `0x5b + (1 << 7) = 219`, which is the 219 bytes `wasm-tools objdump` reports for the code section. An `u32` takes at most 5 bytes; small numbers cost one byte, which is why the format is compact. Immediates such as `i32.const` use the signed variant (`41 7f` is `i32.const -1`).
- Types start with `60` (function), then the parameter vector (`04 7f 7f 7f 7f`: four `i32`; `7f` is the i32 type byte, `7e` i64, `7d` f32, `7c` f64, `7b` v128, `70` funcref, `6f` externref), then the result vector.
- An instruction is an opcode byte plus immediates: `20 01` is `local.get 1`, `41 04` is `i32.const 4`, `0d 01` is `br_if 1`, `28 00 00` is `i32.load` with alignment exponent 0 (that is `align=1`) and offset 0. Prefix bytes extend the opcode space: `0xFC` for bulk memory and saturating conversions, `0xFD` SIMD, `0xFE` atomics, `0xFB` GC.
- Function bodies declare locals as run-length groups (`01 01 7f`: one group of one `i32`); the parameters come first in the local index space.

Everything is index-based: functions, types, tables, memories, globals are referenced by position in their section (imports first). Names exist only in the `name` custom section, which a stripped module can omit.

The shim shows the table and element sections that `hello.wat` does not have:

<!-- listing dump:corelib/wat/syscall_shim.wat:57 -->
```text
   0x0 | 00 61 73 6d | version 1 (Module)
       | 01 00 00 00
   0x8 | 01 19       | type section
   0xa | 04          | 4 count
--- rec group 0 (implicit) ---
   0xb | 60 00 01 7f | [type 0] (func (result i32))
--- rec group 1 (implicit) ---
   0xf | 60 04 7f 7f | [type 1] (func (param i32 i32 i32 i32) (result i32))
       | 7f 7f 01 7f
--- rec group 2 (implicit) ---
  0x17 | 60 03 7f 7f | [type 2] (func (param i32 i32 i32) (result i32))
       | 7f 01 7f   
--- rec group 3 (implicit) ---
  0x1e | 60 01 7f 01 | [type 3] (func (param i32) (result i32))
       | 7f         
  0x23 | 02 0e       | import section
  0x25 | 01          | 1 count
  0x26 | 02 61 79 07 | import [func 0] Import { module: "ay", name: "syscall", ty: Func(1) }
       | 73 79 73 63
       | 61 6c 6c 00
       | 01         
  0x33 | 03 06       | func section
  0x35 | 05          | 5 count
  0x36 | 00          | [func 1] type 0
  0x37 | 00          | [func 2] type 0
  0x38 | 00          | [func 3] type 0
  0x39 | 02          | [func 4] type 2
  0x3a | 03          | [func 5] type 3
  0x3b | 04 05       | table section
  0x3d | 01          | 1 count
  0x3e | 70 01 03 03 | [table 0] Table { ty: TableType { element_type: funcref, table64: false, initial: 3, maximum: Some(3), shared: false }, init: RefNull }
  0x42 | 07 30       | export section
  0x44 | 05          | 5 count
  0x45 | 07 76 65 72 | export Export { name: "version", kind: Func, index: 1 }
       | 73 69 6f 6e
       | 00 01      
  0x4f | 05 79 69 65 | export Export { name: "yield", kind: Func, index: 2 }
       | 6c 64 00 02
  0x57 | 0b 66 72 61 | export Export { name: "frame_alloc", kind: Func, index: 3 }
       | 6d 65 5f 61
       | 6c 6c 6f 63
       | 00 03      
  0x65 | 03 6c 6f 67 | export Export { name: "log", kind: Func, index: 4 }
       | 00 04      
  0x6b | 06 69 6e 76 | export Export { name: "invoke", kind: Func, index: 5 }
       | 6f 6b 65 00
       | 05         
  0x74 | 09 0b       | element section
  0x76 | 01          | 1 count
  0x77 | 02 00       | element table[Some(0)]
  0x79 | 41 00       | i32_const value:0
  0x7b | 0b          | end
  0x7c | 00 03       | 3 items [indices]
  0x7e | 01          | item 1
  0x7f | 02          | item 2
  0x80 | 03          | item 3
  0x81 | 0a 3d       | code section
```
<!-- /listing -->

The table section `70 01 03 03` is `funcref`, flags `01` (has a maximum), minimum 3, maximum 3. The element section fills table 0 from offset 0 with functions 1, 2 and 3 (index 0 is the import).

## The text format (WAT)

WAT is the human form of the same module, an S-expression language. It is not a second language: `wasm-tools parse` turns it into the binary above, and `wasm-tools print` turns the binary back; comments and layout are lost, and names survive only through the `name` section.

- **Flat form** lists instructions one per line in execution order: `local.get 1`, `local.get 0`, `i32.and`. This is what `wasm-tools print` emits and what the generated listings show.
- **Folded form** nests operands inside their consumer: `(i32.and (local.get 1) (local.get 0))`. It is only syntax; the parser flattens it to the same instructions. `hello.wat` is written folded, which reads like an expression tree.
- Identifiers start with `$` and resolve to indices at parse time (`$sys`, `$print`, `$state`); without them you write numbers (`call 0`). `wasm-tools print` emits `(;0;)` index comments, which `tools/listings.ts` strips because indices are volatile.
- Comments are `;;` to the end of the line and `(; ... ;)` for blocks. Whole `(module ...)` text may use inline exports (`(memory (export "memory") 1)`) and inline data/element segments as abbreviations of separate fields.
- Numbers may be hex or contain `_` (`0x1_0000_0000`, as in `memory64.wat`); `align=1`, `offset=48` are memory immediates.

## Instruction families

| Family | Examples | Seen in |
|---|---|---|
| Constants, locals, globals | `i32.const`, `local.get/set/tee`, `global.get/set` | everywhere; `$state` in `hello.wat` |
| Integer arithmetic, bitwise | `i32.add`, `i32.and`, `i32.shl`, `i32.rotl`, `i64.ctz`, `i32.eqz` | `ay_checksum`, `ay_first_free` |
| Comparisons | `i32.lt_u`, `i32.eq`, `i64.lt_u` | `ay_rights_allow`, `ay_first_free` |
| Float and conversions | `f32.add`, `i32.trunc_f32_s` (traps), `i32.trunc_sat_f32_s` (saturates), `i64.extend_i32_u`, `i32.wrap_i64` | `examples/traps.wat`, `tail_call.wat` |
| Parametric | `drop`, `select` | `select` is the branch-free `min` in `$print` |
| Memory | `i32.load`, `i32.load8_u`, `i32.store`, `memory.size/grow`, bulk `memory.copy/fill/init`, `data.drop` | `hello.wat`, `ay_arg_count` |
| Tables and references | `table.get/set/grow/fill/copy/init`, `ref.null`, `ref.func`, `ref.is_null` | the shim's table |
| Control | `block loop if else br br_if br_table return call call_indirect unreachable nop` | next section |
| Vector | `v128.load`, `i32x4.add`, `i8x16.shuffle`, lane extract and replace | chapter 6, `ay_sum` with `simd128` |
| Atomics | `i32.atomic.rmw.cmpxchg`, `memory.atomic.wait32/notify`, `atomic.fence` | chapter 6, `ay_try_acquire` |
| GC, exceptions, tail calls, continuations | `struct.new`, `ref.cast`, `throw`, `try_table`, `return_call`, `resume` | chapter 6 |

Numeric instructions are named `type.operation[_signedness]`; the signedness suffix exists exactly where the answer depends on it (`div_s`, `div_u`, `lt_s`, `shr_u`). Add, sub, mul, and, or, xor, shl, rotl need none: two's complement makes them the same bits.

## Structured control flow

There is **no goto**. Control is made of nested constructs whose branches target a *label*:

- `block` and `if`: a label that points to the **end** of the construct. `br` to it jumps forward, leaving the block.
- `loop`: a label that points to the **start**. `br` to it jumps backward, iterating.
- `br l` branches to the label `l` levels out (`0` innermost); `br_if` does it when the popped i32 is non-zero; `br_table` picks one of several labels by an index, with a default; `return` leaves the function.
- A branch carries the label's result values, and **drops** everything else the construct left on the stack: the stack height at every point is statically known.

`$print` in `hello.wat` is the canonical shape of a `while` loop: `block $done` wraps `loop $next`, `br_if $done` exits forward, `br $next` repeats. The same pattern is in the generated `ay_checksum`, where LLVM unrolled the C `for` loop twice (the x86-64 listing below is unrolled four times):

<!-- listing wat:ay_checksum -->
```wat
(func $ay_checksum (export "ay_checksum") (type 3) (param i32 i32) (result i32)
  (local i32 i32 i32 i32)
  local.get 1
  i32.eqz
  if ;; label = @1
    i32.const -2128831035
    return
  end
  block ;; label = @1
    block ;; label = @2
      local.get 1
      i32.const 1
      i32.eq
      if ;; label = @3
        i32.const -2128831035
        local.set 2
        br 1 (;@2;)
      end
      local.get 1
      i32.const 1
      i32.and
      local.set 4
      local.get 1
      i32.const -2
      i32.and
      local.set 5
      i32.const -2128831035
      local.set 2
      local.get 0
      local.set 1
      loop ;; label = @3
        local.get 1
        i32.const 4
        i32.add
        i32.load
        local.get 1
        i32.load
        local.get 2
        i32.const 5
        i32.rotl
        i32.add
        i32.const -1640531527
        i32.xor
        i32.const 5
        i32.rotl
        i32.add
        i32.const -1640531527
        i32.xor
        local.set 2
        local.get 1
        i32.const 8
        i32.add
        local.set 1
        local.get 5
        local.get 3
        i32.const 2
        i32.add
        local.tee 3
        i32.ne
        br_if 0 (;@3;)
      end
      local.get 4
      i32.eqz
      br_if 1 (;@1;)
    end
    local.get 0
    local.get 3
    i32.const 2
    i32.shl
    i32.add
    i32.load
    local.get 2
    i32.const 5
    i32.rotl
    i32.add
    i32.const -1640531527
    i32.xor
    local.set 2
  end
  local.get 2
)
```
<!-- /listing -->

and the x86-64 version of the same function, where the loop is a conditional `jne` backwards (a goto the hardware allows) and `rotl` is the `rol` instruction:

<!-- listing asm:ay_checksum:x86_64 -->
```asm
ay_checksum:
	push	rbp
	mov	rbp, rsp
	test	rsi, rsi
	je	.LBB0_1
	mov	ecx, esi
	and	ecx, 3
	cmp	rsi, 4
	jae	.LBB0_8
	mov	eax, -2128831035
	xor	edx, edx
	jmp	.LBB0_5
.LBB0_1:
	mov	eax, -2128831035
	pop	rbp
	ret
.LBB0_8:
	and	rsi, -4
	mov	eax, -2128831035
	xor	edx, edx
.LBB0_9:                                # =>This Inner Loop Header: Depth=1
	rol	eax, 5
	add	eax, dword ptr [rdi + 4*rdx]
	xor	eax, -1640531527
	rol	eax, 5
	add	eax, dword ptr [rdi + 4*rdx + 4]
	xor	eax, -1640531527
	rol	eax, 5
	add	eax, dword ptr [rdi + 4*rdx + 8]
	xor	eax, -1640531527
	rol	eax, 5
	add	eax, dword ptr [rdi + 4*rdx + 12]
	xor	eax, -1640531527
	add	rdx, 4
	cmp	rsi, rdx
	jne	.LBB0_9
	test	rcx, rcx
	je	.LBB0_7
.LBB0_5:
	lea	rdx, [rdi + 4*rdx]
	xor	esi, esi
.LBB0_6:                                # =>This Inner Loop Header: Depth=1
	rol	eax, 5
	add	eax, dword ptr [rdx + 4*rsi]
	xor	eax, -1640531527
	inc	rsi
	cmp	rcx, rsi
	jne	.LBB0_6
.LBB0_7:
	pop	rbp
	ret
```
<!-- /listing -->

Why this restriction? A forward-only-or-loop-head label structure lets an engine validate in one linear pass, build SSA directly, and know the operand stack at every branch target. The rationale page cites the Relooper result: "any control flow, even irreducible, can be transformed into structured control flow with guaranteed low code size overhead, and typically minimal throughput overhead" (same Rationale.md, read 2026-11-17). Compilers such as LLVM do that transformation, so C code is unaffected.

`ay_arg_count` is a `switch` in C (`corelib/src/abi.c`). LLVM turned it into a lookup table on both targets: a nine-byte table indexed by `nr - 2` and one range check. In Wasm the table is the first bytes of the data segment at address 65536; in x86-64 it is `.Lswitch.table.ay_arg_count`:

<!-- listing wat:ay_arg_count -->
```wat
(func $ay_arg_count (export "ay_arg_count") (type 4) (param i32) (result i32)
  (local i32)
  local.get 0
  i32.const 2
  i32.sub
  local.tee 0
  i32.const 8
  i32.le_u
  if (result i32) ;; label = @1
    local.get 0
    i32.load8_u offset=65536
  else
    local.get 1
  end
)
```
<!-- /listing -->

<!-- listing asm:ay_arg_count:x86_64 -->
```asm
ay_arg_count:
	push	rbp
	mov	rbp, rsp
	add	edi, -2
	xor	eax, eax
	cmp	edi, 8
	ja	.LBB2_2
	mov	eax, edi
	movzx	eax, byte ptr [rax + .Lswitch.table.ay_arg_count]
.LBB2_2:
	pop	rbp
	ret
.Lswitch.table.ay_arg_count:
	.ascii	"\003\002\001\001\001\002\001\000\001"
```
<!-- /listing -->

`i32.load8_u offset=65536` is the whole lookup: the table is in the module's data section (`\03\02\01\01\01\02\01\00\01`). The control-flow form a lookup table replaces is `br_table`, a jump table over nested blocks. Here it is by hand, with a plain `loop` for comparison; the test checks that it returns the same 13 answers as the corelib's table (`0,0,3,2,1,1,1,2,1,0,1,0,0` for `nr` 0 to 11 and 99):

<!-- listing file:corelib/wat/examples/control.wat -->
```wat
;; ay_arg_count written by hand with the control-flow shape LLVM did not pick: a br_table dispatch.
;; Nested blocks are the targets; br_table picks one by index (the last label is the default).
;; Same answer as ay_arg_count in corelib/src/abi.c: syscall numbers 0..10 map to 0..3 arguments.
(module
  (func (export "arg_count") (param $nr i32) (result i32)
    (block $three                       ;; LOG
      (block $two                       ;; MSG_SET, CAP_DERIVE
        (block $one                     ;; MSG_GET, SEND, RECV, CAP_DROP, FRAME_FREE
          (block $zero                  ;; VERSION, YIELD, FRAME_ALLOC and every unknown number
            (br_table $zero $zero $three $two $one $one $one $two $one $zero $one $zero
              (local.get $nr)))
          (return (i32.const 0)))
        (return (i32.const 1)))
      (return (i32.const 2)))
    (i32.const 3))

  ;; No goto: a loop is a `loop` block whose label is at its top; `br` to it jumps backwards.
  ;; countdown(n) returns how many times the body ran.
  (func (export "countdown") (param $n i32) (result i32)
    (local $steps i32)
    (block $exit
      (loop $top
        (br_if $exit (i32.eqz (local.get $n)))
        (local.set $n (i32.sub (local.get $n) (i32.const 1)))
        (local.set $steps (i32.add (local.get $steps) (i32.const 1)))
        (br $top)))
    (local.get $steps)))
```
<!-- /listing -->

`br_table` pops an index; labels are in the order of the blocks they close, innermost first; an index past the list takes the last label. A `br` into the outermost block leaves the `block $three`, whose result falls out as `i32.const 3`. A last example of structure: `ay_first_free` finds the lowest free frame. `i64.ctz` is count-trailing-zeros, `tzcnt` on x86-64 (the listing shows the encoding `rep bsf`, which CPUs without BMI execute as `bsf`), and `select` replaces a branch:

<!-- listing wat:ay_first_free -->
```wat
(func $ay_first_free (export "ay_first_free") (type 8) (param i64) (result i32)
  i32.const -1
  local.get 0
  i64.const -1
  i64.xor
  i64.ctz
  i32.wrap_i64
  local.get 0
  i64.const -1
  i64.eq
  select
)
```
<!-- /listing -->

<!-- listing asm:ay_first_free:x86_64 -->
```asm
ay_first_free:
	push	rbp
	mov	rbp, rsp
	mov	rax, rdi
	not	rax
	rep		bsf	rcx, rax
	xor	rdi, -1
	mov	eax, -1
	cmovne	eax, ecx
	pop	rbp
	ret
```
<!-- /listing -->

## Locals and globals

Parameters are the first locals; other locals are declared at the top of the function and start at zero. Locals are per call and live in the engine's registers or frame, invisible to others. **Globals** belong to the instance, have a type and a mutability, and may be imported or exported. `hello.wat` keeps its whole state in one mutable global, `$state` (0 print, 1 send, 2 receive), so that `step()` can resume where it left off after returning.

The compiled corelib has one global too: a mutable `i32` with the initial value 65536, which is the C stack pointer (wasm-ld names it `__stack_pointer`; it writes no `name` section, so the name is not in the module, only the index). Wasm has no addressable stack: locals and the operand stack are not in memory and cannot be pointed at. A language that takes addresses of locals needs a **shadow stack** in linear memory, and the toolchain implements its top as that global. `tools/corelib.ts` shrinks it to 64 KiB with `-Wl,-z,stack-size=65536` and fixes the memory at two pages with `-Wl,--initial-memory=131072`.

## Linear memory

A memory is a growable array of bytes, shared by nothing unless declared so. Its size is always a multiple of the **page size, 65536 bytes** (spec 4.2.9), declared in pages with a minimum and an optional maximum. `hello.wat` has `(memory 1)`: 64 KiB. The corelib has 2 pages: 1 for the stack (it grows down from 65536) and 1 holding the static data (the data segments start at 65536).

- **Access**: `i32.load`/`i32.store` take an `i32` address and an immediate `offset` and `align`. The effective address is `address + offset` computed **without wrapping at 32 bits**; the access traps when `effective + size > memory size`. Verified on Bun: with a 1-page memory, `i32.load` at 65532 returns 0, at 65533 traps although byte 65533 is in range (the 4-byte access is not), and `i32.load offset=0xffffffff` at address 1 traps rather than wrapping to 0.
- **Alignment** is a hint, written as an exponent in the binary: `align=1` means "may be unaligned", as `hello.wat` uses to read from byte offset 18 or 32. A misaligned access is legal and merely slower on some hardware. The atomic family is the exception: on Bun `i32.atomic.load` at address 1 traps with `Unaligned memory access`.
- **Little-endian**, always: byte 0 of an i32 is the least significant. That is why `hello.wat` packs four string bytes into one i32 and the LOG syscall emits `(a1 >> (8 * i)) as u8` for `i` in `0..len`.
- **Growth**: `memory.grow n` returns the old size in pages, or -1 when it cannot grow; the memory's base may move, so a host that holds an `ArrayBuffer` of the old memory must re-create the view (the test does so after `grow_app`). Memory can never shrink.
- **Initialization**: *active* data segments copy into memory at instantiation (the three strings of `hello.wat`); *passive* ones are copied later by `memory.init`; the bulk-memory instructions `memory.copy` and `memory.fill` give `memmove` and `memset` in one instruction each, replacing byte loops. clang's default CPU enables them; `-mcpu=mvp`, which the corelib uses, does not (chapter 6): the compiler then emits a call to `memmove` (observed: linking a freestanding `__builtin_memmove` fails with `undefined symbol: memmove`), so `corelib/src/libc.c` defines `memset`, `memcpy`, `memmove` and `memcmp` as plain byte loops.

No stack, heap or file lives outside this array, and an app cannot name any address outside its own memory: that is the isolation that capability design uses (below).

## Tables and `call_indirect`

Linear memory cannot hold function pointers: code is not data. A **table** is an array of references; `call_indirect` takes a table index, looks up the reference and calls it. The shim is the smallest instance of the pattern: a table `$nullary_calls` with `$version`, `$yield`, `$frame_alloc`, and `invoke(slot)` picks one at run time.

The type check happens **at the call**, because tables are untyped data to the validator: the module names the type it expects (`(type $nullary)`), and the engine compares the callee's actual type with it. A mismatch, a null entry or an index outside the table traps. This is what the tests show, on Bun and on wasmtime (`examples/indirect.wat`: slot 0 is `() -> i32`, slot 1 is `(i32) -> i32`, slot 2 does not exist):

| Call | Bun (JavaScriptCore) | wasmtime 50.0.2 |
|---|---|---|
| `call_nullary(0)` | 7 | 7 |
| `call_nullary(1)` | `call_indirect to a signature that does not match` | `wasm trap: indirect call type mismatch` |
| `call_nullary(2)` | `Out of bounds call_indirect` | `wasm trap: undefined element: out of bounds table access` |

<!-- listing file:corelib/wat/examples/indirect.wat -->
```wat
;; call_indirect checks the callee's type at the call, not at link time: the table is untyped data.
(module
  (type $nullary (func (result i32)))
  (type $unary (func (param i32) (result i32)))

  (func $seven (type $nullary) (i32.const 7))
  (func $double (type $unary) (i32.mul (local.get 0) (i32.const 2)))
  (table $slots funcref (elem $seven $double))

  ;; slot 0 matches the expected type; slot 1 has another type and traps; slot 2 is out of bounds and traps.
  (func (export "call_nullary") (param $slot i32) (result i32)
    (call_indirect $slots (type $nullary) (local.get $slot))))
```
<!-- /listing -->

The compiled corelib has a table of 2 `funcref`s too: a C function pointer is an index into it, and the corelib's console (`ay_console.put`, set to the function that calls the `console_put` import) is called through `call_indirect` (19 call sites in the module, all `(type 0)`, observed with `wasm-tools print`). Control-flow integrity falls out of the design: an indirect call can only reach functions that are in the table and have the expected type, and a return address is not a value the program can overwrite (the call stack is not in linear memory).

## Imports, exports and the embedder

A module cannot do anything by itself: no memory outside its own, no clock, no file, no network. It has **imports**, declared as `(module-name, field-name, type)`, and **exports**. A host program, the **embedder** (a browser, `wasmtime`, Bun, or `host.ts` here), *instantiates* the module: it supplies a value for each import, the engine checks each against the declared type (a wrong-typed import fails to link), allocates the module's memories, tables and globals, runs the active segments and the start function, and returns an *instance* whose exports the host can call. In the spec's terms the instance lives in a *store* owned by the embedder; two instances share state only through objects the embedder hands to both (a memory, a table, a function).

`host.ts` is that embedder in under 50 lines. It instantiates the corelib with one import, the serial port, and each app with one import, the corelib's `ay_syscall` export:

<!-- listing file:corelib/host/host.ts#22-31 -->
```ts
export function instantiateCore(corelib: BufferSource, console_put: (byte: number) => void): CoreExports {
  const instance = new WebAssembly.Instance(new WebAssembly.Module(corelib), { ay_host: { console_put } });
  return instance.exports as unknown as CoreExports;
}

/** Instantiate an app module: its only way out of its sandbox is the corelib's syscall entry. */
export function instantiateApp(app: BufferSource, corelib: CoreExports): AppExports & WebAssembly.Exports {
  const imports = { ay: { syscall: corelib.ay_syscall } };
  return new WebAssembly.Instance(new WebAssembly.Module(app), imports).exports as unknown as AppExports;
}
```
<!-- /listing -->

`WebAssembly.Module.imports` makes the contract inspectable: `corelib.test.ts` asserts that the corelib imports exactly `ay_host.console_put` and an app exactly `ay.syscall`. Nothing else is reachable from inside.

### Why one import: `ay.syscall(nr, a0, a1, a2) -> i32`

An app module's whole interface to the world is this one function. That is a design choice of the corelib, and it fits the capability model for four reasons.

1. **No ambient authority.** A module's imports are its authority, fixed at link time by the embedder. A module with an `ay.syscall` import and nothing else cannot name a file, a socket or a clock; it can only ask the corelib. Many imports would spread policy over many host functions; one funnel puts all policy in one place, the capability check inside `Core::syscall`.
2. **Handles, not pointers.** Every argument is a 32-bit integer; there is no address into the caller's memory, because the corelib and the app have **separate linear memories** and neither can read the other's. `LOG` therefore packs up to four bytes into one argument, and messages travel in four registers (`MSG_SET`, `MSG_GET`) rather than a buffer. What an app holds is a *capability handle*: an index into a per-task table, checked against the rights on every call.
3. **Forging does not work.** The test "an app cannot name a corelib object it holds no capability for" calls `LOG` through handle 5 (never granted) and handle 1 (an endpoint, not the console); both return `-2` (`EBADCAP`) and nothing is printed. A handle is meaningful only inside its task's table, and rights only shrink (`CAP_DERIVE` computes `rights & mask`).
4. **One signature for every target.** `include/ay_abi.h` declares `ay_syscall` once; under `__wasm__` it is the import `("ay", "syscall")`, elsewhere the platform provides it (a register convention on x86-64, AArch64 and RISC-V). The same ABI, four words in and one out, is a Wasm import and a CPU instruction, and chapter 8 describes the numbers.

The shim shows the next layer: typed wrappers (`version`, `yield`, `frame_alloc`, `log`) so that a caller sees functions, and `invoke(slot)` over a `funcref` table. A real SDK would generate those wrappers from `ay_abi.h`.

## Cooperative `step()` instead of stack switching

The corelib has a scheduler but no preemption. Wasm cannot suspend a running function from outside: the call stack is not a value an embedder can save, and a hostile app that loops forever cannot be interrupted by the module system alone (an engine may add fuel or epoch interruption; that is an engine feature, not the language). So an app is not a function that runs to its end: it exports `step() -> i32`, the corelib's scheduler calls it, and the result is `0` done, `1` yield, anything else a fault.

`hello.wat` is therefore a state machine. Its `$state` global holds the program counter between steps (print the greeting, send four words, receive and check them); `return (i32.const 1)` is the "yield". Each step runs briefly and returns, and the scheduler (`ay_sched_next`, `ay_sched_report`) chooses the next task round-robin. The price is that the app cannot keep local variables across a yield: everything that must survive is a global or lives in memory.

The **stack-switching** proposal (typed continuations: `cont.new`, `resume`, `suspend`) would let an app suspend in the middle of a call and keep its stack. Its status, read in the feature table on 2026-11-17: phase 3 on [webassembly.org/features](https://webassembly.org/features/) (via `WebAssembly/website/features.json`); Chrome needs a flag; Bun's JavaScriptCore rejects the module with `Type is non-Func, non-Struct, and non-Array`; wasmtime 50.0.2 runs it with `-W stack-switching` on x86-64 Linux (observed on `fr1`) and refuses it on Windows with "the wasm_stack_switching feature is not supported on this compiler configuration". `examples/stack_switching.wat` in chapter 6 writes the same shape as a coroutine. JS Promise Integration (phase 5) solves the narrower case of a Wasm call that waits on a JavaScript promise.

## Traps

A **trap** is an unrecoverable abort of the running computation. WebAssembly code cannot handle it: the engine unwinds to the embedder, which sees an error (`WebAssembly.RuntimeError` in JavaScript, `wasm trap: ...` in wasmtime). Memory and globals keep whatever they held. The exception-handling proposal does not catch traps either: `try_table` with `catch_all` around `unreachable` still ends in the trap on both engines (`examples/traps.wat`). Observed, one row per cause:

| Cause | Instruction | Bun (JavaScriptCore) message | wasmtime 50.0.2 |
|---|---|---|---|
| explicit | `unreachable` | `Unreachable code should not be executed` | `wasm unreachable instruction executed` |
| divide by zero | `i32.div_s 1 0` | `Division by zero` | `integer divide by zero` |
| signed overflow | `i32.div_s INT_MIN -1` | `Integer overflow` | `integer overflow` |
| float to int out of range or NaN | `i32.trunc_f32_s` | `Out of bounds Trunc operation` | `integer overflow` (3e9; NaN not run) |
| memory out of bounds | `i32.load` past the end | `Out of bounds memory access` | `out of bounds memory access` |
| `call_indirect` | type mismatch, bad index | see the table above | see the table above |
| stack exhaustion | unbounded recursion | `RangeError: Maximum call stack size exceeded` (not a `RuntimeError`) | `call stack exhausted` |
| failed GC cast | `ref.cast` to the wrong type | `ref.cast failed to cast reference to target heap type` | `cast failure` |

The saturating conversions (`i32.trunc_sat_f32_s`) are the non-trapping alternative, which clang's default CPU enables and `-mcpu=mvp` does not: for `(int32_t)f` the `generic` CPU gives `i32.trunc_sat_f32_s`, `mvp` gives a range check around `i32.trunc_f32_s` (chapter 6).

The corelib module contains no `unreachable` instruction (observed with `wasm-tools print`): it reports errors by return code. An app that traps inside `step()` has faulted. `host.ts` does not catch that exception (it ends `run`); an embedder that wants the scheduler to survive would catch it and call `ay_sched_report(id, 2)`, which the corelib turns into `core: task N faulted`. The syscall path never traps for a bad request; it returns `EBADCAP`, `EPERM`, `EINVAL`.

## Determinism, and what is not in Wasm

Wasm is deterministic by construction: for the same module and inputs, every instruction has one result, with a short list of exceptions that the spec names. These are the places where two engines may legitimately differ ([spec 4.3 "Numerics"](https://www.w3.org/TR/wasm-core-2/#numerics), read 2026-11-17: "Some operators are non-deterministic, because they can return one of several possible results (such as different NaN values)"):

- **NaN payloads**: the sign and payload of a NaN produced by float arithmetic is any *arithmetic NaN*; copies and `abs`, `neg`, `copysign` are exact.
- **Relaxed SIMD**: the instructions added in Wasm 3.0 have "behaviour that is non-deterministic and implementation-dependent". On Bun on x86-64 `i32x4.relaxed_trunc_f32x4_s` returned `0x80000000` for a NaN lane and for lanes of 1e10 and -1e10 (`examples/relaxed_simd.wat`); the spec allows another result on another CPU, so the test asserts only in-range lanes.
- **Resource limits**: `memory.grow` may fail even when it would fit, stack depth and module size limits are the engine's choice.
- **Threads**: with shared memory, a data race has the weak-memory outcomes of the memory model.
- **Host functions**: the spec says an import "behaves non-deterministically, but within certain constraints that ensure the integrity of the runtime".

What the language does **not** have: no system calls, no file or network or clock, no threads (only the proposal), no signals, no way to read the call stack or a return address, no address-of for locals, no `mmap`, no dynamic code generation inside the sandbox. Everything the program does to the outside is a call to an import, which is exactly what the corelib exploits: Wasm provides computation and isolation, and the syscall import is the single place where the program asks for authority. The *same* property is why the module can run in a browser, in wasmtime or inside a kernel.

## Sources

- WebAssembly Core Specification, release 3.0 (W3C Candidate Recommendation Draft): <https://www.w3.org/TR/wasm-core-2/> (read 2026-11-17).
- Design rationale: <https://github.com/WebAssembly/design/blob/main/Rationale.md> (read 2026-11-17).
- Feature and engine status: <https://webassembly.org/features/> and its data file <https://github.com/WebAssembly/website/blob/main/features.json> (read 2026-11-17).
- Tools run: `wasm-tools 1.263.0` (`parse`, `print`, `dump`, `objdump`, `validate --features all`), `clang 23.1.5` (with `wasm-ld` from LLD 23.1.5), Bun 1.4.5-canary.1+9098897a1, wasmtime 50.0.2 (<https://github.com/bytecodealliance/wasmtime/releases/tag/v50.0.2>, release of 2026-11-16).
- Code: `corelib/src/*.c`, `tools/corelib.ts`, `corelib/wat/*.wat`, `corelib/wat/examples/*.wat`, `corelib/host/*.test.ts`.
