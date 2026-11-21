// bun:ffi binding to libzstd.so.1. Node has no native zstd (it needs @mongodb-js/zstd or node-zstd)
// and Bun does not shim every API yet: a good FFI use case.
//
// API: compress(data, level?) / decompress(data)
// libzstd signatures:
//   size_t ZSTD_compress(void* dst, size_t dstCap, const void* src, size_t srcSize, int level);
//   size_t ZSTD_decompress(void* dst, size_t dstCap, const void* src, size_t srcSize);
//   size_t ZSTD_compressBound(size_t srcSize);
//   unsigned ZSTD_isError(size_t code);
//   const char* ZSTD_getErrorName(size_t code);
//   size_t ZSTD_getFrameContentSize(const void* src, size_t srcSize);

import { dlopen, FFIType, ptr } from "bun:ffi";

const lib = dlopen("libzstd.so.1", {
  ZSTD_compress: {
    args: [FFIType.ptr, FFIType.u64, FFIType.ptr, FFIType.u64, FFIType.i32],
    returns: FFIType.u64,
  },
  ZSTD_decompress: {
    args: [FFIType.ptr, FFIType.u64, FFIType.ptr, FFIType.u64],
    returns: FFIType.u64,
  },
  ZSTD_compressBound: {
    args: [FFIType.u64],
    returns: FFIType.u64,
  },
  ZSTD_isError: {
    args: [FFIType.u64],
    returns: FFIType.u32,
  },
  ZSTD_getErrorName: {
    args: [FFIType.u64],
    returns: FFIType.cstring,
  },
  ZSTD_getFrameContentSize: {
    args: [FFIType.ptr, FFIType.u64],
    returns: FFIType.u64,
  },
});

function check(code: number | bigint, ctx: string): number {
  const c = Number(code);
  if (lib.symbols.ZSTD_isError(c)) {
    const err = lib.symbols.ZSTD_getErrorName(c);
    throw new Error(`zstd ${ctx}: ${err?.toString() ?? "unknown"}`);
  }
  return c;
}

// Pointer to an empty buffer: a 1-byte placeholder is used because `ptr()` rejects
// zero-length buffers. libzstd handles srcSize=0 correctly.
const EMPTY_PTR_BUF = new Uint8Array(1);

/** Compresses a Uint8Array. `level`: 1-22 (default 3, the recommended level). */
export function compress(data: Uint8Array, level = 3): Uint8Array {
  const bound = Number(lib.symbols.ZSTD_compressBound(BigInt(data.byteLength)));
  const out = new Uint8Array(Math.max(bound, 1));
  const srcPtr = data.byteLength === 0 ? ptr(EMPTY_PTR_BUF) : ptr(data);
  const n = check(
    lib.symbols.ZSTD_compress(ptr(out), BigInt(bound), srcPtr, BigInt(data.byteLength), level),
    "compress",
  );
  return out.subarray(0, n);
}

/** Decompresses a zstd payload. */
export function decompress(data: Uint8Array): Uint8Array {
  if (data.byteLength === 0) return new Uint8Array(0);
  const size = Number(
    lib.symbols.ZSTD_getFrameContentSize(ptr(data), BigInt(data.byteLength)),
  );
  if (size < 0 || size > Number.MAX_SAFE_INTEGER) {
    throw new Error("zstd decompress: unknown/corrupt frame size");
  }
  if (size === 0) return new Uint8Array(0);
  const out = new Uint8Array(size);
  const n = check(
    lib.symbols.ZSTD_decompress(ptr(out), BigInt(size), ptr(data), BigInt(data.byteLength)),
    "decompress",
  );
  return out.subarray(0, n);
}
