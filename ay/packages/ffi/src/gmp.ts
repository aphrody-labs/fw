// bun:ffi binding to libgmp.so.10: arbitrary-precision arithmetic.
//
// `mpz_t` is a 16-byte struct on x86_64 (an __mpz_struct):
//   int  _mp_alloc   // 4 bytes
//   int  _mp_size    // 4 bytes
//   mp_limb_t* _mp_d // 8 bytes (pointer)
// It is allocated as a Uint8Array(16) and its pointer is passed to the GMP functions.
// GMP manages its own internal heap through __gmpz_init / __gmpz_clear.

import { dlopen, FFIType, ptr } from "bun:ffi";

// ── FFI ──────────────────────────────────────────────────────────────────────

const lib = dlopen("libgmp.so.10", {
  // void __gmpz_init(mpz_t x)
  __gmpz_init: { args: [FFIType.ptr], returns: FFIType.void },
  // int __gmpz_init_set_str(mpz_t x, const char *str, int base)
  __gmpz_init_set_str: { args: [FFIType.ptr, FFIType.cstring, FFIType.i32], returns: FFIType.i32 },
  // void __gmpz_clear(mpz_t x)
  __gmpz_clear: { args: [FFIType.ptr], returns: FFIType.void },
  // void __gmpz_add(mpz_t r, mpz_t a, mpz_t b)
  __gmpz_add: { args: [FFIType.ptr, FFIType.ptr, FFIType.ptr], returns: FFIType.void },
  // void __gmpz_sub(mpz_t r, mpz_t a, mpz_t b)
  __gmpz_sub: { args: [FFIType.ptr, FFIType.ptr, FFIType.ptr], returns: FFIType.void },
  // void __gmpz_mul(mpz_t r, mpz_t a, mpz_t b)
  __gmpz_mul: { args: [FFIType.ptr, FFIType.ptr, FFIType.ptr], returns: FFIType.void },
  // void __gmpz_tdiv_q(mpz_t q, mpz_t a, mpz_t b)
  __gmpz_tdiv_q: { args: [FFIType.ptr, FFIType.ptr, FFIType.ptr], returns: FFIType.void },
  // void __gmpz_mod(mpz_t r, mpz_t a, mpz_t b)
  __gmpz_mod: { args: [FFIType.ptr, FFIType.ptr, FFIType.ptr], returns: FFIType.void },
  // int __gmpz_cmp(mpz_t a, mpz_t b)
  __gmpz_cmp: { args: [FFIType.ptr, FFIType.ptr], returns: FFIType.i32 },
  // int __gmpz_sgn(mpz_t x) is a macro; the sign is read through cmp_ui instead.
  // int __gmpz_cmp_ui(mpz_t a, unsigned long b)
  __gmpz_cmp_ui: { args: [FFIType.ptr, FFIType.u64], returns: FFIType.i32 },
  // size_t __gmpz_sizeinbase(mpz_t x, int base)
  __gmpz_sizeinbase: { args: [FFIType.ptr, FFIType.i32], returns: FFIType.u64 },
  // char* __gmpz_get_str(char *buf, int base, mpz_t x)
  // With a non-NULL buf, GMP writes the string into it, so the buffer is allocated on the JS side
  // (a NULL buf would make GMP allocate and require its own free function).
  __gmpz_get_str: { args: [FFIType.ptr, FFIType.i32, FFIType.ptr], returns: FFIType.ptr },
  // void __gmpz_pow_ui(mpz_t r, mpz_t base, unsigned long exp)
  __gmpz_pow_ui: { args: [FFIType.ptr, FFIType.ptr, FFIType.u64], returns: FFIType.void },
  // int __gmpz_probab_prime_p(mpz_t n, int reps)
  __gmpz_probab_prime_p: { args: [FFIType.ptr, FFIType.i32], returns: FFIType.i32 },
});

const sym = lib.symbols;

// ── Internal helpers ──────────────────────────────────────────────────────────

/** Allocates an uninitialised mpz_t (16 bytes). */
function allocMpz(): Uint8Array {
  return new Uint8Array(16);
}

/** Encodes a JS string as a NUL-terminated byte array (GMP expects ASCII). */
function encodeStr(s: string): Uint8Array {
  const bytes = new TextEncoder().encode(s);
  const buf = new Uint8Array(bytes.length + 1);
  buf.set(bytes);
  return buf;
}

// ── Public class ──────────────────────────────────────────────────────────────

/**
 * Arbitrary-precision integer backed by libgmp.
 *
 * Named `GmpInt` so that it does not shadow the JS builtin `BigInt`.
 *
 * Lifecycle:
 *   - The constructor is private; use the factories `GmpInt.fromString` /
 *     `GmpInt.fromBigInt`.
 *   - Call `.free()` once the object is no longer needed to release the internal GMP memory.
 *     `.free()` is idempotent and is the only way the GMP limbs are released.
 *   - Every binary operation (`add`, `mul`, ...) returns a new `GmpInt`; operands are not mutated.
 */
export class GmpInt {
  /** Opaque buffer holding the internal `mpz_t` (16 bytes). */
  readonly _mpz: Uint8Array;
  private _freed = false;

  private constructor(mpz: Uint8Array) {
    this._mpz = mpz;
  }

  // ── Factories ──────────────────────────────────────────────────────────────

