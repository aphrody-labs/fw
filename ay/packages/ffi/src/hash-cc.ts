// bun:cc: compiles `native/hash.c` at run time (TinyCC embedded in Bun) and exposes
// 4 C functions to TS. No build step, no .so.
//
// bun:ffi converts a Uint8Array to a native pointer automatically for arguments typed "ptr":
// no manual encoding is needed.

import { cc } from "bun:ffi";
import { nativeSource } from "./native.ts";

const { symbols } = cc({
  source: nativeSource("hash.c"),
  symbols: {
    fnv1a64: { args: ["ptr", "u64"], returns: "u64" },
    djb2: { args: ["ptr", "u64"], returns: "u32" },
    xor_sum: { args: ["ptr", "u64"], returns: "u32" },
    popcount_buf: { args: ["ptr", "u64"], returns: "i32" },
  },
});

export function fnv1a64(data: Uint8Array): bigint {
  return symbols.fnv1a64(data, BigInt(data.byteLength)) as bigint;
}

export function djb2(data: Uint8Array): number {
  return Number(symbols.djb2(data, BigInt(data.byteLength)));
}

export function xorSum(data: Uint8Array): number {
  return Number(symbols.xor_sum(data, BigInt(data.byteLength)));
}

export function popcountBuf(data: Uint8Array): number {
  return Number(symbols.popcount_buf(data, BigInt(data.byteLength)));
}
