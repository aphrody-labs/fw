// RE2 bindings for Bun via bun:ffi.
//
// RE2 provides linear-time regex matching, making it safe for user-supplied
// patterns. The C++ wrapper (native/re2_wrapper.cpp) is compiled to a .so on first
// import with g++ (see native.ts), then loaded with dlopen.
//
// Requires: g++ + libre2-dev on the host (build time), libre2.so.11 (runtime).

import { dlopen, FFIType, ptr, CString, toArrayBuffer, type Pointer } from "bun:ffi";
import { buildSharedLibrary, nativeSource } from "./native.ts";

// ---------------------------------------------------------------------------
// FFI bindings (the .so is built once and cached under os.tmpdir())
// ---------------------------------------------------------------------------

const soPath = buildSharedLibrary({
  source: nativeSource("re2_wrapper.cpp"),
  libs: ["-lre2"],
  owner: "@aphrody/ay-ffi/regex-re2",
});

const lib = dlopen(soPath, {
  re2_new:         { args: [FFIType.cstring], returns: FFIType.ptr },
  re2_free:        { args: [FFIType.ptr],     returns: FFIType.void },
  re2_ok:          { args: [FFIType.ptr],     returns: FFIType.i32 },
  re2_error:       { args: [FFIType.ptr],     returns: FFIType.ptr },
  re2_pattern:     { args: [FFIType.ptr],     returns: FFIType.ptr },
  re2_full_match:  { args: [FFIType.ptr, FFIType.cstring], returns: FFIType.i32 },
  re2_partial_match: { args: [FFIType.ptr, FFIType.cstring], returns: FFIType.i32 },
  re2_num_groups:  { args: [FFIType.ptr],     returns: FFIType.i32 },
  re2_exec:        { args: [FFIType.ptr, FFIType.cstring], returns: FFIType.ptr },
  re2_free_exec:   { args: [FFIType.ptr, FFIType.i32], returns: FFIType.void },
});

const s = lib.symbols;

// re2_exec returns this address instead of a buffer when the pattern has no
// capturing group but matches.
const NO_GROUPS_SENTINEL = 1;
// The C wrapper reports at most this many groups.
const MAX_GROUPS = 32;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function toCString(str: string): Uint8Array {
  const enc = new TextEncoder();
  const bytes = enc.encode(str);
  const buf = new Uint8Array(bytes.byteLength + 1);
  buf.set(bytes);
  buf[bytes.byteLength] = 0;
  return buf;
}

// Parse the packed blob returned by re2_exec.
// Layout: [uint32 count][uint32 off0][uint32 len0]...[...string bytes...]
function parseExecResult(rawPtr: Pointer, n: number): string[] {
  const HEADER = 4 * (1 + 2 * n); // bytes for count + (off,len)*n
  const headerBuf = new Uint8Array(toArrayBuffer(rawPtr, 0, HEADER));
  const view = new DataView(headerBuf.buffer);
  const count = view.getUint32(0, true);
  let dataSize = 0;
  for (let i = 0; i < count; i++) dataSize += view.getUint32(4 + 8 * i + 4, true);
  const fullBuf = new Uint8Array(toArrayBuffer(rawPtr, 0, HEADER + dataSize));
  const fullView = new DataView(fullBuf.buffer);
  const decoder = new TextDecoder();
  const results: string[] = [];
  for (let i = 0; i < count; i++) {
    const off = fullView.getUint32(4 + 8 * i, true);
    const len = fullView.getUint32(4 + 8 * i + 4, true);
    results.push(decoder.decode(fullBuf.subarray(HEADER + off, HEADER + off + len)));
  }
  return results;
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * A compiled RE2 regular expression.
 *
 * Uses `using` / `[Symbol.dispose]` for deterministic memory cleanup:
 *   using re = new Re2("pattern");
 *
 * Flags are translated to RE2 options:
 *   i -> case-insensitive
 *   s -> dot matches newline
 *   m -> multiline (not supported by RE2, silently ignored)
 */
export class Re2 {
  private readonly _ptr: Pointer;
  private _disposed = false;

  constructor(pattern: string, flags?: string) {
    // RE2 embeds flags as (?flags) inline.
    let re2pattern = pattern;
    if (flags) {
      const re2flags = flags
        .replace(/g/g, "")   // no global in RE2 (exec always returns first match)
        .replace(/m/g, "")   // multiline -- not supported; ignore
        .replace(/i/g, "i")
        .replace(/s/g, "s");
      if (re2flags.length > 0) {
        re2pattern = `(?${re2flags})${pattern}`;
      }
    }

    const buf = toCString(re2pattern);
    const p = s.re2_new(ptr(buf));
    if (!p) throw new Error(`re2_new returned null for pattern: ${re2pattern}`);
    const ok = s.re2_ok(p);
    if (!ok) {
      const errPtr = s.re2_error(p);
      const errMsg = errPtr ? new CString(errPtr).toString() : "unknown error";
      s.re2_free(p);
      throw new Error(`RE2 compile error: ${errMsg}`);
    }
    this._ptr = p;
  }

  /** The original pattern string as stored by RE2. */
  get source(): string {
    this._check();
    const p = s.re2_pattern(this._ptr);
    return p ? new CString(p).toString() : "";
  }

  /** Partial match -- returns true if the pattern matches anywhere in text. */
  test(text: string): boolean {
    this._check();
    const buf = toCString(text);
    return s.re2_partial_match(this._ptr, ptr(buf)) === 1;
  }

  /** Full match -- the entire text must match the pattern. */
  fullMatch(text: string): boolean {
    this._check();
    const buf = toCString(text);
    return s.re2_full_match(this._ptr, ptr(buf)) === 1;
  }

  /**
   * Partial match with capturing groups.
   * Returns an array of captured group strings (length = number of groups),
   * or null if no match.
   * Does NOT include the full match at index 0 (unlike JS RegExp.exec).
   */
  exec(text: string): string[] | null {
    this._check();
    const n = Math.min(Number(s.re2_num_groups(this._ptr)), MAX_GROUPS);

    const textBuf = toCString(text);
    const rawPtr = s.re2_exec(this._ptr, ptr(textBuf));

    if (!rawPtr) return null;

    // Sentinel: no capturing groups but partial match succeeded
    if (Number(rawPtr) === NO_GROUPS_SENTINEL) return [];

    try {
      return parseExecResult(rawPtr, n);
    } finally {
      s.re2_free_exec(rawPtr, n);
    }
  }

  /** Free the underlying RE2 object. Called automatically by `using`. */
  close(): void {
    if (this._disposed) return;
    this._disposed = true;
    s.re2_free(this._ptr);
  }

  [Symbol.dispose](): void {
    this.close();
  }

  private _check(): void {
    if (this._disposed) throw new Error("Re2 instance has been disposed");
  }
}

/**
 * Convenience factory -- same as `new Re2(pattern, flags)`.
 *
 * @example
 *   const match = re2("(\\d+)").exec("port 8080"); // ["8080"]
 */
export function re2(pattern: string, flags?: string): Re2 {
  return new Re2(pattern, flags);
}
