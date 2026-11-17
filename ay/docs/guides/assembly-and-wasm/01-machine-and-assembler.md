# 1. The machine, the assembler and the code base

## What an assembler is

An assembly language is a human-readable spelling of a processor's machine code; the assembler is the program that turns that spelling into bytes. French Wikipedia puts it as "le langage de plus bas niveau qui représente le langage machine" and notes that in everyday speech the word *assembleur* names both the language and the tool ([fr.wikipedia.org/wiki/Assembleur](https://fr.wikipedia.org/wiki/Assembleur), read 2026-11-16). Three things happen during assembly:

1. **Mnemonics become opcodes.** `add eax, 1` becomes the bytes `83 C0 01`.
2. **Labels become addresses.** `jne .loop` is encoded as a displacement the assembler computes.
3. **Directives shape the output.** `.section`, `.align`, `.long`, `.skip` place code and data; they are not instructions.

Assembly is specific to one instruction set: the same idea is spelled differently for x86-64, AArch64 and RISC-V, and the same program written for one cannot run on another. Disassembly is lossy: names, comments and macros do not survive.

A compiler such as `clang` (LLVM) produces assembly or machine code from a higher-level language; an assembler only encodes. A linker (`ld.lld`, `lld-link`, `wasm-ld`) joins the objects, a loader maps the result into memory. `clang -S` shows what the compiler hands to the assembler. That is how every native listing in this guide is made.

## The machine model every ISA shares

All the architectures in this guide implement the same abstract machine:

- **Registers**: a few dozen word-sized storage cells inside the CPU. Arithmetic works on registers.
- **Memory**: a large byte-addressed array. Only load and store instructions touch it (RISC), or instructions may take memory operands (x86).
- **Program counter**: the address of the next instruction; branches change it.
- **Stack**: a region of memory addressed through a stack-pointer register, used for return addresses, spilled values and locals.
- **Flags or compare-and-branch**: x86 and AArch64 keep condition flags; RISC-V compares inside the branch.
- **Privilege levels**: user code cannot touch hardware; it asks the operating system through a *system call* instruction.

WebAssembly is a different kind of machine: a **stack machine** with typed values, no registers, no addresses of code, and a sandboxed linear memory. It is designed to be validated, compiled to any of the real ISAs, and run without trusting the module. Chapters 5 to 7 cover it.

| Concept | x86-64 | AArch64 | RISC-V RV64 | WebAssembly |
|---|---|---|---|---|
| Machine word | 64-bit | 64-bit | 64-bit | `i32`, `i64`, `f32`, `f64`, `v128` |
| General registers | 16 (`rax`..`r15`) | 31 (`x0`..`x30`) + `xzr` | 31 (`x1`..`x31`) + `zero` | none: operand stack and locals |
| Condition state | `RFLAGS` | `NZCV` | none | none: comparisons yield `i32` |
| Control flow | `jmp`, `jcc`, `call`, `ret` | `b`, `b.cond`, `bl`, `ret` | `j`, `beq`.., `jal`, `ret` | `block`, `loop`, `if`, `br`, `br_table`, `call` |
| System call | `syscall` | `svc #0` | `ecall` | call an imported function |
| Atomic compare-and-swap | `lock cmpxchg` | `cas` / `ldaxr`+`stlxr` | `lr`/`sc`, `amo*` | `i32.atomic.rmw.cmpxchg` |
| Where it runs | hardware | hardware | hardware | any engine that validates it |

## One code base

Everything in the guide comes from `corelib/`, the SDK corelib: a freestanding C23 library (no libc dependency; it carries its own `memcpy` and friends) holding the task table and scheduler, a frame allocator, an IPC bus and a capability-based syscall ABI (chapter 8). The ABI is the C header `corelib/include/ay_abi.h`. The same sources are built four ways, all with clang and lld:

