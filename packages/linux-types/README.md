# @aphrody/linux-types

Linux kernel UAPI for Bun, generated from the kernel itself (the sanitized headers of the stable tree pinned in `kernel.toml`), per arch (`x86_64`, `aarch64`):

- every UAPI struct and union with exact layout (size, alignment, member offsets, bitfields, arrays, anonymous members, flexible arrays), as a TypeScript interface plus a layout descriptor with `read` / `write` / `zeroed` / `encode` codecs over `ArrayBuffer` / `DataView`;
- every enum, and every integer constant (object-like macros evaluated by clang, plus enumerators);
- every ioctl request decoded into `dir`, `type`, `nr`, `size` with the argument layout the macro names;
- the native syscall table (name to number and back);
- `syscall()`, `ioctl()`, `addressOf()` and `allocAligned()` over `bun:ffi` and libc.

```ts
import { allocAligned, syscall } from "@aphrody/linux-types";
import { struct_new_utsname, syscalls } from "@aphrody/linux-types/x86_64";

const buffer = allocAligned(struct_new_utsname);
syscall(syscalls.uname, buffer);
struct_new_utsname.read(buffer).release; // "7.3.0-10-generic"
```

Per-arch modules: `@aphrody/linux-types/x86_64` and `@aphrody/linux-types/aarch64` (`src/<arch>/`: `structs.ts`, `enums.ts`, `constants.ts`, `ioctls.ts`, `syscalls.ts`, `meta.ts`). `meta.counts` has the counts, `meta.headersExcluded` the headers that cannot be compiled into the combined unit and why.

## Names (C to TypeScript)

| C | export |
| --- | --- |
| `struct foo` | `struct_foo` (interface and layout, same name) |
| `union foo` | `union_foo` |
| `enum foo` | `enum_foo` (object `as const` and union type) |
| anonymous record or enum behind `typedef ... name;` | `typedef_name` |
| `typedef struct foo name;` | `typedef_name`, an alias of `struct_foo` |
| macro or enumerator `NAME` | `NAME` (constant) |
| `_IO*` macro, legacy `TC*`/`TIOC*`/`FIO*` request | `ioctls.NAME` |
| `__NR_name` | `syscalls.name`, `syscallNames[number]` |
| anonymous member | key `_anon0`, `_anon1`, ... in declaration order |

The generator fails on any duplicate export, on two different definitions of one C name, and on a constant that is not a valid TypeScript identifier; nothing is renamed silently.

## Values

`i8 u8 i16 u16 i32 u32 f32 f64` are `number`; `i64`, `u64`, `long`, `size_t` and **pointers are `bigint`** (a pointer member holds the address; `addressOf(buffer)` gives a buffer's address as a bigint); `char[N]` is a string (UTF-8 up to the first NUL, so binary `char` arrays are better read from the raw bytes); other one-byte arrays are `Uint8Array` / `Int8Array`; bitfields are `number` (`bigint` when the declared type is 64-bit); a flexible array member decodes the elements that fit after it (`zeroed(extra)` allocates the tail). Constants are `number`, or `bigint` above 2^53. Little-endian only. A union's `write` takes any subset of its members; its `read` returns all of them.

Libc-supplied types the kernel headers expect (`struct sockaddr`, `struct sockaddr_storage`) come from a small stub with the glibc ABI; they are marked in `structs.ts`. Legacy tty requests have no argument type in the headers; `TIOCGWINSZ`, `TIOCSWINSZ`, `TCGETS*`, `TCSETS*`, `TCGETA`, `TCSETA*` get theirs from tty_ioctl(4), listed in `ioctls.ts`.

`syscall()` calls libc `syscall(2)` and reads `errno` right after a `-1` result (libc keeps it per thread; the read is a second FFI call, so do not run other libc-touching code between them). It throws `SyscallError`.

## Regenerate and gate

Heavy: runs on `fr1` (clang, pahole, bpftool).

```sh
bun tools/fr1.ts run -- bash -c 'cd packages/linux-types && bun install --frozen-lockfile && bun run headers --out /srv/xbuild/scratch/linux-types/headers && bun run gen --headers /srv/xbuild/scratch/linux-types/headers'
bun run check                                   # type check (bun check) of src, gen and test
bun run test                                    # codecs, kernel calls over bun:ffi, layouts against clang
bun run gen --check --headers <dir>             # drift: regenerate to a temp dir, diff against src/
```

`headers` fetches the commit pinned in `kernel.toml` as a sparse partial clone and runs `make headers_install` for each arch; no kernel is built. `gen` runs, per arch: an all-headers translation unit (headers that cannot be combined are pruned and listed in `meta`), `clang -g -fno-eliminate-unused-debug-types`, `pahole -J` (BTF), `bpftool -j btf dump`, macro evaluation by clang, then a cross-check of every record size and every named member offset against `sizeof` / `offsetof` evaluated by clang (a mismatch fails the run). Output is deterministic: no timestamps, no absolute paths.
