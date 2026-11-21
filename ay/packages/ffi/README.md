# @aphrody/ay-ffi

Bun bindings to system C libraries. Every module is loaded on its own, so importing one never loads the others.

Legacy: restored from npm `@aphrody/gmp@0.1.0`, `@aphrody/zstd@0.1.0`, `@aphrody/libmagic@0.1.0`, `@aphrody/xml2@0.1.0`, `@aphrody/archive@0.1.0`, `@aphrody/regex-re2@0.1.0`, `@aphrody/http-curl@0.1.0` and `@aphrody/hash-cc@0.1.0`.

| Subpath | Library | Mechanism | Needs |
|---|---|---|---|
| `@aphrody/ay-ffi/gmp` | libgmp.so.10 | bun:ffi | `GmpInt`: arbitrary-precision integers |
| `@aphrody/ay-ffi/zstd` | libzstd.so.1 | bun:ffi | `compress`, `decompress` |
| `@aphrody/ay-ffi/libmagic` | libmagic.so.1 | bun:ffi | `MagicCookie`, `detectMime`, `detectMimeBuffer` |
| `@aphrody/ay-ffi/xml2` | libxml2.so.16 | bun:ffi | `parseXml`, `parseXmlFile` |
| `@aphrody/ay-ffi/archive` | libarchive.so.13 | bun:ffi | `listEntries`, `extractAll`, `createTarGz`; `AY_LIBARCHIVE` overrides the library name or path |
| `@aphrody/ay-ffi/regex-re2` | libre2.so.11 | bun:ffi + g++-built wrapper | `Re2`, `re2`; linear-time regex |
| `@aphrody/ay-ffi/http-curl` | libcurl | bun:ffi + gcc-built wrapper | `get`, `head` |
| `@aphrody/ay-ffi/hash-cc` | none | bun:cc (TinyCC) | `fnv1a64`, `djb2`, `xorSum`, `popcountBuf` |

The wrappers in `native/` (`re2_wrapper.cpp`, `curl_wrapper.c`) are compiled on first import by `src/native.ts` into
`os.tmpdir()/ay-ffi/<hash>/`, keyed by the hash of the source, the compiler and the link flags. `AY_FFI_CC` and `AY_FFI_CXX`
override the compilers (default `gcc`, `g++`). The development packages (`libre2-dev`, `libcurl4-openssl-dev`) are needed to
build them. The bindings assume Linux x86_64 (struct offsets in `xml2`, `gmp`).

```ts
import { GmpInt } from "@aphrody/ay-ffi/gmp";
import { compress, decompress } from "@aphrody/ay-ffi/zstd";
```

## Test

```
bun test
```

The tests need the libraries above, `tar` and `gcc`/`g++`; `hash-cc` needs a Bun build that ships `bun:ffi` `cc`.
