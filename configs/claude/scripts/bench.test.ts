import { expect, test } from "bun:test";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  type BenchFile,
  casesFromHooks,
  compareTable,
  fromHyperfine,
  payloadFor,
  summarizeEvents,
} from "./bench.ts";
import { toSlash } from "./lib.ts";

const hooks = {
  hooks: {
    SessionStart: [
      {
        matcher: "startup",
        hooks: [
          {
            type: "command",
            command:
              'bun "${CLAUDE_PLUGIN_ROOT}/hooks-handlers/a.ts" pull claude',
          },
        ],
      },
    ],
    PostToolUse: [
      {
        matcher: "Edit|Write",
        hooks: [
          {
            type: "command",
            command: 'bun "${CLAUDE_PLUGIN_ROOT}/hooks-handlers/b.ts" x',
          },
          { type: "prompt" },
        ],
      },
    ],
  },
};

test("casesFromHooks expands plugin root, ids and payloads", () => {
  const cases = casesFromHooks(
    "p",
    join(tmpdir(), "plugin-root", "p"),
    hooks,
    "/cwd",
  );
  expect(cases.map((c) => c.id)).toEqual([
    "p/sessionstart/a-claude",
    "p/posttooluse/b-x",
  ]);
  expect(cases[0]!.command).toContain(
    `${toSlash(join(tmpdir(), "plugin-root", "p"))}/hooks-handlers/a.ts`,
  );
  expect(cases[1]!.payload.tool_name).toBe("Edit");
});

test("payloadFor shapes each event", () => {
  expect(payloadFor("UserPromptSubmit", "", "/c").prompt).toBe(
    "benchmark prompt",
  );
  expect(payloadFor("SessionStart", "", "/c").source).toBe("startup");
  expect(payloadFor("PreToolUse", "", "/c").tool_name).toBe("Bash");
});

test("fromHyperfine reads schema 2 and schema 1 exports", () => {
  const c = casesFromHooks("p", "/r", hooks, "/cwd")[0]!;
  const v2 = fromHyperfine(c, {
    command: "x",
    summary: {
      time_wall_clock: {
        count: 5,
        mean: 0.0123456,
        median: 0.012,
        stddev: 0.001,
        min: 0.011,
        max: 0.014,
      },
    },
  });
  expect(v2).toMatchObject({ meanMs: 12.35, runs: 5, minMs: 11 });
  const v1 = fromHyperfine(c, {
    command: "x",
    mean: 0.02,
    median: 0.02,
    stddev: 0,
    min: 0.02,
    max: 0.02,
    times: [0.02, 0.02],
  });
  expect(v1).toMatchObject({ meanMs: 20, runs: 2 });
});

test("summarizeEvents gates wall time on the slowest hook and sums totals", () => {
  const mk = (event: string, meanMs: number) => ({
    id: "",
    plugin: "",
    event,
    command: "",
    meanMs,
    medianMs: 0,
    stddevMs: 0,
    minMs: 0,
    maxMs: 0,
    runs: 1,
  });
  expect(summarizeEvents([mk("A", 10), mk("A", 30), mk("B", 5)])).toEqual([
    { event: "A", processes: 2, wallMs: 30, totalMs: 40 },
    { event: "B", processes: 1, wallMs: 5, totalMs: 5 },
  ]);
});

test("compareTable lines up events", () => {
  const f = (wall: number, procs: number): BenchFile => ({
    label: "",
    date: "",
    bun: "",
    hyperfine: "",
    baseline: {} as never,
    cases: [],
    events: [
      { event: "E", processes: procs, wallMs: wall, totalMs: wall * procs },
    ],
  });
  expect(compareTable(f(100, 3), f(10, 2))).toContain(
    "| E | 100.0 | 10.0 | 3 | 2 | 300.0 | 20.0 |",
  );
});
