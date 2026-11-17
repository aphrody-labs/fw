import { describe, expect, test } from "bun:test";
import {
  CLOUDFLARE_OS_GATEKEEPERS,
  DurableWorkflowRun,
  decodeAiStreamChunk,
  encodeAiStreamPart,
  evalVirtualNyshPipeline,
} from "../src/index.ts";

describe("@aphrody/ay-edge-stack (Cloudflare-OS + Vercel Bun SDK)", () => {
  test("Cloudflare-OS gatekeepers follow ay-{unit}-{stage} naming", () => {
    expect(CLOUDFLARE_OS_GATEKEEPERS.length).toBeGreaterThanOrEqual(5);
    for (const gk of CLOUDFLARE_OS_GATEKEEPERS) {
      expect(gk.workerName).toMatch(/^ay-gatekeeper-[a-z0-9-]+-prod$/);
    }
  });

  test("AI stream SSE frames round-trip cleanly", () => {
    const raw =
      encodeAiStreamPart({ type: "text-delta", delta: "Bonjour " }) +
      encodeAiStreamPart({ type: "finish" });
    const parsed = decodeAiStreamChunk(raw);
    expect(parsed).toEqual([{ type: "text-delta", delta: "Bonjour " }, { type: "finish" }]);
  });

  test("DurableWorkflowRun memoizes completed steps deterministically", async () => {
    const wf = new DurableWorkflowRun("wf-01");
    let runs = 0;
    const a = await wf.step("compile", () => ++runs);
    const b = await wf.step("compile", () => ++runs);
    expect(a).toBe(1);
    expect(b).toBe(1);
    expect(wf.completedSteps()).toEqual(["compile"]);
  });

  test("virtual just-bash / nysh pipeline executes cat | grep | wc -l in memory", () => {
    const vfs = new Map([["ay.toml", "dioxus\ncloudflare-os\ndioxus-native\n"]]);
    const res = evalVirtualNyshPipeline("cat ay.toml | grep dioxus | wc -l", vfs);
    expect(res).toEqual({ stdout: "2\n", exitCode: 0 });
  });
});