- bare-metal `x86_64-unknown-none-elf`, booted in QEMU through a hand-written assembly entry (`corelib/arch/x86_64/boot.s`);
- `wasm32`, a WebAssembly module whose apps are other Wasm modules (`corelib/src/wasm.c`, host in `corelib/host`);
- a native process on the platform C library, with clang and with `clang-cl` + `lld-link` for the MSVC ABI;
- listings only: the pure functions are also compiled with `-S` for AArch64 and RISC-V.

The smallest function that shows the whole pipeline is the rights check of the capability model: a capability may perform an operation when it holds every right the operation needs.

<!-- listing c:corelib/src/abi.c#ay_rights_allow -->
```c
/// Whether a capability holding `have` may perform an operation that needs `need`.
AY_API uint32_t ay_rights_allow(uint32_t have, uint32_t need) { return (have & need) == need; }
```
<!-- /listing -->

Compiled for four targets (the function is exported as `ay_rights_allow` so every build has a stable symbol):

x86-64:

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

AArch64:

<!-- listing asm:ay_rights_allow:aarch64 -->
```asm
ay_rights_allow:
	bics	wzr, w1, w0
	cset	w0, eq
	ret
```
<!-- /listing -->

RISC-V (RV64GC):

<!-- listing asm:ay_rights_allow:riscv64 -->
```asm
ay_rights_allow:
	and	a0, a0, a1
	xor	a0, a0, a1
	seqz	a0, a0
	ret
```
<!-- /listing -->

WebAssembly (text format, linked module):

<!-- listing wat:ay_rights_allow -->
```wat
(func $ay_rights_allow (export "ay_rights_allow") (type 3) (param i32 i32) (result i32)
  local.get 1
  local.get 0
  i32.and
  local.get 1
  i32.eq
)
```
<!-- /listing -->

Read them side by side. The x86-64 and AArch64 versions use the System V and AAPCS64 argument registers (`edi`/`esi` and `w0`/`w1`), the result goes back in `eax`/`w0`. RISC-V uses `a0`/`a1`. WebAssembly has no registers: the arguments are locals, `local.get` pushes them on the operand stack and the instructions consume the stack, which is exactly why the function reads as postfix arithmetic.

## Tools used

| Tool | What it does here | Version used (2026-11-16) |
|---|---|---|
| `clang`, `lld`, `wasm-ld` | compile the corelib, `-S` shows native assembly, `wasm-ld` links Wasm | LLVM 23.1.5 here (23.1.6 on fr1); version pinned in `listings.toml` |
| `wasm-tools` | `parse` WAT to binary, `print` binary to WAT, `dump` annotated bytes, `validate`, `component` | 1.263.0 (released 2026-10-30) |
| `wabt` (`wat2wasm`, `wasm2wat`) | the reference-style alternative to `wasm-tools parse`/`print` | 1.0.42 (2026-09-15) |
| QEMU | boots the bare-metal image on fr1 | 11.0.3 |
| Bun | runs the Wasm modules through JavaScriptCore | 1.4.5 |

Sources: [github.com/bytecodealliance/wasm-tools](https://github.com/bytecodealliance/wasm-tools/releases), [github.com/WebAssembly/wabt](https://github.com/WebAssembly/wabt/releases), read with `gh api` on 2026-11-16.

## How the listings stay true

Every code block that comes from the code base sits between `<!-- listing SPEC -->` and `<!-- /listing -->` markers and is written by `tools/listings.ts`:

```sh
bun tools/listings.ts           # regenerate every block of docs/guides/assembly-and-wasm*
bun tools/listings.ts --check   # CI: fail when a block differs from what the code generates now
```

The generator compiles the corelib with the pinned toolchain (`docs/guides/assembly-and-wasm/listings.toml`), extracts the function from `clang -S` output or from `wasm-tools print`, and rewrites the block. When the code or the toolchain changes, `--check` fails until the guide is regenerated. Hand-written files (`corelib/wat/*.wat`, `corelib/asm/*.s`) are embedded the same way, so they cannot drift either.
