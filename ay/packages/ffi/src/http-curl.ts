// libcurl binding for Bun via bun:ffi and a gcc-built wrapper.
//
// bun:cc (TinyCC) cannot compile libcurl usage correctly: curl_easy_setopt is variadic and uses GCC
// typecheck macros that generate code incompatible with the x86_64 System V calling convention
// of TCC (SIGSEGV crash).
//
// Solution: native/curl_wrapper.c is compiled with gcc on first load of the module (src/native.ts),
// and the resulting .so is loaded with dlopen. The .so is cached in a temporary directory keyed by
// the hash of the source, so it is recompiled only when the source changes.
//
// Exposed API:
//   get(url)  -> { body: string, status: number }
//   head(url) -> { status: number }

import { dlopen, FFIType, ptr, toArrayBuffer } from "bun:ffi";
import { buildSharedLibrary, nativeSource } from "./native.ts";

function loadLib() {
  const soPath = buildSharedLibrary({
    source: nativeSource("curl_wrapper.c"),
    libs: ["-lcurl"],
    owner: "@aphrody/ay-ffi/http-curl",
  });

  return dlopen(soPath, {
    // int curl_get(const char *url, Buffer *buf)
    curl_get: { args: [FFIType.cstring, FFIType.ptr], returns: FFIType.i32 },
    // long curl_get_status(const char *url)
    curl_get_status: { args: [FFIType.cstring], returns: FFIType.i64 },
    // long curl_get_response_code(const char *url)
    curl_get_response_code: { args: [FFIType.cstring], returns: FFIType.i64 },
    // const char *buffer_data(Buffer *buf)
    buffer_data: { args: [FFIType.ptr], returns: FFIType.ptr },
    // void buffer_free(Buffer *buf)
    buffer_free: { args: [FFIType.ptr], returns: FFIType.void },
  });
}

const lib = loadLib();
const s = lib.symbols;

/** Encodes a string as a NUL-terminated C string. */
function cstr(str: string): Uint8Array {
  return Buffer.from(str + "\0", "utf8");
}

// sizeof(Buffer) = sizeof(char*) + sizeof(size_t) = 8 + 8 = 16 bytes (x86_64)
const BUFFER_STRUCT_SIZE = 16;

/**
 * Performs an HTTP GET on `url`.
 * @returns `{ body, status }`: body is UTF-8, status is the HTTP status code.
 * @throws  When cURL cannot reach the URL.
 */
export async function get(url: string): Promise<{ body: string; status: number }> {
  const urlBuf = cstr(url);
  // Allocate the C Buffer on the JS heap (16 bytes, zero-initialised)
  const bufStruct = new Uint8Array(BUFFER_STRUCT_SIZE);

  const n = s.curl_get(ptr(urlBuf), ptr(bufStruct)) as number;
  if (n < 0) {
    throw new Error(`curl GET failed for URL: ${url}`);
  }

  let body = "";
  if (n > 0) {
    // buffer_data() returns the char* malloc'd in curl_wrapper.c
    const dataPtr = s.buffer_data(ptr(bufStruct));
    if (dataPtr) {
      // toArrayBuffer(ptr, byteOffset, byteLength): zero-copy view of the C heap
      const ab = toArrayBuffer(dataPtr, 0, n);
      body = new TextDecoder().decode(ab);
    }
  }

  // Free the C memory allocated by curl_get
  s.buffer_free(ptr(bufStruct));

  // Fetch the HTTP status code (separate request, discarded body)
  const status = Number(s.curl_get_response_code(ptr(cstr(url))));

  return { body, status: status < 0 ? 0 : status };
}

/**
 * Performs a HEAD request on `url` (no body, status code only).
 * @returns `{ status }`: HTTP status code.
 * @throws  When cURL cannot reach the URL.
 */
export async function head(url: string): Promise<{ status: number }> {
  const code = Number(s.curl_get_status(ptr(cstr(url))));
  if (code < 0) {
    throw new Error(`curl HEAD failed for URL: ${url}`);
  }
  return { status: code };
}
