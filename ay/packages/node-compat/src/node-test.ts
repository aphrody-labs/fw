// Adapter node:test -> bun:test.
//
// `node:test` (Node.js v18+) has an API incompatible with `bun:test`.
// This module provides a light shim that redirects node:test calls to the equivalent
// bun:test primitives, so that code written for node:test runs unchanged under `bun test`.
//
// Implemented APIs:
//   test(name, fn)          - single test (fn receives a TestContext t)
//   it(name, fn)            - alias of test
//   describe(name, fn)      - test suite (alias: suite)
//   suite(name, fn)         - alias of describe
//   before(fn)              - beforeAll hook
//   after(fn)               - afterAll hook
//   beforeEach(fn)
//   afterEach(fn)
//
// The TestContext object `t` passed to the callbacks:
//   t.assert               - the complete node:assert module
//   t.diagnostic(msg)      - prints a TAP comment "# msg"
//   t.skip(msg?)           - marks the test as skipped (best effort)
//   t.todo(msg?)           - marks the test as todo (best effort)
//   t.name                 - name of the test

import {
  test as bunTest,
  describe as bunDescribe,
  beforeAll,
  afterAll,
  beforeEach,
  afterEach,
} from "bun:test";
import * as assert from "node:assert";

// ---------------------------------------------------------------------------
// TestContext: the `t` object passed to node:test callbacks
// ---------------------------------------------------------------------------

export class TestContext {
  readonly name: string;
  readonly assert: typeof assert = assert;

  constructor(name: string) {
    this.name = name;
  }

  /** Prints a TAP diagnostic message ("# message"). */
  diagnostic(msg: string): void {
    console.log(`  # ${msg}`);
  }

  /**
   * Marks the current test as skipped.
   * bun:test has no runtime equivalent, so this throws to signal the intent to the runner
   * (the test is counted as failed, not skipped). To be revisited once bun:test exposes a
   * runtime skip.
   */
  skip(msg?: string): void {
    throw Object.assign(new Error(`test skipped${msg ? `: ${msg}` : ""}`), {
      name: "SkipError",
    });
  }

  /**
   * Marks the current test as todo.
   * Same constraint as skip(): bun:test has no runtime API for it yet.
   */
  todo(msg?: string): void {
    console.log(`  # TODO${msg ? `: ${msg}` : ""}`);
  }
}

// ---------------------------------------------------------------------------
// Public functions
// ---------------------------------------------------------------------------

/** `node:test` test: registers a single test. The callback receives a TestContext. */
export function test(
  name: string,
  fn: (t: TestContext) => void | Promise<void>,
): void {
  bunTest(name, () => fn(new TestContext(name)));
}

/** Alias of `test`, identical to the node:test API. */
export const it = test;

/** `node:test` describe: groups tests into a suite. */
export const describe = bunDescribe;

/** `node:test` suite: alias of describe. */
export const suite = bunDescribe;

/** `node:test` before: runs once before all the tests of the block. */
export const before = beforeAll;

/** `node:test` after: runs once after all the tests of the block. */
export const after = afterAll;

export { beforeEach, afterEach };

// ---------------------------------------------------------------------------
// Default export (import nodeTest from "@aphrody/ay-node-compat/node-test")
// ---------------------------------------------------------------------------

export default {
  test,
  it,
  describe: bunDescribe,
  suite: bunDescribe,
  before: beforeAll,
  after: afterAll,
  beforeEach,
  afterEach,
  TestContext,
};
