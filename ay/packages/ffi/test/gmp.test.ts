import { describe, test, expect } from "bun:test";
import { GmpInt } from "../src/gmp.ts";

describe("GmpInt (bun:ffi -> libgmp.so.10)", () => {
  // 1. fromString + toString round-trip
  test("fromString('123').toString() === '123'", () => {
    const n = GmpInt.fromString("123");
    expect(n.toString()).toBe("123");
    n.free();
  });

  // 2. Addition with big integers (50 x '9' + 1 = '1' + 50 x '0')
  test("high-precision addition: 999...9 (50 digits) + 1", () => {
    const nines = "9".repeat(50);
    const expected = "1" + "0".repeat(50);
    const a = GmpInt.fromString(nines);
    const one = GmpInt.fromString("1");
    const result = a.add(one);
    expect(result.toString()).toBe(expected);
    a.free();
    one.free();
    result.free();
  });

  // 3. Multiplication: 2^100 via pow
  test("pow: 2^100 via GmpInt.fromString('2').pow(100)", () => {
    const two = GmpInt.fromString("2");
    const r = two.pow(100);
    // 2^100 = 1267650600228229401496703205376
    expect(r.toString()).toBe("1267650600228229401496703205376");
    two.free();
    r.free();
  });

  // 4. Comparison: cmp returns the right sign
  test("cmp: -1 / 0 / +1", () => {
    const a = GmpInt.fromString("100");
    const b = GmpInt.fromString("200");
    const c = GmpInt.fromString("100");
    expect(a.cmp(b)).toBe(-1);
    expect(b.cmp(a)).toBe(1);
    expect(a.cmp(c)).toBe(0);
    a.free();
    b.free();
    c.free();
  });

  // 5. Division and modulo
  test("div and mod: 17 / 5 = 3 remainder 2", () => {
    const seventeen = GmpInt.fromString("17");
    const five = GmpInt.fromString("5");
    const q = seventeen.div(five);
    const r = seventeen.mod(five);
    expect(q.toString()).toBe("3");
    expect(r.toString()).toBe("2");
    seventeen.free();
    five.free();
    q.free();
    r.free();
  });

  // 6. fromBigInt round-trip
  test("fromBigInt(BigInt('12345678901234567890')).toString()", () => {
    const val = 12345678901234567890n;
    const n = GmpInt.fromBigInt(val);
    expect(n.toString()).toBe("12345678901234567890");
    expect(n.toBigInt()).toBe(val);
    n.free();
  });

  // 7. isProbablyPrime
  test("isProbablyPrime: 7 is prime, 4 is not", () => {
    const seven = GmpInt.fromString("7");
    const four = GmpInt.fromString("4");
    expect(seven.isProbablyPrime()).toBe(true);
    expect(four.isProbablyPrime()).toBe(false);
    seven.free();
    four.free();
  });

  // 8. free() is idempotent and does not throw
  test("free() is idempotent", () => {
    const n = GmpInt.fromString("42");
    expect(() => {
      n.free();
      n.free(); // second call must not crash
    }).not.toThrow();
  });

  // Bonus: subtraction with a negative result
  test("sub: 3 - 10 = -7", () => {
    const three = GmpInt.fromString("3");
    const ten = GmpInt.fromString("10");
    const r = three.sub(ten);
    expect(r.toString()).toBe("-7");
    three.free();
    ten.free();
    r.free();
  });

  // Bonus: toString in base 16
  test("toString(16): 255 -> 'ff'", () => {
    const n = GmpInt.fromString("255");
    expect(n.toString(16)).toBe("ff");
    n.free();
  });

  test("mul: 123456789 * 987654321 = 121932631112635269", () => {
    const a = GmpInt.fromString("123456789");
    const b = GmpInt.fromString("987654321");
    const r = a.mul(b);
    expect(r.toString()).toBe("121932631112635269");
    a.free();
    b.free();
    r.free();
  });

  test("div truncates toward zero: -17 / 5 = -3", () => {
    const a = GmpInt.fromString("-17");
    const b = GmpInt.fromString("5");
    const q = a.div(b);
    expect(q.toString()).toBe("-3");
    a.free();
    b.free();
    q.free();
  });

  test("mod result is non-negative: -7 mod 3 = 2", () => {
    const a = GmpInt.fromString("-7");
    const b = GmpInt.fromString("3");
    const r = a.mod(b);
    expect(r.toString()).toBe("2");
    a.free();
    b.free();
    r.free();
  });

  test("div and mod by zero throw instead of aborting the process", () => {
    const a = GmpInt.fromString("1");
    const zero = GmpInt.fromString("0");
    expect(() => a.div(zero)).toThrow("division by zero");
    expect(() => a.mod(zero)).toThrow("division by zero");
    a.free();
    zero.free();
  });

  test("toString of a number far beyond 8192 digits is complete", () => {
    const two = GmpInt.fromString("2");
    const big = two.pow(100_000); // 30103 decimal digits
    const text = big.toString();
    expect(text.length).toBe(30103);
    expect(text.startsWith("9990020930143845")).toBe(true);
    expect(big.toBigInt()).toBe(2n ** 100_000n);
    two.free();
    big.free();
  });

  test("fromString rejects an invalid string", () => {
    expect(() => GmpInt.fromString("12x4")).toThrow("invalid string");
  });

  test("pow rejects a negative or fractional exponent", () => {
    const two = GmpInt.fromString("2");
    expect(() => two.pow(-1)).toThrow("exp must be an integer >= 0");
    expect(() => two.pow(1.5)).toThrow("exp must be an integer >= 0");
    two.free();
  });

  test("toString rejects a base outside 2..62", () => {
    const n = GmpInt.fromString("5");
    expect(() => n.toString(1)).toThrow("base must be an integer between 2 and 62");
    expect(() => n.toString(63)).toThrow("base must be an integer between 2 and 62");
    n.free();
  });

  test("using a freed object throws", () => {
    const n = GmpInt.fromString("5");
    n.free();
    expect(() => n.toString()).toThrow("object already freed");
    expect(() => n.add(n)).toThrow("object already freed");
  });
});
