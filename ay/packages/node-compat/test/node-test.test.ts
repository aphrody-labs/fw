// Meta-test: the module itself is used to test the adapter.
// It is intentionally a "dogfooding" test: if the adapter is broken, the tests fail in an
// observable way (the calls are not registered correctly in bun:test).
//
// Assertions about internal behaviour (t.assert, etc.) also use the native bun:test imports
// where needed to avoid recursion.

import nodeTest, { test, describe, it, before, after, beforeEach, afterEach, TestContext } from "../src/node-test.ts";
import { expect } from "bun:test";
import * as nodeAssert from "node:assert";

// ---------------------------------------------------------------------------
// Test 1: test() with a synchronous callback passes
// ---------------------------------------------------------------------------
test("test() with a synchronous callback passes", (_t) => {
  // Reaching this point without a throw means the test passed.
  const x = 1 + 1;
  if (x !== 2) throw new Error("arithmetic broken");
});

// ---------------------------------------------------------------------------
// Test 2: test() with an async callback passes
// ---------------------------------------------------------------------------
test("test() with an async callback passes", async (_t) => {
  const result = await Promise.resolve(42);
  if (result !== 42) throw new Error("promise broken");
});

// ---------------------------------------------------------------------------
// Test 3: it() is an alias of test()
// ---------------------------------------------------------------------------
it("it() is an alias of test()", (_t) => {
  // it and test point to the same function: if both are declared here and
  // bun test runs them, the alias works.
  expect(it).toBe(test);
});

// ---------------------------------------------------------------------------
// Tests 4-8 inside a describe() to test the grouping
// ---------------------------------------------------------------------------
describe("describe() group: internal tests of the adapter", () => {
  // Test 4: describe() groups tests
  test("describe() groups tests correctly", (_t) => {
    // Being inside a describe() and running proves the grouping.
    expect(true).toBe(true);
  });

  // Test 5: the callback receives an object t with t.assert
  test("the callback receives an object t with t.assert", (t) => {
    expect(t).toBeInstanceOf(TestContext);
    expect(t.assert).toBeDefined();
    expect(typeof t.assert.strictEqual).toBe("function");
  });

  // Test 6: t.assert.strictEqual(1, 1) does not throw
  test("t.assert.strictEqual(1, 1) does not throw", (t) => {
    // Must pass without an exception
    t.assert.strictEqual(1, 1);
    t.assert.strictEqual("foo", "foo");
  });

  // Test 7: t.assert.strictEqual(1, 2) throws an AssertionError
  test("t.assert.strictEqual(1, 2) throws an AssertionError", (t) => {
    let threw = false;
    try {
      t.assert.strictEqual(1, 2);
    } catch (err) {
      threw = true;
      expect(err).toBeInstanceOf(nodeAssert.AssertionError);
    }
    expect(threw).toBe(true);
  });

  // Test 8: t.diagnostic("msg") does not throw
  test("t.diagnostic() does not throw", (t) => {
    // diagnostic writes to stdout: only check that it does not throw
    t.diagnostic("diagnostic test message");
    t.diagnostic("");
  });

  test("the callback receives a context named after the test", (t) => {
    expect(t.name).toBe("the callback receives a context named after the test");
  });

  test("t.skip() throws a SkipError carrying the message", (t) => {
    expect(() => t.skip("not today")).toThrow("test skipped: not today");
    try {
      t.skip();
    } catch (err) {
      expect((err as Error).name).toBe("SkipError");
      expect((err as Error).message).toBe("test skipped");
    }
  });

  test("t.todo() prints a TODO comment and does not throw", (t) => {
    expect(() => t.todo("later")).not.toThrow();
    expect(() => t.todo()).not.toThrow();
  });
});

// ---------------------------------------------------------------------------
// Bonus: lifecycle hooks (before/after/beforeEach/afterEach)
// These hooks must register without error.
// ---------------------------------------------------------------------------
describe("node:test lifecycle hooks (before/after/beforeEach/afterEach)", () => {
  let log: string[] = [];

  before(() => { log.push("before"); });
  after(() => { log.push("after"); });
  beforeEach(() => { log.push("beforeEach"); });
  afterEach(() => { log.push("afterEach"); });

  test("before() ran before this test", (_t) => {
    expect(log).toContain("before");
  });

  test("beforeEach() runs before each test", (_t) => {
    expect(log).toContain("beforeEach");
  });

  test("afterEach() ran after the previous tests", (_t) => {
    expect(log).toContain("afterEach");
  });
});

describe("default export", () => {
  test("exposes the same functions as the named exports", (_t) => {
    expect(nodeTest.test).toBe(test);
    expect(nodeTest.it).toBe(it);
    expect(nodeTest.describe).toBe(describe);
    expect(nodeTest.suite).toBe(describe);
    expect(nodeTest.TestContext).toBe(TestContext);
  });
});