  /**
   * Creates a `GmpInt` from a string in the given base.
   * @param s    String representing the integer (a leading `-` is allowed).
   * @param base Base from 2 to 62 (default 10).
   */
  static fromString(s: string, base = 10): GmpInt {
    const mpz = allocMpz();
    const cstr = encodeStr(s);
    const rc = sym.__gmpz_init_set_str(ptr(mpz), ptr(cstr), base);
    if (rc !== 0) {
      sym.__gmpz_clear(ptr(mpz));
      throw new Error(`GmpInt.fromString: invalid string "${s}" in base ${base}`);
    }
    return new GmpInt(mpz);
  }

  /** Creates a `GmpInt` from a native JS `bigint`. */
  static fromBigInt(n: bigint): GmpInt {
    return GmpInt.fromString(n.toString(10), 10);
  }

  // ── Binary operations ──────────────────────────────────────────────────────

  private _assertAlive(tag: string): void {
    if (this._freed) throw new Error(`GmpInt.${tag}: object already freed`);
  }

  private _binOp(
    other: GmpInt,
    fn: (r: number, a: number, b: number) => void,
    tag: string,
  ): GmpInt {
    this._assertAlive(tag);
    other._assertAlive(tag);
    const result = allocMpz();
    sym.__gmpz_init(ptr(result));
    fn(ptr(result), ptr(this._mpz), ptr(other._mpz));
    return new GmpInt(result);
  }

  /** Division-like operations abort the process in GMP when the divisor is zero. */
  private _assertNonZero(divisor: GmpInt, tag: string): void {
    divisor._assertAlive(tag);
    if (sym.__gmpz_cmp_ui(ptr(divisor._mpz), 0n) === 0) {
      throw new Error(`GmpInt.${tag}: division by zero`);
    }
  }

  /** Addition. */
  add(other: GmpInt): GmpInt {
    return this._binOp(other, sym.__gmpz_add.bind(sym), "add");
  }

  /** Subtraction. */
  sub(other: GmpInt): GmpInt {
    return this._binOp(other, sym.__gmpz_sub.bind(sym), "sub");
  }

  /** Multiplication. */
  mul(other: GmpInt): GmpInt {
    return this._binOp(other, sym.__gmpz_mul.bind(sym), "mul");
  }

  /** Integer division (truncated toward zero). */
  div(other: GmpInt): GmpInt {
    this._assertNonZero(other, "div");
    return this._binOp(other, sym.__gmpz_tdiv_q.bind(sym), "div");
  }

  /** Modulo (`mpz_mod`: the result is always non-negative). */
  mod(other: GmpInt): GmpInt {
    this._assertNonZero(other, "mod");
    return this._binOp(other, sym.__gmpz_mod.bind(sym), "mod");
  }

  // ── Unary operations ───────────────────────────────────────────────────────

  /** Power: `this ** exp` (exp >= 0). */
  pow(exp: number): GmpInt {
    this._assertAlive("pow");
    if (exp < 0 || !Number.isInteger(exp)) {
      throw new Error("GmpInt.pow: exp must be an integer >= 0");
    }
    const result = allocMpz();
    sym.__gmpz_init(ptr(result));
    sym.__gmpz_pow_ui(ptr(result), ptr(this._mpz), BigInt(exp));
    return new GmpInt(result);
  }

  // ── Comparison ─────────────────────────────────────────────────────────────

  /**
   * Compares `this` and `other`.
   * @returns -1 if this < other, 0 if equal, +1 if this > other.
   */
  cmp(other: GmpInt): number {
    this._assertAlive("cmp");
    other._assertAlive("cmp");
    const raw = sym.__gmpz_cmp(ptr(this._mpz), ptr(other._mpz));
    return raw < 0 ? -1 : raw > 0 ? 1 : 0;
  }

  // ── Tests ──────────────────────────────────────────────────────────────────

  /**
   * Probabilistic Miller-Rabin primality test.
   * @param reps Number of rounds (default 15; 15 or more gives a false-positive rate below 4^-reps).
   * @returns `true` if probably prime, `false` if certainly composite.
   */
  isProbablyPrime(reps = 15): boolean {
    this._assertAlive("isProbablyPrime");
    return sym.__gmpz_probab_prime_p(ptr(this._mpz), reps) > 0;
  }

  // ── Conversion ─────────────────────────────────────────────────────────────

  /** String representation in the given base (2 to 62, default 10). */
  toString(base = 10): string {
    this._assertAlive("toString");
    if (base < 2 || base > 62 || !Number.isInteger(base)) {
      throw new Error("GmpInt.toString: base must be an integer between 2 and 62");
    }
    // mpz_sizeinbase may overestimate by one; add room for the sign and the NUL terminator.
    const size = Number(sym.__gmpz_sizeinbase(ptr(this._mpz), base)) + 2;
    const buf = new Uint8Array(size);
    sym.__gmpz_get_str(ptr(buf), base, ptr(this._mpz));
    const end = buf.indexOf(0);
    return new TextDecoder().decode(end === -1 ? buf : buf.subarray(0, end));
  }

  /** Converts to a native JS `bigint` (through the base-10 string). */
  toBigInt(): bigint {
    return BigInt(this.toString(10));
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  /** Releases the internal GMP memory. Idempotent. */
  free(): void {
    if (this._freed) return;
    sym.__gmpz_clear(ptr(this._mpz));
    this._freed = true;
  }
}
