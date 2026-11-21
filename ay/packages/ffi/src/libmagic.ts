// bun:ffi binding to libmagic.so.1: MIME type detection (the equivalent of the `file` command).
// Neither Bun nor Node has a native equivalent; the npm libraries (`file-type`, `mime-types`) are
// pure JS and much less reliable.
//
// Minimal C API:
//   magic_t magic_open(int flags);
//   int     magic_load(magic_t cookie, const char *filename); // NULL = default database
//   const char* magic_file  (magic_t cookie, const char *filename);
//   const char* magic_buffer(magic_t cookie, const void *buf, size_t length);
//   const char* magic_error(magic_t cookie);
//   void    magic_close(magic_t cookie);

import { dlopen, FFIType, ptr } from "bun:ffi";

const MAGIC_MIME_TYPE = 0x000010; // "text/plain"
export const MAGIC_MIME_ENCODING = 0x000400;
export const MAGIC_MIME = MAGIC_MIME_TYPE | MAGIC_MIME_ENCODING;

const lib = dlopen("libmagic.so.1", {
  magic_open: { args: [FFIType.i32], returns: FFIType.ptr },
  magic_load: { args: [FFIType.ptr, FFIType.cstring], returns: FFIType.i32 },
  magic_file: { args: [FFIType.ptr, FFIType.cstring], returns: FFIType.cstring },
  magic_buffer: { args: [FFIType.ptr, FFIType.ptr, FFIType.u64], returns: FFIType.cstring },
  magic_error: { args: [FFIType.ptr], returns: FFIType.cstring },
  magic_close: { args: [FFIType.ptr], returns: FFIType.void },
});

type Cookie = ReturnType<typeof lib.symbols.magic_open>;

/** Magic handle: wraps a libmagic cookie opened with the chosen flags. */
export class MagicCookie {
  private readonly cookie: Cookie;
  private closed = false;

  constructor(flags: number = MAGIC_MIME_TYPE) {
    const cookie = lib.symbols.magic_open(flags);
    if (!cookie) throw new Error("magic_open failed");
    this.cookie = cookie;
    // magic_load(NULL) loads the default database (magic.mgc).
    const rc = lib.symbols.magic_load(cookie, null);
    if (rc !== 0) {
      const err = lib.symbols.magic_error(cookie);
      lib.symbols.magic_close(cookie);
      throw new Error(`magic_load: ${err?.toString() ?? "unknown"}`);
    }
  }

  /** Detects the MIME type of a file on disk. */
  file(path: string): string {
    this.ensureOpen();
    const encoded = Buffer.from(path + "\0", "utf8");
    const result = lib.symbols.magic_file(this.cookie, ptr(encoded));
    return result?.toString() ?? "";
  }

  /** Detects the MIME type of an in-memory Uint8Array / Buffer. */
  buffer(data: Uint8Array): string {
    this.ensureOpen();
    if (data.byteLength === 0) return "application/x-empty";
    const result = lib.symbols.magic_buffer(this.cookie, ptr(data), BigInt(data.byteLength));
    return result?.toString() ?? "";
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;
    lib.symbols.magic_close(this.cookie);
  }

  private ensureOpen() {
    if (this.closed) throw new Error("cookie closed");
  }
}

/** Helper: detects the MIME type of a file (opens and closes a cookie on every call). */
export function detectMime(path: string): string {
  const c = new MagicCookie(MAGIC_MIME_TYPE);
  try {
    return c.file(path);
  } finally {
    c.close();
  }
}

/** Helper: detects the MIME type of a buffer. */
export function detectMimeBuffer(data: Uint8Array): string {
  const c = new MagicCookie(MAGIC_MIME_TYPE);
  try {
    return c.buffer(data);
  } finally {
    c.close();
  }
}
